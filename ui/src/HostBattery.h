#pragma once

#include <QObject>
#include <QTimer>

// The handheld's own battery (wireframe "R36S 81%"), read from
// /sys/class/power_supply/*/capacity for the first supply of type "Battery".
// percent is -1 where there is none (e.g. a desktop), and the UI hides it.
class HostBattery : public QObject {
  Q_OBJECT
  Q_PROPERTY(int percent READ percent NOTIFY changed)
  Q_PROPERTY(bool charging READ charging NOTIFY changed)

 public:
  explicit HostBattery(const QString& sysfsRoot = QStringLiteral("/sys/class/power_supply"), QObject* parent = nullptr);
  int percent() const { return m_percent; }
  bool charging() const { return m_charging; }
  void refresh();

 signals:
  void changed();

 private:
  QString m_root;
  QTimer m_timer;
  int m_percent = -1;
  bool m_charging = false;
};
