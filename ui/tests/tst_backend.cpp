// Unit tests for the UI backend: IPC framing, IpcClient (against a real
// QLocalServer), TelemetryModel / AlarmModel / ParamModel parsing, and the
// zerotier-cli JSON parsers used by NetStatus.
#include <QJsonArray>
#include <QJsonDocument>
#include <QLocalServer>
#include <QLocalSocket>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QtEndian>
#include <QtTest>

#include "AlarmModel.h"
#include "IpcClient.h"
#include "IpcFraming.h"
#include "NetStatus.h"
#include "ParamModel.h"
#include "TelemetryModel.h"

namespace {
QJsonObject json(const char* s) { return QJsonDocument::fromJson(QByteArray(s)).object(); }

// Example telemetry from docs/ipc.md.
const char* kTelemetry = R"({
  "type": "telemetry", "seq": 1234,
  "link": {"lq": 98, "rssi_dbm": -64, "snr_db": 9, "tx_power_mw": 100, "rf_mode": 7,
           "downlink_lq": 97, "downlink_rssi_dbm": -66, "age_ms": 120, "stale": false},
  "battery": {"voltage": 15.6, "current": 1.2, "used_mah": 450, "percent": 72, "age_ms": 400, "stale": false},
  "status": {"text": "RDY", "age_ms": 300, "stale": false},
  "arm": {"state": "armed", "deadman": true, "aux1": true},
  "input": {"mode": "drive", "device": true, "turbo": false},
  "channels": [1500, 1500, 1500, 1500, 2000, 1000, 1500, 1500, 1500, 1500, 1500, 1500, 1500, 1500, 1500, 1500],
  "tx": {"serial_open": true, "synced": true, "frame_period_us": 4000, "frames": 81234},
  "module": {"name": "ELRS TX 2400", "connected": true},
  "overrides": [{"channel": 2, "value_us": 1600, "expires_in_ms": 300}]
})";

// Example params from docs/ipc.md, plus a float, a hidden entry and gap options.
const char* kParams = R"({
  "type": "params", "complete": true,
  "device": {"name": "ELRS TX 2400", "serial": 1162629715, "firmware": 197891, "parameters_total": 25},
  "params": [
    {"number": 5, "parent": 4, "type": "text_selection", "name": "Max Power", "hidden": false,
     "value": 3, "min": 0, "max": 4, "options": ["10", "25", "50", "100", "250"], "unit": "mW"},
    {"number": 4, "parent": 0, "type": "folder", "name": "TX Power", "hidden": false, "children": [5, 6]},
    {"number": 9, "parent": 0, "type": "command", "name": "Bind", "hidden": false, "status": 0, "info": ""},
    {"number": 1, "parent": 0, "type": "text_selection", "name": "Packet Rate", "hidden": false,
     "value": 1, "min": 0, "max": 3, "options": ["50Hz", "", "150Hz", "250Hz"], "unit": ""},
    {"number": 12, "parent": 0, "type": "float", "name": "Gain", "hidden": false,
     "value": 125, "min": 0, "max": 500, "decimal_point": 2, "step": 5, "unit": "x"},
    {"number": 13, "parent": 0, "type": "info", "name": "Hidden", "hidden": true, "text": "x"}
  ]
})";
}  // namespace

class TstBackend : public QObject {
  Q_OBJECT

 private slots:
  // ---------------------------------------------------------------- framing
  void framingRoundTrip() {
    const QJsonObject msg{{"type", "hello"}, {"version", 1}, {"daemon", "0.1.0"}};
    const QByteArray wire = ipc::encode(msg);
    QCOMPARE(qFromBigEndian<quint32>(reinterpret_cast<const uchar*>(wire.constData())), quint32(wire.size() - 4));
    ipc::FrameDecoder d;
    QList<QJsonObject> out;
    QCOMPARE(d.feed(wire, out), ipc::FrameDecoder::Error::None);
    QCOMPARE(out.size(), 1);
    QCOMPARE(out.first(), msg);
    QCOMPARE(d.buffered(), 0);
  }

