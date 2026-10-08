#include "TelemetryModel.h"

#include <QJsonArray>

namespace {
// JSON null (or a missing group) becomes an empty map: "never received".
QVariantMap group(const QJsonObject& msg, const char* key) {
  const auto v = msg.value(QLatin1String(key));
  if (!v.isObject()) return {};
  QVariantMap m;
  const auto o = v.toObject();
  for (auto it = o.begin(); it != o.end(); ++it)
    if (!it.value().isNull()) m.insert(it.key(), it.value().toVariant());
  return m;
}
}  // namespace

TelemetryModel::TelemetryModel(int staleUiMs, QObject* parent) : QObject(parent), m_staleUiMs(staleUiMs) {
  m_tick.setInterval(200);
  connect(&m_tick, &QTimer::timeout, this, &TelemetryModel::tick);
  m_tick.start();
}

bool TelemetryModel::update(const QJsonObject& msg) {
  if (msg.value(QLatin1String("type")).toString() != QLatin1String("telemetry")) return false;
  m_seq = msg.value(QLatin1String("seq")).toInteger(-1);
  m_link = group(msg, "link");
  m_battery = group(msg, "battery");
  m_status = group(msg, "status");
  m_arm = group(msg, "arm");
  m_input = group(msg, "input");
  m_tx = group(msg, "tx");
  m_module = group(msg, "module");
  m_overrides = msg.value(QLatin1String("overrides")).toArray().toVariantList();
  m_channels = msg.value(QLatin1String("channels")).toArray().toVariantList();
  m_valid = true;
  m_since.start();
  const bool menu = m_input.value(QStringLiteral("mode")).toString() == QLatin1String("menu");
  if (menu != m_menuMode) {
    m_menuMode = menu;
    emit menuModeChanged();
  }
  setFresh(true);
  emit updated();
  return true;
}

void TelemetryModel::connectionLost() {
  setFresh(false);
  // Never keep showing the menu without a daemon driving it.
  if (m_menuMode) {
    m_menuMode = false;
    emit menuModeChanged();
  }
}

int TelemetryModel::snapshotAgeMs() const { return m_since.isValid() ? int(m_since.elapsed()) : -1; }

bool TelemetryModel::isLive(const QString& group) const {
  if (!m_fresh) return false;
  const QVariantMap* g = nullptr;
  if (group == QLatin1String("link")) g = &m_link;
  else if (group == QLatin1String("battery")) g = &m_battery;
  else if (group == QLatin1String("status")) g = &m_status;
  if (!g) return true;
  return !g->isEmpty() && !g->value(QStringLiteral("stale")).toBool();
}

void TelemetryModel::setFresh(bool f) {
  if (f == m_fresh) return;
  m_fresh = f;
  emit freshChanged();
}

void TelemetryModel::tick() {
  if (m_fresh && m_since.isValid() && m_since.elapsed() > m_staleUiMs) setFresh(false);
  emit ageTick();
}
