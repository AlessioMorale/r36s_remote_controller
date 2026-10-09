#include "NetStatus.h"

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLoggingCategory>
#include <QNetworkInterface>

#include "AppConfig.h"

Q_LOGGING_CATEGORY(lcNet, "rc.net")

NetStatus::VpnResult NetStatus::parseNetworks(const QByteArray& json, const QString& networkId) {
  QJsonParseError pe{};
  const auto doc = QJsonDocument::fromJson(json, &pe);
  if (pe.error != QJsonParseError::NoError || !doc.isArray()) return {QStringLiteral("unknown")};
  for (const auto& v : doc.array()) {
    const auto n = v.toObject();
    const QString id = n.value(QLatin1String("id")).toString(n.value(QLatin1String("nwid")).toString());
    if (!networkId.isEmpty() && id.compare(networkId, Qt::CaseInsensitive) != 0) continue;
    if (n.value(QLatin1String("status")).toString() == QLatin1String("OK") &&
        !n.value(QLatin1String("assignedAddresses")).toArray().isEmpty())
      return {QStringLiteral("ok")};
  }
  return {QStringLiteral("down")};
}

NetStatus::PathResult NetStatus::parsePeers(const QByteArray& json, const QString& nodeId) {
  if (nodeId.isEmpty()) return {QStringLiteral("unknown"), -1};
  QJsonParseError pe{};
  const auto doc = QJsonDocument::fromJson(json, &pe);
  if (pe.error != QJsonParseError::NoError || !doc.isArray()) return {QStringLiteral("unknown"), -1};
  for (const auto& v : doc.array()) {
    const auto p = v.toObject();
    if (p.value(QLatin1String("address")).toString().compare(nodeId, Qt::CaseInsensitive) != 0) continue;
    const int lat = p.value(QLatin1String("latency")).toInt(-1);
    bool direct = false;
    if (!p.value(QLatin1String("tunneled")).toBool()) {
      for (const auto& pv : p.value(QLatin1String("paths")).toArray()) {
        const auto path = pv.toObject();
        if (path.value(QLatin1String("active")).toBool() && !path.value(QLatin1String("expired")).toBool()) {
          direct = true;
          break;
        }
      }
    }
    return {direct ? QStringLiteral("direct") : QStringLiteral("relayed"), lat};
  }
  // Not in the peer list: no session with the robot at all, so traffic (if any) is relayed.
  return {QStringLiteral("relayed"), -1};
}

NetStatus::NetStatus(const AppConfig* cfg, QObject* parent) : QObject(parent), m_cfg(cfg) {
  m_pollTimer.setInterval(cfg->netPollMs());
  connect(&m_pollTimer, &QTimer::timeout, this, &NetStatus::poll);
  m_probeTimer.setInterval(cfg->probeIntervalMs());
  connect(&m_probeTimer, &QTimer::timeout, this, &NetStatus::probe);
  m_probeTimeout.setSingleShot(true);
  m_probeTimeout.setInterval(cfg->probeTimeoutMs());
  connect(&m_probeTimeout, &QTimer::timeout, this, [this] { probeResult(false); });
}

void NetStatus::start() {
  m_pollTimer.start();
  m_probeTimer.start();
  poll();
  probe();
}

void NetStatus::set(QString& field, const QString& v) {
  if (field == v) return;
  field = v;
  emit changed();
}

void NetStatus::poll() {
  pollWifi();
  if (m_proc) return;  // previous poll still running
  runZt({QStringLiteral("-j"), QStringLiteral("listnetworks")}, [this](bool ok, const QByteArray& out) {
    set(m_vpn, ok ? parseNetworks(out, m_cfg->zerotierNetwork()).vpn : QStringLiteral("unknown"));
    if (!ok) {
      set(m_path, QStringLiteral("unknown"));
      return;
    }
    runZt({QStringLiteral("-j"), QStringLiteral("peers")}, [this](bool ok2, const QByteArray& out2) {
      const auto r = ok2 ? parsePeers(out2, m_cfg->robotNodeId()) : PathResult{QStringLiteral("unknown"), -1};
      set(m_path, m_vpn == QLatin1String("ok") ? r.path : QStringLiteral("unknown"));
      if (r.latencyMs != m_peerLatencyMs) {
        m_peerLatencyMs = r.latencyMs;
        emit changed();
      }
    });
  });
}