  void framingByteByByte() {
    const QByteArray wire = ipc::encode(json(kTelemetry));
    ipc::FrameDecoder d;
    QList<QJsonObject> out;
    for (int i = 0; i < wire.size(); ++i) {
      QCOMPARE(d.feed(wire.mid(i, 1), out), ipc::FrameDecoder::Error::None);
      QCOMPARE(out.size(), i == wire.size() - 1 ? 1 : 0);
    }
    QCOMPARE(out.first().value("seq").toInt(), 1234);
  }

  void framingSeveralInOneChunk() {
    QByteArray wire;
    for (int i = 0; i < 3; ++i) wire += ipc::encode(QJsonObject{{"type", "ack"}, {"id", i}, {"ok", true}});
    const QByteArray partial = ipc::encode(QJsonObject{{"type", "ack"}, {"id", 99}});
    wire += partial.left(6);
    ipc::FrameDecoder d;
    QList<QJsonObject> out;
    QCOMPARE(d.feed(wire, out), ipc::FrameDecoder::Error::None);
    QCOMPARE(out.size(), 3);
    QCOMPARE(out.at(2).value("id").toInt(), 2);
    QCOMPARE(d.buffered(), 6);
    QCOMPARE(d.feed(partial.mid(6), out), ipc::FrameDecoder::Error::None);
    QCOMPARE(out.size(), 4);
    QCOMPARE(out.last().value("id").toInt(), 99);
  }

  void framingRejectsOversize() {
    QByteArray hdr(4, 0);
    qToBigEndian<quint32>(ipc::kMaxMessageBytes + 1, reinterpret_cast<uchar*>(hdr.data()));
    ipc::FrameDecoder d;
    QList<QJsonObject> out;
    QCOMPARE(d.feed(hdr, out), ipc::FrameDecoder::Error::TooLarge);
    // Exactly the maximum is fine (it just waits for the body).
    ipc::FrameDecoder d2;
    qToBigEndian<quint32>(ipc::kMaxMessageBytes, reinterpret_cast<uchar*>(hdr.data()));
    QCOMPARE(d2.feed(hdr, out), ipc::FrameDecoder::Error::None);
  }

  void framingRejectsBadJson() {
    QByteArray bad = "{not json";
    QByteArray hdr(4, 0);
    qToBigEndian<quint32>(quint32(bad.size()), reinterpret_cast<uchar*>(hdr.data()));
    ipc::FrameDecoder d;
    QList<QJsonObject> out;
    QCOMPARE(d.feed(hdr + bad, out), ipc::FrameDecoder::Error::BadJson);
    // A JSON array is not an object either.
    ipc::FrameDecoder d2;
    QByteArray arr = "[1,2]";
    qToBigEndian<quint32>(quint32(arr.size()), reinterpret_cast<uchar*>(hdr.data()));
    QCOMPARE(d2.feed(hdr + arr, out), ipc::FrameDecoder::Error::BadJson);
  }

  // -------------------------------------------------------------- telemetry
  void telemetryParsesIpcExample() {
    TelemetryModel t(1000);
    QSignalSpy spy(&t, &TelemetryModel::updated);
    QVERIFY(t.update(json(kTelemetry)));
    QCOMPARE(spy.count(), 1);
    QVERIFY(t.valid());
    QVERIFY(t.fresh());
    QCOMPARE(t.seq(), 1234);
    QCOMPARE(t.link().value("lq").toInt(), 98);
    QCOMPARE(t.link().value("rssi_dbm").toInt(), -64);
    QCOMPARE(t.battery().value("voltage").toDouble(), 15.6);
    QCOMPARE(t.status().value("text").toString(), QString("RDY"));
    QCOMPARE(t.arm().value("state").toString(), QString("armed"));
    QCOMPARE(t.overrides().size(), 1);
    QCOMPARE(t.overrides().first().toMap().value("value_us").toInt(), 1600);
    QCOMPARE(t.channels().size(), 16);
    QVERIFY(!t.menuMode());
    QVERIFY(t.isLive("link"));
    QVERIFY(!t.update(json(R"({"type":"alarm"})")));
  }

