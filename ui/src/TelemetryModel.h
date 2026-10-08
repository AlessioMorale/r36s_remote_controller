#pragma once

#include <QElapsedTimer>
#include <QJsonObject>
#include <QObject>
#include <QTimer>
#include <QVariantList>
#include <QVariantMap>

// The latest `telemetry` snapshot (docs/ipc.md), exposed to QML group by group.
// Groups are plain maps that mirror the JSON (QML reads Telemetry.link.lq).
// A group that was never received is an empty map, so every field reads as
// `undefined` and the UI shows "—".
//
// Staleness: each group's own `stale` flag comes from the daemon. On top of
// that the UI marks everything stale when no snapshot arrived for staleUiMs
// (`fresh` false), e.g. the daemon hung or the socket dropped.
class TelemetryModel : public QObject {
  Q_OBJECT
  Q_PROPERTY(QVariantMap link READ link NOTIFY updated)
  Q_PROPERTY(QVariantMap battery READ battery NOTIFY updated)
  Q_PROPERTY(QVariantMap status READ status NOTIFY updated)
  Q_PROPERTY(QVariantMap arm READ arm NOTIFY updated)
  Q_PROPERTY(QVariantMap input READ input NOTIFY updated)
  Q_PROPERTY(QVariantMap tx READ tx NOTIFY updated)
  Q_PROPERTY(QVariantMap module READ module NOTIFY updated)
  Q_PROPERTY(QVariantList overrides READ overrides NOTIFY updated)
  Q_PROPERTY(QVariantList channels READ channels NOTIFY updated)
  Q_PROPERTY(qint64 seq READ seq NOTIFY updated)
  Q_PROPERTY(bool valid READ valid NOTIFY updated)
  Q_PROPERTY(bool fresh READ fresh NOTIFY freshChanged)
  Q_PROPERTY(bool menuMode READ menuMode NOTIFY menuModeChanged)
  Q_PROPERTY(int snapshotAgeMs READ snapshotAgeMs NOTIFY ageTick)

 public:
  explicit TelemetryModel(int staleUiMs = 1000, QObject* parent = nullptr);

  // Parses one `telemetry` message. Returns false if it is not one.
  bool update(const QJsonObject& msg);
  // Called when the IPC connection drops: keeps the last values, marks not fresh.
  void connectionLost();

  QVariantMap link() const { return m_link; }
  QVariantMap battery() const { return m_battery; }
  QVariantMap status() const { return m_status; }
  QVariantMap arm() const { return m_arm; }
  QVariantMap input() const { return m_input; }
  QVariantMap tx() const { return m_tx; }
  QVariantMap module() const { return m_module; }
  QVariantList overrides() const { return m_overrides; }
  QVariantList channels() const { return m_channels; }
  qint64 seq() const { return m_seq; }
  bool valid() const { return m_valid; }
  bool fresh() const { return m_fresh; }
  bool menuMode() const { return m_menuMode; }
  int snapshotAgeMs() const;

  // Field is present and the daemon does not flag it stale, and the snapshot is fresh.
  Q_INVOKABLE bool isLive(const QString& group) const;

 signals:
  void updated();
  void freshChanged();
  void menuModeChanged();
  void ageTick();

 private:
  void setFresh(bool f);
  void tick();

  QVariantMap m_link, m_battery, m_status, m_arm, m_input, m_tx, m_module;
  QVariantList m_overrides, m_channels;
  qint64 m_seq = -1;
  bool m_valid = false;
  bool m_fresh = false;
  bool m_menuMode = false;
  int m_staleUiMs;
  QElapsedTimer m_since;
  QTimer m_tick;
};
