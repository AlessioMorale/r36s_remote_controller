#pragma once

#include <QJsonObject>
#include <QMap>
#include <QObject>
#include <QVariantList>
#include <QVariantMap>

// The ELRS module parameter tree from `params` messages (docs/ipc.md).
// QML reads it through childrenOf(folder) and re-evaluates on `revision`.
class ParamModel : public QObject {
  Q_OBJECT
  Q_PROPERTY(int revision READ revision NOTIFY changed)
  Q_PROPERTY(bool complete READ complete NOTIFY changed)
  Q_PROPERTY(QVariantMap device READ device NOTIFY changed)
  Q_PROPERTY(int count READ count NOTIFY changed)

 public:
  // CRSF command status (PARAMETER type `command`).
  enum CommandStatus { Idle = 0, Click = 1, Executing = 2, AskConfirm = 3, Confirm = 4, Cancel = 5, Query = 6 };
  Q_ENUM(CommandStatus)

  explicit ParamModel(QObject* parent = nullptr);

  bool setTree(const QJsonObject& msg);
  void clear();

  int revision() const { return m_revision; }
  bool complete() const { return m_complete; }
  QVariantMap device() const { return m_device; }
  int count() const { return int(m_params.size()); }

  // Visible children of a folder (0 = root), in the folder's `children` order
  // when given, otherwise by parameter number. Hidden entries are skipped.
  Q_INVOKABLE QVariantList childrenOf(int folder) const;
  Q_INVOKABLE QVariantMap param(int number) const { return m_params.value(number); }
  Q_INVOKABLE bool isEditable(int number) const;
  // Display string for `value` (or the entry's own value when `value` is undefined).
  Q_INVOKABLE QString displayValue(int number, const QVariant& value = QVariant()) const;
  // Next valid value in `direction` (+1 / -1) from `from`, clamped; skips empty options.
  Q_INVOKABLE QVariant stepValue(int number, const QVariant& from, int direction) const;

  static QString formatValue(const QVariantMap& p, const QVariant& value);
  static bool isNumericType(const QString& type);

 signals:
  void changed();

 private:
  QMap<int, QVariantMap> m_params;
  QVariantMap m_device;
  bool m_complete = false;
  int m_revision = 0;
};
