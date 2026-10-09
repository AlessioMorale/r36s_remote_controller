// Native handheld UI host (design §3.2). Thin C++: IPC client, models, and
// network status, exposed to QML as singletons in the `RcBackend` module.
#include <QCommandLineParser>
#include <QDir>
#include <QFontDatabase>
#include <QElapsedTimer>
#include <QGuiApplication>
#include <QLoggingCategory>
#include <QQmlApplicationEngine>
#include <QQuickWindow>
#include <QTimer>

#if RC_UI_HAS_WEBENGINE
#include <QtWebEngineQuick/qtwebenginequickglobal.h>
#endif

#include "AlarmModel.h"
#include "AppConfig.h"
#include "HostBattery.h"
#include "IpcClient.h"
#include "NetStatus.h"
#include "ParamModel.h"
#include "TelemetryModel.h"
#include "WebrtcVideo.h"

Q_LOGGING_CATEGORY(lcMain, "rc.main")

namespace {
// --config must be known before QGuiApplication exists (Chromium flags are
// read by QtWebEngine at initialization), so scan argv by hand.
QString preScanConfig(int argc, char** argv) {
  for (int i = 1; i < argc; ++i) {
    const QByteArray a(argv[i]);
    if (a == "--config" && i + 1 < argc) return QString::fromLocal8Bit(argv[i + 1]);
    if (a.startsWith("--config=")) return QString::fromLocal8Bit(a.mid(9));
  }
  return {};
}
}  // namespace

