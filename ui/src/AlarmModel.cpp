#include "AlarmModel.h"

AlarmModel::AlarmModel(QObject* parent) : QObject(parent) {}

bool AlarmModel::apply(const QJsonObject& msg) {
  if (msg.value(QLatin1String("type")).toString() != QLatin1String("alarm")) return false;
  const QString id = msg.value(QLatin1String("id")).toString();
  if (id.isEmpty()) return false;
  const bool on = msg.value(QLatin1String("active")).toBool();
  if (!on) {
    if (m_alarms.remove(id) > 0) emit changed();
    return true;
  }
  Alarm a;
  const auto prev = m_alarms.constFind(id);
  if (prev != m_alarms.cend()) a.since = prev->since;  // keep the original onset on repeats
  else a.since.start();
  a.level = msg.value(QLatin1String("level")).toString(QStringLiteral("visible"));
  a.message = msg.value(QLatin1String("message")).toString(id);
  m_alarms.insert(id, a);
  emit changed();
  return true;
}

void AlarmModel::clearDaemonAlarms() {
  bool any = false;
  for (auto it = m_alarms.begin(); it != m_alarms.end();) {
    if (!it->local) {
      it = m_alarms.erase(it);
      any = true;
    } else {
      ++it;
    }
  }
  if (any) emit changed();
}

void AlarmModel::setLocal(const QString& id, const QString& level, const QString& message, bool active) {
  if (!active) {
    if (m_alarms.remove(id) > 0) emit changed();
    return;
  }
  if (m_alarms.contains(id)) return;
  Alarm a;
  a.level = level;
  a.message = message;
  a.local = true;
  a.since.start();
  m_alarms.insert(id, a);
  emit changed();
}

QVariantMap AlarmModel::toMap(const QString& id, const Alarm& a) const {
  return {{QStringLiteral("id"), id},
          {QStringLiteral("level"), a.level},
          {QStringLiteral("message"), a.message},
          {QStringLiteral("local"), a.local},
          {QStringLiteral("sinceMs"), a.since.isValid() ? a.since.elapsed() : 0}};
}

QVariantList AlarmModel::active() const {
  QVariantList l;
  for (auto it = m_alarms.cbegin(); it != m_alarms.cend(); ++it) l << toMap(it.key(), it.value());
  return l;
}

bool AlarmModel::loud() const {
  for (const auto& a : m_alarms)
    if (a.level == QLatin1String("loud")) return true;
  return false;
}

QVariantList AlarmModel::loudAlarms() const {
  QVariantList l;
  for (auto it = m_alarms.cbegin(); it != m_alarms.cend(); ++it)
    if (it->level == QLatin1String("loud")) l << toMap(it.key(), it.value());
  return l;
}
