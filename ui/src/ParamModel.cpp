#include "ParamModel.h"

#include <QJsonArray>
#include <QtMath>

ParamModel::ParamModel(QObject* parent) : QObject(parent) {}

bool ParamModel::isNumericType(const QString& t) {
  static const QStringList numeric = {QStringLiteral("uint8"),  QStringLiteral("int8"),  QStringLiteral("uint16"),
                                      QStringLiteral("int16"),  QStringLiteral("uint32"), QStringLiteral("int32"),
                                      QStringLiteral("float")};
  return numeric.contains(t);
}

bool ParamModel::setTree(const QJsonObject& msg) {
  if (msg.value(QLatin1String("type")).toString() != QLatin1String("params")) return false;
  m_params.clear();
  const auto list = msg.value(QLatin1String("params")).toArray();
  for (const auto& v : list) {
    const auto o = v.toObject();
    if (!o.contains(QLatin1String("number"))) continue;
    m_params.insert(o.value(QLatin1String("number")).toInt(), o.toVariantMap());
  }
  m_device = msg.value(QLatin1String("device")).toObject().toVariantMap();
  m_complete = msg.value(QLatin1String("complete")).toBool();
  ++m_revision;
  emit changed();
  return true;
}

void ParamModel::clear() {
  m_params.clear();
  m_device.clear();
  m_complete = false;
  ++m_revision;
  emit changed();
}

QVariantList ParamModel::childrenOf(int folder) const {
  QList<int> numbers;
  const auto f = m_params.constFind(folder);
  if (f != m_params.cend() && f->contains(QStringLiteral("children"))) {
    for (const auto& c : f->value(QStringLiteral("children")).toList()) numbers << c.toInt();
  } else {
    for (auto it = m_params.cbegin(); it != m_params.cend(); ++it)
      if (it.key() != folder && it->value(QStringLiteral("parent")).toInt() == folder) numbers << it.key();
  }
  QVariantList out;
  for (int n : numbers) {
    const auto it = m_params.constFind(n);
    if (it == m_params.cend() || it->value(QStringLiteral("hidden")).toBool()) continue;
    out << *it;
  }
  return out;
}

bool ParamModel::isEditable(int number) const {
  const auto p = m_params.value(number);
  const QString t = p.value(QStringLiteral("type")).toString();
  return t == QLatin1String("text_selection") || isNumericType(t);
}

QString ParamModel::displayValue(int number, const QVariant& value) const {
  const auto it = m_params.constFind(number);
  if (it == m_params.cend()) return QString();
  return formatValue(*it, value.isValid() && !value.isNull() ? value : it->value(QStringLiteral("value")));
}

QString ParamModel::formatValue(const QVariantMap& p, const QVariant& value) {
  const QString type = p.value(QStringLiteral("type")).toString();
  const QString unit = p.value(QStringLiteral("unit")).toString().trimmed();
  auto withUnit = [&unit](const QString& s) { return unit.isEmpty() || s.isEmpty() ? s : s + QLatin1Char(' ') + unit; };

  if (type == QLatin1String("text_selection")) {
    const auto opts = p.value(QStringLiteral("options")).toList();
    const int i = value.toInt();
    if (i < 0 || i >= opts.size()) return QStringLiteral("?");
    return withUnit(opts.at(i).toString());
  }
  if (type == QLatin1String("float")) {
    const int dp = p.value(QStringLiteral("decimal_point")).toInt();
    return withUnit(QString::number(value.toDouble() / qPow(10.0, dp), 'f', dp));
  }
  if (isNumericType(type)) return withUnit(QString::number(value.toLongLong()));
  if (type == QLatin1String("string") || type == QLatin1String("info")) return p.value(QStringLiteral("text")).toString();
  if (type == QLatin1String("folder")) return QStringLiteral("›");
  if (type == QLatin1String("command")) {
    const QString info = p.value(QStringLiteral("info")).toString();
    switch (p.value(QStringLiteral("status")).toInt()) {
      case Executing: return info.isEmpty() ? QStringLiteral("running…") : info;
      case AskConfirm: return info.isEmpty() ? QStringLiteral("confirm?") : info;
      default: return info.isEmpty() ? QStringLiteral("Run") : info;
    }
  }
  return value.toString();
}

QVariant ParamModel::stepValue(int number, const QVariant& from, int direction) const {
  const auto it = m_params.constFind(number);
  if (it == m_params.cend()) return from;
  const QString type = it->value(QStringLiteral("type")).toString();
  const int dir = direction >= 0 ? 1 : -1;
  if (type == QLatin1String("text_selection")) {
    const auto opts = it->value(QStringLiteral("options")).toList();
    const int lo = qMax(0, it->value(QStringLiteral("min"), 0).toInt());
    const int hi = qMin(int(opts.size()) - 1, it->value(QStringLiteral("max"), int(opts.size()) - 1).toInt());
    int i = from.toInt();
    for (int n = i + dir; n >= lo && n <= hi; n += dir)
      if (!opts.at(n).toString().trimmed().isEmpty()) return n;  // ELRS marks unavailable options with ""
    return i;
  }
  if (isNumericType(type)) {
    const qint64 step = qMax<qint64>(1, it->value(QStringLiteral("step"), 1).toLongLong());
    const qint64 lo = it->value(QStringLiteral("min")).toLongLong();
    const qint64 hi = it->value(QStringLiteral("max")).toLongLong();
    return qBound(lo, from.toLongLong() + dir * step, hi);
  }
  return from;
}
