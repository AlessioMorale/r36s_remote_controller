#include "IpcClient.h"

#include <QJsonValue>
#include <QLoggingCategory>

Q_LOGGING_CATEGORY(lcIpc, "rc.ipc")

IpcClient::IpcClient(const QString& socketPath, int reconnectMs, int requestTimeoutMs, QObject* parent)
    : QObject(parent), m_path(socketPath), m_reconnectMs(reconnectMs), m_requestTimeoutMs(requestTimeoutMs) {
  m_reconnect.setSingleShot(true);
  m_reconnect.setInterval(m_reconnectMs);
  connect(&m_reconnect, &QTimer::timeout, this, &IpcClient::tryConnect);

  m_timeouts.setInterval(100);
  connect(&m_timeouts, &QTimer::timeout, this, &IpcClient::checkTimeouts);

  connect(&m_sock, &QLocalSocket::connected, this, [this] {
    m_decoder.reset();
    setError(QString());
    setConnected(true);
    qCInfo(lcIpc) << "connected to" << m_path;
  });
  connect(&m_sock, &QLocalSocket::readyRead, this, &IpcClient::onReadyRead);
  connect(&m_sock, &QLocalSocket::errorOccurred, this,
          [this](QLocalSocket::LocalSocketError) { setError(m_sock.errorString()); });
  // Covers both a dropped connection and a failed connect attempt (no socket file,
  // connection refused): either way, retry after reconnectMs.
  connect(&m_sock, &QLocalSocket::stateChanged, this, [this](QLocalSocket::LocalSocketState st) {
    if (st == QLocalSocket::UnconnectedState) onDisconnected();
  });
}

IpcClient::~IpcClient() {
  m_running = false;
  m_sock.disconnect(this);
  m_sock.abort();
}

void IpcClient::start() {
  m_running = true;
  m_timeouts.start();
  tryConnect();
}

void IpcClient::stop() {
  m_running = false;
  m_reconnect.stop();
  m_sock.abort();
}

void IpcClient::tryConnect() {
  if (!m_running || m_sock.state() != QLocalSocket::UnconnectedState) return;
  m_sock.connectToServer(m_path, QIODevice::ReadWrite);
}

void IpcClient::onDisconnected() {
  const bool was = m_connected;
  setConnected(false);
  if (was) qCInfo(lcIpc) << "disconnected:" << m_lastError;
  failAllPending(QStringLiteral("disconnected"));
  if (m_running && !m_reconnect.isActive()) m_reconnect.start();
}

void IpcClient::setConnected(bool c) {
  if (c == m_connected) return;
  m_connected = c;
  if (!c) {
    m_daemonVersion.clear();
    m_protocolVersion = 0;
    emit helloChanged();
  }
  emit connectedChanged();
}

void IpcClient::setError(const QString& e) {
  if (e == m_lastError) return;
  m_lastError = e;
  emit lastErrorChanged();
}

void IpcClient::onReadyRead() {
  QList<QJsonObject> msgs;
  const auto err = m_decoder.feed(m_sock.readAll(), msgs);
  for (const auto& m : msgs) dispatch(m);
  if (err != ipc::FrameDecoder::Error::None) {
    setError(err == ipc::FrameDecoder::Error::TooLarge ? QStringLiteral("oversized frame")
                                                        : QStringLiteral("invalid JSON frame"));
    qCWarning(lcIpc) << "protocol error:" << m_lastError << "- dropping connection";
    m_decoder.reset();
    m_sock.abort();
  }
}

void IpcClient::dispatch(const QJsonObject& msg) {
  emit messageReceived(msg);
  const QString type = msg.value(QLatin1String("type")).toString();
  if (type == QLatin1String("telemetry")) {
    emit telemetryReceived(msg);
  } else if (type == QLatin1String("alarm")) {
    emit alarmReceived(msg);
  } else if (type == QLatin1String("params")) {
    emit paramsReceived(msg);
  } else if (type == QLatin1String("menu_input")) {
    emit menuInput(msg.value(QLatin1String("button")).toString());
  } else if (type == QLatin1String("hello")) {
    m_daemonVersion = msg.value(QLatin1String("daemon")).toString();
    m_protocolVersion = msg.value(QLatin1String("version")).toInt();
    if (m_protocolVersion != kSupportedProtocol)
      qCWarning(lcIpc) << "daemon speaks IPC version" << m_protocolVersion << "UI expects" << kSupportedProtocol;
    emit helloChanged();
  } else if (type == QLatin1String("ack")) {
    const int id = msg.value(QLatin1String("id")).toInt(-1);
    const auto it = m_pending.find(id);
    if (it == m_pending.end()) {
      qCDebug(lcIpc) << "ack for unknown/expired id" << id;
      return;
    }
    const QString reqType = it->type;
    m_pending.erase(it);
    emit ackReceived(id, msg.value(QLatin1String("ok")).toBool(), msg.value(QLatin1String("error")).toString(),
                     reqType);
  } else {
    qCDebug(lcIpc) << "ignoring unknown message type" << type;
  }
}

int IpcClient::request(const QString& type, const QVariantMap& fields) {
  const int id = m_nextId++;
  if (!m_connected) {
    // Report asynchronously so callers can register the id first.
    QMetaObject::invokeMethod(
        this, [this, id, type] { emit ackReceived(id, false, QStringLiteral("daemon not connected"), type); },
        Qt::QueuedConnection);
    return id;
  }
  QJsonObject msg = QJsonObject::fromVariantMap(fields);
  msg.insert(QStringLiteral("type"), type);
  msg.insert(QStringLiteral("id"), id);
  Pending p{type, {}};
  p.age.start();
  m_pending.insert(id, p);
  m_sock.write(ipc::encode(msg));
  m_sock.flush();
  qCDebug(lcIpc) << "->" << type << id;
  return id;
}

void IpcClient::checkTimeouts() {
  QList<int> expired;
  for (auto it = m_pending.cbegin(); it != m_pending.cend(); ++it)
    if (it->age.elapsed() > m_requestTimeoutMs) expired << it.key();
  for (int id : expired) {
    const QString t = m_pending.take(id).type;
    emit ackReceived(id, false, QStringLiteral("timeout"), t);
  }
}

void IpcClient::failAllPending(const QString& why) {
  const auto pending = std::exchange(m_pending, {});
  for (auto it = pending.cbegin(); it != pending.cend(); ++it) emit ackReceived(it.key(), false, why, it->type);
}