int main(int argc, char** argv) {
  auto* cfg = new AppConfig;
  const bool loaded = cfg->load(preScanConfig(argc, argv));

#if RC_UI_HAS_WEBENGINE
  if (!cfg->chromiumFlags().isEmpty()) {
    QByteArray flags = qgetenv("QTWEBENGINE_CHROMIUM_FLAGS");
    flags += ' ' + cfg->chromiumFlags().toLocal8Bit();
    qputenv("QTWEBENGINE_CHROMIUM_FLAGS", flags.trimmed());
  }
  QtWebEngineQuick::initialize();
#endif

  QGuiApplication app(argc, argv);
  QCoreApplication::setOrganizationName(QStringLiteral("pizzarobotics"));
  QCoreApplication::setApplicationName(QStringLiteral("rc_ui"));
  QCoreApplication::setApplicationVersion(cfg->uiVersion());
  cfg->setParent(&app);

  // Brand fonts (Slamming Works style guide): Archivo Black, IBM Plex Sans and Mono, bundled so the
  // handheld needs nothing installed. Theme.qml selects them by family name.
  for (const char* f : {"ArchivoBlack-Regular", "IBMPlexSans-Regular", "IBMPlexSans-SemiBold",
                        "IBMPlexMono-Regular", "IBMPlexMono-SemiBold"}) {
    const QString path = QStringLiteral(":/qt/qml/RcUi/resources/fonts/%1.ttf").arg(QLatin1String(f));
    if (QFontDatabase::addApplicationFont(path) < 0) qCWarning(lcMain) << "font not loaded" << path;
  }

  QCommandLineParser parser;
  parser.setApplicationDescription(QStringLiteral("Slamming Works handheld ground station UI"));
  parser.addHelpOption();
  parser.addVersionOption();
  parser.addOption({QStringLiteral("config"), QStringLiteral("Config file (JSON)."), QStringLiteral("path")});
  parser.addOption({QStringLiteral("socket"), QStringLiteral("Override ipc.socket_path."), QStringLiteral("path")});
  parser.addOption({QStringLiteral("windowed"), QStringLiteral("Do not go fullscreen (desktop testing).")});
  parser.addOption({QStringLiteral("mute"), QStringLiteral("Disable the alarm tone.")});
  parser.addOption({QStringLiteral("theme"), QStringLiteral("Force theme: dark or light."), QStringLiteral("name")});
  parser.addOption({QStringLiteral("screenshot"),
                    QStringLiteral("Grab the window to <path> after --screenshot-delay ms, then exit."),
                    QStringLiteral("path")});
  parser.addOption({QStringLiteral("screenshot-delay"), QStringLiteral("Delay before the grab (default 3000)."),
                    QStringLiteral("ms"), QStringLiteral("3000")});
  parser.process(app);

  if (!loaded) qCInfo(lcMain) << "using built-in config defaults" << cfg->error();
  else qCInfo(lcMain) << "config:" << cfg->configSource();
  if (parser.isSet(QStringLiteral("socket"))) cfg->setSocketPath(parser.value(QStringLiteral("socket")));
  if (parser.isSet(QStringLiteral("windowed"))) cfg->setFullscreen(false);
  if (parser.isSet(QStringLiteral("mute"))) cfg->setSoundEnabled(false);
  cfg->restorePersistedTheme();
  if (parser.isSet(QStringLiteral("theme"))) cfg->overrideTheme(parser.value(QStringLiteral("theme")));

  IpcClient ipc(cfg->socketPath(), cfg->reconnectMs(), cfg->requestTimeoutMs());
  TelemetryModel telemetry(cfg->staleUiMs());
  AlarmModel alarms;
  ParamModel params;
  NetStatus net(cfg);
  HostBattery host;
  WebrtcVideo webrtc;

  QObject::connect(&ipc, &IpcClient::telemetryReceived, &telemetry, &TelemetryModel::update);
  QObject::connect(&ipc, &IpcClient::alarmReceived, &alarms, &AlarmModel::apply);
  QObject::connect(&ipc, &IpcClient::paramsReceived, &params, &ParamModel::setTree);
  QObject::connect(&ipc, &IpcClient::connectedChanged, &app, [&] {
    if (ipc.isConnected()) return;
    telemetry.connectionLost();
    alarms.clearDaemonAlarms();
    params.clear();
  });

  // UI-local loud alarm: the daemon is gone or silent (no snapshot for daemonLostMs).
  QElapsedTimer lastSnapshot;
  lastSnapshot.start();
  QObject::connect(&ipc, &IpcClient::telemetryReceived, &app, [&] { lastSnapshot.restart(); });
  QTimer daemonWatch;
  daemonWatch.setInterval(200);
  QObject::connect(&daemonWatch, &QTimer::timeout, &app, [&] {
    const bool lost = lastSnapshot.elapsed() > cfg->daemonLostMs();
    alarms.setLocal(QString::fromLatin1(AlarmModel::kDaemonLost), QStringLiteral("loud"),
                    ipc.isConnected() ? QStringLiteral("Control daemon silent")
                                      : QStringLiteral("Control daemon not connected"),
                    lost);
  });
  daemonWatch.start();

  qmlRegisterSingletonInstance("RcBackend", 1, 0, "AppConfig", cfg);
  qmlRegisterSingletonInstance("RcBackend", 1, 0, "Ipc", &ipc);
  qmlRegisterSingletonInstance("RcBackend", 1, 0, "Telemetry", &telemetry);
  qmlRegisterSingletonInstance("RcBackend", 1, 0, "Alarms", &alarms);
  qmlRegisterSingletonInstance("RcBackend", 1, 0, "Params", &params);
  qmlRegisterSingletonInstance("RcBackend", 1, 0, "Net", &net);
  qmlRegisterSingletonInstance("RcBackend", 1, 0, "Host", &host);
  qmlRegisterSingletonInstance("RcBackend", 1, 0, "Webrtc", &webrtc);

  QQmlApplicationEngine engine;
  QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app,
                   [] { QCoreApplication::exit(2); }, Qt::QueuedConnection);
  engine.load(QUrl(QStringLiteral("qrc:/qt/qml/RcUi/qml/Main.qml")));
  if (engine.rootObjects().isEmpty()) return 2;

  ipc.start();
  net.start();

  if (parser.isSet(QStringLiteral("screenshot"))) {
    const QString out = parser.value(QStringLiteral("screenshot"));
    const int delay = parser.value(QStringLiteral("screenshot-delay")).toInt();
    QTimer::singleShot(delay, &app, [&engine, out] {
      auto* win = qobject_cast<QQuickWindow*>(engine.rootObjects().constFirst());
      const QImage img = win ? win->grabWindow() : QImage();
      QDir().mkpath(QFileInfo(out).absolutePath());
      const bool ok = !img.isNull() && img.save(out);
      qCInfo(lcMain) << "screenshot" << out << (ok ? "saved" : "FAILED") << img.size();
      QCoreApplication::exit(ok ? 0 : 3);
    });
  }

  return app.exec();
}