  void telemetryNullsAndStale() {
    TelemetryModel t(1000);
    QVERIFY(t.update(json(R"({"type":"telemetry","seq":1,"link":null,
        "battery":{"voltage":15.1,"percent":null,"age_ms":5000,"stale":true},
        "status":{"text":"FLT:MOTOR_L","age_ms":10,"stale":false},
        "input":{"mode":"menu","device":false,"turbo":false},"overrides":[]})")));
    QVERIFY(t.link().isEmpty());
    QVERIFY(!t.link().contains("lq"));
    QVERIFY(!t.battery().contains("percent"));  // null field -> absent -> "n/a"
    QVERIFY(t.battery().value("stale").toBool());
    QVERIFY(!t.isLive("link"));
    QVERIFY(!t.isLive("battery"));
    QVERIFY(t.isLive("status"));
    QVERIFY(t.menuMode());
    QVERIFY(t.arm().isEmpty());
  }

  void telemetryGoesStaleWithoutSnapshots() {
    TelemetryModel t(100);
    QSignalSpy spy(&t, &TelemetryModel::freshChanged);
    QVERIFY(t.update(json(kTelemetry)));
    QVERIFY(t.fresh());
    QTRY_VERIFY_WITH_TIMEOUT(!t.fresh(), 1000);
    QVERIFY(!t.isLive("link"));
    QVERIFY(t.update(json(kTelemetry)));
    QVERIFY(t.fresh());
    t.connectionLost();
    QVERIFY(!t.fresh());
  }

  // ----------------------------------------------------------------- alarms
  void alarms() {
    AlarmModel a;
    QVERIFY(a.apply(json(R"({"type":"alarm","id":"elrs_lost","level":"loud","active":true,"message":"ELRS link lost"})")));
    QVERIFY(a.apply(json(R"({"type":"alarm","id":"battery_stale","level":"visible","active":true,"message":"x"})")));
    QVERIFY(a.loud());
    QVERIFY(a.elrsLost());
    QCOMPARE(a.loudAlarms().size(), 1);
    QCOMPARE(a.activeIds().size(), 2);
    a.setLocal(AlarmModel::kDaemonLost, "loud", "daemon", true);
    a.clearDaemonAlarms();
    QCOMPARE(a.activeIds(), QStringList{AlarmModel::kDaemonLost});
    a.setLocal(AlarmModel::kDaemonLost, "loud", "daemon", false);
    QVERIFY(!a.loud());
    QVERIFY(a.apply(json(R"({"type":"alarm","id":"status_stale","level":"visible","active":true})")));
    QVERIFY(a.isActive("status_stale"));
    QVERIFY(!a.loud());
    QVERIFY(a.apply(json(R"({"type":"alarm","id":"status_stale","level":"visible","active":false})")));
    QVERIFY(a.activeIds().isEmpty());
  }

  // ----------------------------------------------------------------- params
  void paramsTree() {
    ParamModel p;
    QVERIFY(p.setTree(json(kParams)));
    QVERIFY(p.complete());
    QCOMPARE(p.device().value("name").toString(), QString("ELRS TX 2400"));
    const auto root = p.childrenOf(0);
    QStringList names;
    for (const auto& v : root) names << v.toMap().value("name").toString();
    QCOMPARE(names, (QStringList{"Packet Rate", "TX Power", "Bind", "Gain"}));  // hidden skipped
    const auto tx = p.childrenOf(4);  // children [5, 6]; 6 unknown -> skipped
    QCOMPARE(tx.size(), 1);
    QCOMPARE(p.displayValue(5), QString("100 mW"));
    QCOMPARE(p.displayValue(5, 4), QString("250 mW"));
    QCOMPARE(p.displayValue(12), QString("1.25 x"));
    QCOMPARE(p.displayValue(9), QString("Run"));
    QVERIFY(p.isEditable(5));
    QVERIFY(!p.isEditable(9));
    // Stepping clamps at max and skips ELRS' empty (unavailable) options.
    QCOMPARE(p.stepValue(5, 4, +1).toInt(), 4);
    QCOMPARE(p.stepValue(5, 3, +1).toInt(), 4);
    QCOMPARE(p.stepValue(1, 0, +1).toInt(), 2);
    QCOMPARE(p.stepValue(1, 2, -1).toInt(), 0);
    QCOMPARE(p.stepValue(12, 125, +1).toLongLong(), 130);
    QCOMPARE(p.stepValue(12, 498, +1).toLongLong(), 500);
  }

  // ------------------------------------------------------------- IpcClient
  void ipcClientEndToEnd() {
    QTemporaryDir dir;
    const QString path = dir.filePath("c.sock");
    QLocalServer server;
    QVERIFY(server.listen(path));

    IpcClient c(path, 100, 2000);
    QSignalSpy conn(&c, &IpcClient::connectedChanged);
    QSignalSpy tele(&c, &IpcClient::telemetryReceived);
    QSignalSpy menu(&c, &IpcClient::menuInput);
    QSignalSpy acks(&c, &IpcClient::ackReceived);
    c.start();
    QTRY_VERIFY(server.hasPendingConnections() || c.isConnected());
    QVERIFY(server.waitForNewConnection(1000) || true);
    QLocalSocket* s = server.nextPendingConnection();
    QVERIFY(s);
    QTRY_VERIFY(c.isConnected());

    s->write(ipc::encode(json(R"({"type":"hello","version":1,"daemon":"0.1.0"})")));
    s->write(ipc::encode(json(kTelemetry)));
    s->write(ipc::encode(json(R"({"type":"menu_input","button":"up"})")));
    s->flush();
    QTRY_COMPARE(tele.count(), 1);
    QTRY_COMPARE(menu.count(), 1);
    QCOMPARE(menu.first().first().toString(), QString("up"));
    QCOMPARE(c.daemonVersion(), QString("0.1.0"));
    QCOMPARE(c.protocolVersion(), 1);

    // Request -> framed JSON with id -> ack routed back with the request type.
    const int id = c.request("param_write", {{"number", 5}, {"value", 2}});
    ipc::FrameDecoder d;
    QList<QJsonObject> got;
    QTRY_VERIFY((s->bytesAvailable() > 0 && (d.feed(s->readAll(), got), true) && !got.isEmpty()) || !got.isEmpty());
    QCOMPARE(got.first().value("type").toString(), QString("param_write"));
    QCOMPARE(got.first().value("id").toInt(), id);
    QCOMPARE(got.first().value("number").toInt(), 5);
    QCOMPARE(got.first().value("value").toInt(), 2);
    s->write(ipc::encode(QJsonObject{{"type", "ack"}, {"id", id}, {"ok", false}, {"error", "nope"}}));
    s->flush();
    QTRY_COMPARE(acks.count(), 1);
    QCOMPARE(acks.first().at(0).toInt(), id);
    QCOMPARE(acks.first().at(1).toBool(), false);
    QCOMPARE(acks.first().at(2).toString(), QString("nope"));
    QCOMPARE(acks.first().at(3).toString(), QString("param_write"));
    QCOMPARE(c.pendingRequests(), 0);

    // Server drops the client -> client reconnects on its own.
    s->disconnectFromServer();
    QTRY_VERIFY(!c.isConnected());
    QTRY_VERIFY_WITH_TIMEOUT(server.waitForNewConnection(50), 3000);
    QLocalSocket* s2 = server.nextPendingConnection();
    QVERIFY(s2);
    QTRY_VERIFY(c.isConnected());

    // An oversized length prefix makes the client drop the connection (and reconnect).
    QByteArray hdr(4, 0);
    qToBigEndian<quint32>(ipc::kMaxMessageBytes + 10, reinterpret_cast<uchar*>(hdr.data()));
    s2->write(hdr);
    s2->flush();
    QTRY_VERIFY(!c.isConnected());
    QCOMPARE(c.lastError(), QString("oversized frame"));
    QTRY_VERIFY_WITH_TIMEOUT(server.waitForNewConnection(50), 3000);
    QVERIFY(server.nextPendingConnection());
    QTRY_VERIFY(c.isConnected());
  }

  void ipcClientTimeoutsAndOffline() {
    QTemporaryDir dir;
    const QString path = dir.filePath("t.sock");
    IpcClient offline(path, 100, 200);
    QSignalSpy offAcks(&offline, &IpcClient::ackReceived);
    offline.start();
    const int id0 = offline.request("get_state");
    QTRY_COMPARE(offAcks.count(), 1);
    QCOMPARE(offAcks.first().at(0).toInt(), id0);
    QCOMPARE(offAcks.first().at(2).toString(), QString("daemon not connected"));

    QLocalServer server;
    QVERIFY(server.listen(path));
    QTRY_VERIFY_WITH_TIMEOUT(offline.isConnected(), 2000);  // auto-reconnect picked the server up
    QVERIFY(server.nextPendingConnection());
    const int id1 = offline.request("param_refresh");
    QTRY_COMPARE_WITH_TIMEOUT(offAcks.count(), 2, 2000);
    QCOMPARE(offAcks.last().at(0).toInt(), id1);
    QCOMPARE(offAcks.last().at(2).toString(), QString("timeout"));
  }

  // -------------------------------------------------------------- zerotier
  void zerotierParsers() {
    const QByteArray nets = R"([{"id":"8056c2e21c000001","name":"kvn","status":"OK",
        "assignedAddresses":["10.147.17.20/24"]},
       {"id":"aaaaaaaaaaaaaaaa","status":"REQUESTING_CONFIGURATION","assignedAddresses":[]}])";
    QCOMPARE(NetStatus::parseNetworks(nets, "").vpn, QString("ok"));
    QCOMPARE(NetStatus::parseNetworks(nets, "8056C2E21C000001").vpn, QString("ok"));
    QCOMPARE(NetStatus::parseNetworks(nets, "aaaaaaaaaaaaaaaa").vpn, QString("down"));
    QCOMPARE(NetStatus::parseNetworks("[]", "").vpn, QString("down"));
    QCOMPARE(NetStatus::parseNetworks("zerotier-cli: missing authtoken", "").vpn, QString("unknown"));

    const QByteArray peers = R"([
      {"address":"778cde7190","latency":461,"role":"PLANET","tunneled":false,
       "paths":[{"active":true,"expired":false,"address":"103.195.103.66/9993","preferred":true}]},
      {"address":"abcdef0123","latency":23,"role":"LEAF","tunneled":false,
       "paths":[{"active":true,"expired":false,"address":"192.168.1.40/9993","preferred":true}]},
      {"address":"1111111111","latency":180,"role":"LEAF","tunneled":false,"paths":[]},
      {"address":"2222222222","latency":200,"role":"LEAF","tunneled":true,
       "paths":[{"active":true,"expired":false}]}])";
    auto r = NetStatus::parsePeers(peers, "abcdef0123");
    QCOMPARE(r.path, QString("direct"));
    QCOMPARE(r.latencyMs, 23);
    QCOMPARE(NetStatus::parsePeers(peers, "1111111111").path, QString("relayed"));
    QCOMPARE(NetStatus::parsePeers(peers, "2222222222").path, QString("relayed"));
    QCOMPARE(NetStatus::parsePeers(peers, "9999999999").path, QString("relayed"));
    QCOMPARE(NetStatus::parsePeers(peers, "").path, QString("unknown"));
  }
};

QTEST_GUILESS_MAIN(TstBackend)
#include "tst_backend.moc"
