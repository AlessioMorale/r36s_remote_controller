#pragma once

#include <QElapsedTimer>
#include <QJsonObject>
#include <QMap>
#include <QObject>
#include <QStringList>
#include <QVariantList>

// Active alarms. Daemon alarms come from `alarm` messages (docs/ipc.md) and are
// cleared when the connection drops (the daemon repeats active ones on connect).
// The UI adds its own local alarm `ui_daemon_lost` (loud) when the daemon is
// unreachable or silent; it is never sent anywhere.
class AlarmModel : public QObject {
  Q_OBJECT
  Q_PROPERTY(QVariantList active READ active NOTIFY changed)
  Q_PROPERTY(QStringList activeIds READ activeIds NOTIFY changed)
  Q_PROPERTY(bool loud READ loud NOTIFY changed)
  Q_PROPERTY(bool elrsLost READ elrsLost NOTIFY changed)
  Q_PROPERTY(QVariantList loudAlarms READ loudAlarms NOTIFY changed)

 public:
  static constexpr const char* kDaemonLost = "ui_daemon_lost";

  explicit AlarmModel(QObject* parent = nullptr);

  bool apply(const QJsonObject& msg);
  void clearDaemonAlarms();
  void setLocal(const QString& id, const QString& level, const QString& message, bool active);

  QVariantList active() const;
  QStringList activeIds() const { return m_alarms.keys(); }
  bool loud() const;
  bool elrsLost() const { return m_alarms.contains(QStringLiteral("elrs_lost")); }
  QVariantList loudAlarms() const;
  Q_INVOKABLE bool isActive(const QString& id) const { return m_alarms.contains(id); }

 signals:
  void changed();

 private:
  struct Alarm {
    QString level;
    QString message;
    bool local = false;
    QElapsedTimer since;
  };
  QVariantMap toMap(const QString& id, const Alarm& a) const;
  QMap<QString, Alarm> m_alarms;
};
