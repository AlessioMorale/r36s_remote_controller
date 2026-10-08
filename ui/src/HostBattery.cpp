#include "HostBattery.h"

#include <QDir>
#include <QFile>

namespace {
QString readLine(const QString& path) {
  QFile f(path);
  if (!f.open(QIODevice::ReadOnly)) return {};
  return QString::fromLatin1(f.readLine()).trimmed();
}
}  // namespace

HostBattery::HostBattery(const QString& sysfsRoot, QObject* parent) : QObject(parent), m_root(sysfsRoot) {
  m_timer.setInterval(10000);
  connect(&m_timer, &QTimer::timeout, this, &HostBattery::refresh);
  m_timer.start();
  refresh();
}

void HostBattery::refresh() {
  int percent = -1;
  bool charging = false;
  const QDir dir(m_root);
  for (const QString& name : dir.entryList(QDir::Dirs | QDir::NoDotAndDotDot)) {
    const QString base = dir.filePath(name);
    if (readLine(base + QStringLiteral("/type")) != QLatin1String("Battery")) continue;
    bool ok = false;
    const int p = readLine(base + QStringLiteral("/capacity")).toInt(&ok);
    if (!ok) continue;
    percent = qBound(0, p, 100);
    charging = readLine(base + QStringLiteral("/status")) == QLatin1String("Charging");
    break;
  }
  if (percent != m_percent || charging != m_charging) {
    m_percent = percent;
    m_charging = charging;
    emit changed();
  }
}
