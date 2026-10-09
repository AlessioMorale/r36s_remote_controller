#pragma once

#include <QObject>
#include <QString>
#include <QJsonObject>

// UI configuration, loaded once from a JSON file (see config/rc_ui.json).
// Search order: --config <path>, $RC_UI_CONFIG, /etc/rc/ui.json, built-in defaults.
// The theme can be changed at runtime from the menu; the choice is persisted in
// QSettings (it overrides the file value on the next start).
class AppConfig : public QObject {
  Q_OBJECT
  Q_PROPERTY(QString socketPath READ socketPath CONSTANT)
  Q_PROPERTY(QString robotHost READ robotHost CONSTANT)
  Q_PROPERTY(int bridgePort READ bridgePort CONSTANT)
  Q_PROPERTY(bool lichtblickEnabled READ lichtblickEnabled CONSTANT)
  Q_PROPERTY(QString lichtblickUrl READ lichtblickUrl CONSTANT)
  Q_PROPERTY(int lichtblickLoadTimeoutMs READ lichtblickLoadTimeoutMs CONSTANT)
  Q_PROPERTY(int lichtblickPingIntervalMs READ lichtblickPingIntervalMs CONSTANT)
  Q_PROPERTY(int lichtblickHangTimeoutMs READ lichtblickHangTimeoutMs CONSTANT)
  Q_PROPERTY(bool webEngineAvailable READ webEngineAvailable CONSTANT)
  Q_PROPERTY(bool webrtcAvailable READ webrtcAvailable CONSTANT)
  Q_PROPERTY(QString videoSource READ videoSource CONSTANT)
  Q_PROPERTY(QString signallerUri READ signallerUri CONSTANT)
  Q_PROPERTY(QString webrtcStunServer READ webrtcStunServer CONSTANT)
  Q_PROPERTY(int webrtcStallMs READ webrtcStallMs CONSTANT)
  Q_PROPERTY(int webrtcConnectTimeoutMs READ webrtcConnectTimeoutMs CONSTANT)
  Q_PROPERTY(bool soundAvailable READ soundAvailable CONSTANT)
  Q_PROPERTY(bool soundEnabled READ soundEnabled CONSTANT)
  Q_PROPERTY(QString layout READ layout CONSTANT)
  Q_PROPERTY(bool fullscreen READ fullscreen CONSTANT)
  Q_PROPERTY(int staleUiMs READ staleUiMs CONSTANT)
  Q_PROPERTY(int daemonLostMs READ daemonLostMs CONSTANT)
  Q_PROPERTY(QString fontFamily READ fontFamily CONSTANT)
  Q_PROPERTY(QString theme READ theme WRITE setTheme NOTIFY themeChanged)
  Q_PROPERTY(QString uiVersion READ uiVersion CONSTANT)
  Q_PROPERTY(QString configSource READ configSource CONSTANT)

 public:
  explicit AppConfig(QObject* parent = nullptr);

  // Loads `path` (or the default search order when empty). Never fails: missing
  // or invalid files fall back to defaults; `error()` describes what happened.
  bool load(const QString& path);
  void loadFromJson(const QJsonObject& root);
  QString error() const { return m_error; }

  // Applies the theme persisted in QSettings (call after QCoreApplication exists).
  void restorePersistedTheme();

  QString socketPath() const { return m_socketPath; }
  int reconnectMs() const { return m_reconnectMs; }
  int requestTimeoutMs() const { return m_requestTimeoutMs; }

  QString robotHost() const { return m_robotHost; }
  int bridgePort() const { return m_bridgePort; }
  int probeIntervalMs() const { return m_probeIntervalMs; }
  int probeTimeoutMs() const { return m_probeTimeoutMs; }

  bool lichtblickEnabled() const { return m_lichtblickEnabled; }
  QString lichtblickUrl() const;  // template with {host} / {port} expanded
  QString chromiumFlags() const { return m_chromiumFlags; }
  int lichtblickLoadTimeoutMs() const { return m_lbLoadTimeoutMs; }
  int lichtblickPingIntervalMs() const { return m_lbPingIntervalMs; }
  int lichtblickHangTimeoutMs() const { return m_lbHangTimeoutMs; }

  QString wifiInterface() const { return m_wifiInterface; }
  QString zerotierCli() const { return m_zerotierCli; }
  QString zerotierNetwork() const { return m_zerotierNetwork; }
  QString robotNodeId() const { return m_robotNodeId; }
  int netPollMs() const { return m_netPollMs; }

  // "webrtc" (native view fed by the robot's webrtcsink), "lichtblick" or "none". The config value
  // "auto" resolves to webrtc when built with it, else lichtblick.
  QString videoSource() const;
  int signallerPort() const { return m_signallerPort; }
  QString signallerUri() const;  // ws://host:port
  QString webrtcStunServer() const { return m_webrtcStun; }
  int webrtcStallMs() const { return m_webrtcStallMs; }
  int webrtcConnectTimeoutMs() const { return m_webrtcConnectTimeoutMs; }
  // Port NetStatus probes: the one the active video source needs.
  int probePort() const;
  bool webrtcAvailable() const;
  bool webEngineAvailable() const;
  bool soundAvailable() const;
  bool soundEnabled() const { return m_soundEnabled && soundAvailable(); }
  void setSoundEnabled(bool on) { m_soundEnabled = on; }
  QString layout() const { return m_layout; }
  bool fullscreen() const { return m_fullscreen; }
  void setFullscreen(bool on) { m_fullscreen = on; }
  int staleUiMs() const { return m_staleUiMs; }
  int daemonLostMs() const { return m_daemonLostMs; }
  QString fontFamily() const { return m_fontFamily; }
  QString theme() const { return m_theme; }
  void setTheme(const QString& theme);           // persisted (menu toggle)
  void overrideTheme(const QString& theme);      // this run only (--theme)
  QString uiVersion() const;
  QString configSource() const { return m_source; }

  void setSocketPath(const QString& p) { m_socketPath = p; }

 signals:
  void themeChanged();

 private:
  QString m_error;
  QString m_source = QStringLiteral("defaults");

  QString m_socketPath = QStringLiteral("/run/rc/control.sock");
  int m_reconnectMs = 500;
  int m_requestTimeoutMs = 3000;

  QString m_robotHost = QStringLiteral("10.147.17.10");
  int m_bridgePort = 8765;
  int m_probeIntervalMs = 2000;
  int m_probeTimeoutMs = 1500;

  QString m_videoSource = QStringLiteral("auto");
  int m_signallerPort = 8443;
  QString m_webrtcStun;
  int m_webrtcStallMs = 3000;
  int m_webrtcConnectTimeoutMs = 10000;

  bool m_lichtblickEnabled = true;
  QString m_lichtblickUrlTemplate = QStringLiteral(
      "file:///opt/lichtblick/index.html?ds=foxglove-websocket&ds.url=ws%3A%2F%2F{host}%3A{port}&openIn=web");
  QString m_chromiumFlags;
  int m_lbLoadTimeoutMs = 30000;
  int m_lbPingIntervalMs = 3000;
  int m_lbHangTimeoutMs = 10000;

  QString m_wifiInterface = QStringLiteral("wlan0");
  QString m_zerotierCli = QStringLiteral("zerotier-cli");
  QString m_zerotierNetwork;
  QString m_robotNodeId;
  int m_netPollMs = 2000;

  bool m_soundEnabled = true;
  QString m_layout = QStringLiteral("B");
  bool m_fullscreen = true;
  int m_staleUiMs = 1000;
  int m_daemonLostMs = 1500;
  QString m_fontFamily;
  QString m_theme = QStringLiteral("dark");
};