void NetStatus::pollWifi() {
  const QString ifname = m_cfg->wifiInterface();
  if (ifname.isEmpty()) {
    set(m_wifi, QStringLiteral("unknown"));
    return;
  }
  const auto iface = QNetworkInterface::interfaceFromName(ifname);
  const bool up = iface.isValid() && iface.flags().testFlag(QNetworkInterface::IsUp) &&
                  iface.flags().testFlag(QNetworkInterface::IsRunning);
  set(m_wifi, up ? QStringLiteral("up") : QStringLiteral("down"));
}

void NetStatus::runZt(const QStringList& args, std::function<void(bool, const QByteArray&)> done) {
  auto* p = new QProcess(this);
  m_proc = p;
  p->setProcessChannelMode(QProcess::SeparateChannels);
  auto finished = std::make_shared<bool>(false);
  auto finish = [this, p, done, finished](bool ok) {
    if (*finished) return;
    *finished = true;
    const QByteArray out = ok ? p->readAllStandardOutput() : QByteArray();
    p->deleteLater();
    if (m_proc == p) m_proc = nullptr;
    done(ok, out);
  };
  connect(p, &QProcess::errorOccurred, this, [this, finish](QProcess::ProcessError e) {
    if (e == QProcess::FailedToStart) {
      if (m_ztAvailable) qCInfo(lcNet) << "zerotier-cli not available; VPN state unknown";
      m_ztAvailable = false;
      emit changed();
    }
    finish(false);
  });
  connect(p, &QProcess::finished, this, [this, finish](int code, QProcess::ExitStatus st) {
    if (!m_ztAvailable) {
      m_ztAvailable = true;
      emit changed();
    }
    finish(st == QProcess::NormalExit && code == 0);
  });
  QTimer::singleShot(1500, p, [p] { p->kill(); });
  p->start(m_cfg->zerotierCli(), args, QIODevice::ReadOnly);
}

void NetStatus::probe() {
  if (m_sock) return;  // still probing
  if (m_cfg->robotHost().isEmpty()) return;
  m_sock = new QTcpSocket(this);
  connect(m_sock, &QTcpSocket::connected, this, [this] { probeResult(true); });
  connect(m_sock, &QTcpSocket::errorOccurred, this, [this] { probeResult(false); });
  m_probeClock.start();
  m_probeTimeout.start();
  m_sock->connectToHost(m_cfg->robotHost(), quint16(m_cfg->probePort()));
}

void NetStatus::probeResult(bool ok) {
  if (!m_sock) return;
  m_probeTimeout.stop();
  const int elapsed = int(m_probeClock.elapsed());
  QTcpSocket* s = std::exchange(m_sock, nullptr);
  s->disconnect(this);
  s->abort();
  s->deleteLater();

  const bool was = m_reachable;
  const int lat = m_latencyMs;
  if (ok) {
    m_failures = 0;
    m_reachable = true;
    m_latencyMs = elapsed;
  } else if (++m_failures >= 2) {
    m_reachable = false;
    m_latencyMs = -1;
  }
  if (was != m_reachable || lat != m_latencyMs) emit changed();
}

QString NetStatus::robotEndpoint() const {
  return QStringLiteral("%1:%2").arg(m_cfg->robotHost()).arg(m_cfg->probePort());
}

bool NetStatus::degraded() const {
  return !m_reachable || m_wifi == QLatin1String("down") || m_vpn == QLatin1String("down") ||
         m_path == QLatin1String("relayed");
}

QString NetStatus::summary() const {
  if (m_wifi == QLatin1String("down")) return QStringLiteral("WiFi down");
  if (m_vpn == QLatin1String("down")) return QStringLiteral("VPN down");
  QString s;
  if (m_path == QLatin1String("direct")) s = QStringLiteral("WiFi direct");
  else if (m_path == QLatin1String("relayed")) s = QStringLiteral("VPN relayed");
  else s = QStringLiteral("WiFi");
  if (!m_reachable) return s + QStringLiteral(" · robot unreachable");
  if (m_latencyMs >= 0) s += QStringLiteral(" %1 ms").arg(m_latencyMs);
  return s;
}
