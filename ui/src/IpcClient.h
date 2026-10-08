#pragma once

#include <QElapsedTimer>
#include <QHash>
#include <QJsonObject>
#include <QLocalSocket>
#include <QObject>
#include <QTimer>
#include <QVariantMap>

#include "IpcFraming.h"

// Client side of docs/ipc.md. Connects to the daemon's Unix socket, retries
// every `reconnectMs` while disconnected, decodes frames and dispatches them.
// Requests get increasing integer ids; every request is answered by exactly one
// ackReceived() (the daemon's ack, a timeout, or "not connected").
class IpcClient : public QObject {
  Q_OBJECT
  Q_PROPERTY(bool connected READ isConnected NOTIFY connectedChanged)
  Q_PROPERTY(QString daemonVersion READ daemonVersion NOTIFY helloChanged)
  Q_PROPERTY(int protocolVersion READ protocolVersion NOTIFY helloChanged)
  Q_PROPERTY(QString socketPath READ socketPath CONSTANT)
  Q_PROPERTY(QString lastError READ lastError NOTIFY lastErrorChanged)

 public:
  static constexpr int kSupportedProtocol = 1;

  explicit IpcClient(const QString& socketPath, int reconnectMs = 500, int requestTimeoutMs = 3000,
                     QObject* parent = nullptr);
  ~IpcClient() override;

  void start();
  void stop();

  bool isConnected() const { return m_connected; }
  QString daemonVersion() const { return m_daemonVersion; }
  int protocolVersion() const { return m_protocolVersion; }
  QString socketPath() const { return m_path; }
  QString lastError() const { return m_lastError; }
  int pendingRequests() const { return int(m_pending.size()); }

  // Sends {"type": type, "id": <new id>, ...fields}. Returns the id.
  Q_INVOKABLE int request(const QString& type, const QVariantMap& fields = {});

 signals:
  void connectedChanged();
  void helloChanged();
  void lastErrorChanged();
  void messageReceived(const QJsonObject& msg);  // every daemon -> UI message
  void telemetryReceived(const QJsonObject& msg);
  void alarmReceived(const QJsonObject& msg);
  void paramsReceived(const QJsonObject& msg);
  void menuInput(const QString& button);
  void ackReceived(int id, bool ok, const QString& error, const QString& requestType);

 private:
  void tryConnect();
  void onReadyRead();
  void onDisconnected();
  void setConnected(bool c);
  void setError(const QString& e);
  void dispatch(const QJsonObject& msg);
  void checkTimeouts();
  void failAllPending(const QString& why);

  struct Pending {
    QString type;
    QElapsedTimer age;
  };

  QString m_path;
  int m_reconnectMs;
  int m_requestTimeoutMs;
  QLocalSocket m_sock;
  QTimer m_reconnect;
  QTimer m_timeouts;
  ipc::FrameDecoder m_decoder;
  QHash<int, Pending> m_pending;
  int m_nextId = 1;
  bool m_connected = false;
  bool m_running = false;
  QString m_daemonVersion;
  int m_protocolVersion = 0;
  QString m_lastError;
};
