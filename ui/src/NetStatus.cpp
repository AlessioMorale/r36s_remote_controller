#include "NetStatus.h"

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLoggingCategory>
#include <QFile>
#include <QFileInfo>
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

bool NetStatus::isVirtualName(const QString& n) {
  static const char* const prefixes[] = {"lo", "zt", "docker", "veth", "br-", "virbr", "tun", "tap",
                                         "wg", "tailscale", "dummy", "sit", "ip6tnl", "can"};
  for (const char* p : prefixes)
    if (n.startsWith(QLatin1String(p))) return true;
  return false;
}

QString NetStatus::parseDefaultRoute(const QByteArray& route) {
  // Iface Destination Gateway Flags ...; the default route has Destination 00000000 and RTF_UP.
  const auto lines = route.split('\n');
  for (int i = 1; i < lines.size(); ++i) {
    const auto f = lines[i].simplified().split(' ');
    if (f.size() < 4 || f[1] != "00000000") continue;
    bool ok = false;
    const uint flags = f[3].toUInt(&ok, 16);
    if (ok && (flags & 0x1)) return QString::fromLatin1(f[0]);
  }
  return {};
}

NetStatus::LinkResult NetStatus::pickLink(const QList<Iface>& ifaces, const QStringList& allow,
                                          const QString& defaultRouteIf) {
  QList<Iface> up;
  if (allow.isEmpty()) {
    for (const auto& i : ifaces)
      if (i.running) up << i;
  } else {
    for (const auto& name : allow)
      for (const auto& i : ifaces)
        if (i.name == name && i.running) up << i;
  }
  if (up.isEmpty()) return {QStringLiteral("down"), {}, {}, {}};

  int primary = -1;
  for (int i = 0; i < up.size() && primary < 0; ++i)
    if (!defaultRouteIf.isEmpty() && up[i].name == defaultRouteIf) primary = i;
  if (allow.isEmpty()) {  // auto: wired before wireless, then interfaces with an address first
    for (int i = 0; i < up.size() && primary < 0; ++i)
      if (!up[i].wireless && !up[i].ipv4.isEmpty()) primary = i;
    for (int i = 0; i < up.size() && primary < 0; ++i)
      if (!up[i].ipv4.isEmpty()) primary = i;
  }
  if (primary < 0) primary = 0;

  QStringList all;
  for (const auto& i : up)
    all << i.name + QLatin1Char(' ') + (i.ipv4.isEmpty() ? QStringLiteral("no IPv4") : i.ipv4.join(QStringLiteral(", ")));
  const Iface& p = up[primary];
  return {p.wireless ? QStringLiteral("wifi") : QStringLiteral("ethernet"), p.name,
          p.ipv4.value(0), all.join(QStringLiteral(" · "))};
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
  pollLink();
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

void NetStatus::pollLink() {
  LinkResult r{QStringLiteral("unknown"), {}, {}, {}};
  if (!m_cfg->netOff()) {
    const QStringList allow = m_cfg->netAuto() ? QStringList() : m_cfg->netInterfaces();
    QList<Iface> list;
    for (const auto& qi : QNetworkInterface::allInterfaces()) {
      const QString name = qi.name();
      if (m_cfg->netAuto() && (qi.flags().testFlag(QNetworkInterface::IsLoopBack) || isVirtualName(name))) continue;
      const QString sys = QStringLiteral("/sys/class/net/") + name;
      Iface i;
      i.name = name;
      i.wireless = QFileInfo::exists(sys + QStringLiteral("/wireless")) || QFileInfo::exists(sys + QStringLiteral("/phy80211")) ||
                   qi.type() == QNetworkInterface::Wifi;
      i.running = qi.flags().testFlag(QNetworkInterface::IsUp) && qi.flags().testFlag(QNetworkInterface::IsRunning);
      for (const auto& e : qi.addressEntries())
        if (e.ip().protocol() == QAbstractSocket::IPv4Protocol) i.ipv4 << e.ip().toString();
      list << i;
    }
    QString defIf;
    QFile route(QStringLiteral("/proc/net/route"));
    if (route.open(QIODevice::ReadOnly)) defIf = parseDefaultRoute(route.readAll());
    r = pickLink(list, allow, defIf);
  }
  if (r.link != m_link.link || r.ifname != m_link.ifname || r.address != m_link.address ||
      r.addresses != m_link.addresses) {
    m_link = r;
    emit changed();
  }
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
  return !m_reachable || m_link.link == QLatin1String("down") || m_vpn == QLatin1String("down") ||
         m_path == QLatin1String("relayed");
}

QString NetStatus::linkLabel() const {
  if (m_link.link == QLatin1String("wifi")) return QStringLiteral("WiFi");
  if (m_link.link == QLatin1String("ethernet")) return QStringLiteral("Ethernet");
  if (m_link.link == QLatin1String("down")) return QStringLiteral("No network");
  return QStringLiteral("Network");
}

QString NetStatus::summary() const {
  if (m_link.link == QLatin1String("down")) return QStringLiteral("No network");
  if (m_vpn == QLatin1String("down")) return QStringLiteral("VPN down");
  const QString kind = m_link.link == QLatin1String("wifi")       ? QStringLiteral("WiFi")
                       : m_link.link == QLatin1String("ethernet") ? QStringLiteral("Eth")
                                                                  : QStringLiteral("Net");
  QString s;
  if (m_path == QLatin1String("direct")) s = kind + QStringLiteral(" direct");
  else if (m_path == QLatin1String("relayed")) s = QStringLiteral("VPN relayed");
  else s = kind;
  if (!m_reachable) return s + QStringLiteral(" · robot unreachable");
  if (m_latencyMs >= 0) s += QStringLiteral(" %1 ms").arg(m_latencyMs);
  return s;
}
