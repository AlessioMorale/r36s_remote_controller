#pragma once

#include <QByteArray>
#include <QElapsedTimer>
#include <QObject>
#include <QPointer>
#include <QProcess>
#include <QStringList>
#include <QTcpSocket>
#include <QTimer>

#include <functional>
#include <memory>

class AppConfig;

// WiFi / VPN / robot reachability (plan T4.6). Informational only: these
// indicators are "quiet" severity and never raise a loud alarm (design §1, R2).
//
// * link:  the handheld's own network link, WiFi and Ethernet alike: "wifi" or "ethernet"
//          for the primary connected interface, "down" when none is connected, "unknown"
//          when detection is off (net.interfaces "off"). Every feature that needs the
//          network (video, plots, bridge) works over either. The primary interface is the
//          one carrying the default route, else the first wired one, else the first wireless.
// * address / addresses: the primary interface's IPv4 address, and every connected
//          interface with its addresses ("enx00e0… 192.168.8.70 · wlan0 10.0.0.5").
// * vpn:   from `zerotier-cli -j listnetworks`: "ok" when the configured network
//          (or any network, if none configured) has status OK, else "down";
//          "unknown" when zerotier-cli is missing or fails.
// * path:  from `zerotier-cli -j peers` for the robot's node id: "direct" when
//          the peer has an active, non-expired path, "relayed" otherwise;
//          "unknown" without a node id or zerotier-cli.
// * robotReachable: TCP connect probe to robot.host and the active video source's port (signaller_port for webrtc, else bridge_port); latency is the
//          connect time. Goes true on one success, false after two failures.
class NetStatus : public QObject {
  Q_OBJECT
  Q_PROPERTY(QString link READ link NOTIFY changed)
  Q_PROPERTY(QString linkLabel READ linkLabel NOTIFY changed)
  Q_PROPERTY(QString linkInterface READ linkInterface NOTIFY changed)
  Q_PROPERTY(QString address READ address NOTIFY changed)
  Q_PROPERTY(QString addresses READ addresses NOTIFY changed)
  Q_PROPERTY(QString vpn READ vpn NOTIFY changed)
  Q_PROPERTY(QString path READ path NOTIFY changed)
  Q_PROPERTY(int latencyMs READ latencyMs NOTIFY changed)
  Q_PROPERTY(int peerLatencyMs READ peerLatencyMs NOTIFY changed)
  Q_PROPERTY(bool robotReachable READ robotReachable NOTIFY changed)
  Q_PROPERTY(bool zerotierAvailable READ zerotierAvailable NOTIFY changed)
  Q_PROPERTY(QString summary READ summary NOTIFY changed)
  Q_PROPERTY(bool degraded READ degraded NOTIFY changed)
  Q_PROPERTY(QString robotEndpoint READ robotEndpoint CONSTANT)

 public:
  struct VpnResult {
    QString vpn;  // ok / down / unknown
  };
  struct PathResult {
    QString path;  // direct / relayed / unknown
    int latencyMs = -1;
  };
  struct Iface {
    QString name;
    bool wireless = false;
    bool running = false;
    QStringList ipv4;
  };
  struct LinkResult {
    QString link;       // wifi / ethernet / down
    QString ifname;     // primary interface, "" when down
    QString address;    // its first IPv4 address, "" if it has none
    QString addresses;  // every connected interface: "name addr[, addr]" joined by " · "
  };
  // Picks the primary link. `allow` empty = auto (any interface given, which the caller has
  // already filtered to physical ones); otherwise only those names, in that priority order.
  static LinkResult pickLink(const QList<Iface>& ifaces, const QStringList& allow, const QString& defaultRouteIf);
  // Interface of the IPv4 default route from /proc/net/route ("" if none).
  static QString parseDefaultRoute(const QByteArray& procNetRoute);
  // Auto mode skips loopback and virtual / VPN interfaces (ZeroTier, Docker, bridges, tunnels).
  static bool isVirtualName(const QString& name);

  static VpnResult parseNetworks(const QByteArray& json, const QString& networkId);
  static PathResult parsePeers(const QByteArray& json, const QString& nodeId);

  explicit NetStatus(const AppConfig* cfg, QObject* parent = nullptr);
  void start();

  QString link() const { return m_link.link; }
  QString linkLabel() const;
  QString linkInterface() const { return m_link.ifname; }
  QString address() const { return m_link.address; }
  QString addresses() const { return m_link.addresses; }
  QString vpn() const { return m_vpn; }
  QString path() const { return m_path; }
  int latencyMs() const { return m_latencyMs; }
  int peerLatencyMs() const { return m_peerLatencyMs; }
  bool robotReachable() const { return m_reachable; }
  bool zerotierAvailable() const { return m_ztAvailable; }
  QString summary() const;
  bool degraded() const;
  QString robotEndpoint() const;

 signals:
  void changed();

 private:
  void poll();
  void pollLink();
  void runZt(const QStringList& args, std::function<void(bool ok, const QByteArray&)> done);
  void probe();
  void probeResult(bool ok);
  void set(QString& field, const QString& v);

  const AppConfig* m_cfg;
  QTimer m_pollTimer;
  QTimer m_probeTimer;
  QPointer<QProcess> m_proc;
  QTcpSocket* m_sock = nullptr;
  QTimer m_probeTimeout;
  QElapsedTimer m_probeClock;

  LinkResult m_link{QStringLiteral("unknown"), {}, {}, {}};
  QString m_vpn = QStringLiteral("unknown");
  QString m_path = QStringLiteral("unknown");
  int m_latencyMs = -1;
  int m_peerLatencyMs = -1;
  bool m_reachable = false;
  int m_failures = 0;
  bool m_ztAvailable = true;
};
