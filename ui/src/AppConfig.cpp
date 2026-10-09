#include "AppConfig.h"

#include <QFile>
#include <QJsonDocument>
#include <QSettings>

namespace {
QString str(const QJsonObject& o, const char* key, const QString& def) {
  const auto v = o.value(QLatin1String(key));
  return v.isString() ? v.toString() : def;
}
int num(const QJsonObject& o, const char* key, int def) {
  const auto v = o.value(QLatin1String(key));
  return v.isDouble() ? v.toInt() : def;
}
bool flag(const QJsonObject& o, const char* key, bool def) {
  const auto v = o.value(QLatin1String(key));
  return v.isBool() ? v.toBool() : def;
}
}  // namespace

AppConfig::AppConfig(QObject* parent) : QObject(parent) {}

bool AppConfig::load(const QString& path) {
  QStringList candidates;
  if (!path.isEmpty()) {
    candidates << path;
  } else {
    const QString env = qEnvironmentVariable("RC_UI_CONFIG");
    if (!env.isEmpty()) candidates << env;
    candidates << QStringLiteral("/etc/rc/ui.json");
  }
  for (const QString& c : candidates) {
    QFile f(c);
    if (!f.exists()) continue;
    if (!f.open(QIODevice::ReadOnly)) {
      m_error = QStringLiteral("cannot open %1").arg(c);
      continue;
    }
    QJsonParseError pe{};
    const auto doc = QJsonDocument::fromJson(f.readAll(), &pe);
    if (pe.error != QJsonParseError::NoError || !doc.isObject()) {
      m_error = QStringLiteral("%1: %2").arg(c, pe.errorString());
      continue;
    }
    loadFromJson(doc.object());
    m_source = c;
    return true;
  }
  if (!path.isEmpty() && m_error.isEmpty()) m_error = QStringLiteral("%1 not found").arg(path);
  return false;
}

void AppConfig::loadFromJson(const QJsonObject& root) {
  const auto ipc = root.value(QLatin1String("ipc")).toObject();
  m_socketPath = str(ipc, "socket_path", m_socketPath);
  m_reconnectMs = num(ipc, "reconnect_ms", m_reconnectMs);
  m_requestTimeoutMs = num(ipc, "request_timeout_ms", m_requestTimeoutMs);

  const auto robot = root.value(QLatin1String("robot")).toObject();
  m_robotHost = str(robot, "host", m_robotHost);
  m_bridgePort = num(robot, "bridge_port", m_bridgePort);
  m_probeIntervalMs = num(robot, "probe_interval_ms", m_probeIntervalMs);
  m_probeTimeoutMs = num(robot, "probe_timeout_ms", m_probeTimeoutMs);

  m_signallerPort = num(robot, "signaller_port", m_signallerPort);

  const auto video = root.value(QLatin1String("video")).toObject();
  m_videoSource = str(video, "source", m_videoSource);
  m_webrtcStun = str(video, "stun_server", m_webrtcStun);
  m_webrtcStallMs = num(video, "stall_ms", m_webrtcStallMs);
  m_webrtcConnectTimeoutMs = num(video, "connect_timeout_ms", m_webrtcConnectTimeoutMs);

  const auto lb = root.value(QLatin1String("lichtblick")).toObject();
  m_lichtblickEnabled = flag(lb, "enabled", m_lichtblickEnabled);
  m_lichtblickUrlTemplate = str(lb, "url", m_lichtblickUrlTemplate);
  m_chromiumFlags = str(lb, "chromium_flags", m_chromiumFlags);
  m_lbLoadTimeoutMs = num(lb, "load_timeout_ms", m_lbLoadTimeoutMs);
  m_lbPingIntervalMs = num(lb, "ping_interval_ms", m_lbPingIntervalMs);
  m_lbHangTimeoutMs = num(lb, "hang_timeout_ms", m_lbHangTimeoutMs);

  const auto net = root.value(QLatin1String("net")).toObject();
  m_wifiInterface = str(net, "wifi_interface", m_wifiInterface);
  m_zerotierCli = str(net, "zerotier_cli", m_zerotierCli);
  m_zerotierNetwork = str(net, "zerotier_network", m_zerotierNetwork);
  m_robotNodeId = str(net, "robot_node_id", m_robotNodeId);
  m_netPollMs = num(net, "poll_ms", m_netPollMs);

  const auto ui = root.value(QLatin1String("ui")).toObject();
  m_theme = str(ui, "theme", m_theme);
  m_layout = str(ui, "layout", m_layout);
  m_fullscreen = flag(ui, "fullscreen", m_fullscreen);
  m_soundEnabled = flag(ui, "sound", m_soundEnabled);
  m_staleUiMs = num(ui, "stale_ui_ms", m_staleUiMs);
  m_daemonLostMs = num(ui, "daemon_lost_ms", m_daemonLostMs);
  m_fontFamily = str(ui, "font_family", m_fontFamily);
}

void AppConfig::restorePersistedTheme() {
  QSettings s;
  const QString t = s.value(QStringLiteral("ui/theme")).toString();
  if (t == QLatin1String("dark") || t == QLatin1String("light")) {
    m_theme = t;
    emit themeChanged();
  }
}

void AppConfig::setTheme(const QString& theme) {
  if (theme == m_theme) return;
  m_theme = theme;
  QSettings s;
  s.setValue(QStringLiteral("ui/theme"), theme);
  emit themeChanged();
}

void AppConfig::overrideTheme(const QString& theme) {
  if (theme == m_theme) return;
  m_theme = theme;
  emit themeChanged();
}

QString AppConfig::lichtblickUrl() const {
  QString u = m_lichtblickUrlTemplate;
  u.replace(QLatin1String("{host}"), m_robotHost);
  u.replace(QLatin1String("{port}"), QString::number(m_bridgePort));
  return u;
}

QString AppConfig::videoSource() const {
  if (m_videoSource == QLatin1String("none") || m_videoSource == QLatin1String("lichtblick") ||
      m_videoSource == QLatin1String("webrtc"))
    return m_videoSource;
  return webrtcAvailable() ? QStringLiteral("webrtc") : QStringLiteral("lichtblick");
}

QString AppConfig::signallerUri() const {
  return QStringLiteral("ws://%1:%2").arg(m_robotHost).arg(m_signallerPort);
}

int AppConfig::probePort() const {
  return videoSource() == QLatin1String("webrtc") ? m_signallerPort : m_bridgePort;
}

bool AppConfig::webrtcAvailable() const { return RC_UI_HAS_WEBRTC; }
bool AppConfig::webEngineAvailable() const { return RC_UI_HAS_WEBENGINE; }
bool AppConfig::soundAvailable() const { return RC_UI_HAS_SOUND; }
QString AppConfig::uiVersion() const { return QStringLiteral(RC_UI_VERSION); }
