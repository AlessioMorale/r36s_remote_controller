// WebrtcVideo and the video config. The end-to-end test needs the gst-plugins-rs plugins and a
// signalling server; it runs only when RC_TEST_SIGNALLING points at gst-webrtc-signalling-server
// (and GST_PLUGIN_PATH at the plugin directory), otherwise it is skipped.
#include <QJsonDocument>
#include <QProcess>
#include <QSignalSpy>
#include <QVideoFrame>
#include <QVideoSink>
#include <QtTest>

#include "AppConfig.h"
#include "WebrtcVideo.h"

class TstWebrtc : public QObject {
  Q_OBJECT

 private slots:
  void pipelineDescription() {
    const QString p = WebrtcVideo::buildPipeline(QStringLiteral("ws://10.0.0.2:8443"), QString());
    QVERIFY(p.startsWith(QStringLiteral("webrtcsrc signaller::uri=ws://10.0.0.2:8443 connect-to-first-producer=true")));
    QVERIFY(p.contains(QStringLiteral("video-codecs=\"<H264>\"")));
    QVERIFY(!p.contains(QStringLiteral("stun-server")));
    QVERIFY(p.contains(QStringLiteral("! decodebin ! videoconvert ! video/x-raw,format=RGBA ! appsink name=rc_sink")));
    QVERIFY(p.contains(QStringLiteral("sync=false max-buffers=1 drop=true")));
    QVERIFY(WebrtcVideo::buildPipeline(QStringLiteral("ws://h:1"), QStringLiteral("none"))
                .contains(QStringLiteral("stun-server=stun://127.0.0.1:3478")));
    QVERIFY(WebrtcVideo::buildPipeline(QStringLiteral("ws://h:1"), QStringLiteral("stun://a:1"))
                .contains(QStringLiteral("stun-server=stun://a:1")));
  }

  void videoConfig() {
    AppConfig c;
    auto parse = [&](const char* json) {
      c.loadFromJson(QJsonDocument::fromJson(json).object());
    };
    // Default resolves to the build's best source and probes the matching port.
    QCOMPARE(c.videoSource(), c.webrtcAvailable() ? QStringLiteral("webrtc") : QStringLiteral("lichtblick"));
    parse(R"({"video":{"source":"lichtblick"},"robot":{"host":"1.2.3.4","bridge_port":8765,"signaller_port":9000}})");
    QCOMPARE(c.videoSource(), QStringLiteral("lichtblick"));
    QCOMPARE(c.probePort(), 8765);
    parse(R"({"video":{"source":"webrtc","stun_server":"none","stall_ms":1234}})");
    QCOMPARE(c.videoSource(), QStringLiteral("webrtc"));
    QCOMPARE(c.probePort(), 9000);
    QCOMPARE(c.signallerUri(), QStringLiteral("ws://1.2.3.4:9000"));
    QCOMPARE(c.webrtcStunServer(), QStringLiteral("none"));
    QCOMPARE(c.webrtcStallMs(), 1234);
    parse(R"({"video":{"source":"bogus"}})");
    QCOMPARE(c.videoSource(), c.webrtcAvailable() ? QStringLiteral("webrtc") : QStringLiteral("lichtblick"));
    parse(R"({"video":{"source":"none"}})");
    QCOMPARE(c.videoSource(), QStringLiteral("none"));
  }

  void unavailableWithoutPlugin() {
    WebrtcVideo v;
    if (v.available()) QSKIP("webrtcsrc is installed");
    v.start(QStringLiteral("ws://127.0.0.1:1"));
    QCOMPARE(v.state(), QStringLiteral("unavailable"));
    v.stop();
    QCOMPARE(v.state(), QStringLiteral("off"));
  }

  void failsAndRetriesWithoutServer() {
    WebrtcVideo v;
    if (!v.available()) QSKIP("webrtcsrc plugin not available");
    QSignalSpy states(&v, &WebrtcVideo::stateChanged);
    v.start(QStringLiteral("ws://127.0.0.1:1"), QStringLiteral("none"), 3000, 2000);
    // Connection refused or the connect timeout: it must end up retrying, never crash or block.
    QTRY_COMPARE_WITH_TIMEOUT(v.state(), QStringLiteral("retrying"), 8000);
    QVERIFY(!v.reason().isEmpty());
    v.stop();
    QCOMPARE(v.state(), QStringLiteral("off"));
  }

  void receivesFramesAndRecovers() {
    const QByteArray sigBin = qgetenv("RC_TEST_SIGNALLING");
    if (sigBin.isEmpty()) QSKIP("RC_TEST_SIGNALLING not set");
    WebrtcVideo v;
    if (!v.available()) QSKIP("webrtcsrc plugin not available");

    QProcess server;
    server.start(QString::fromLocal8Bit(sigBin), {QStringLiteral("--port"), QStringLiteral("18443")});
    QVERIFY(server.waitForStarted());
    QTest::qWait(500);

    auto startSender = [](QProcess& p) {
      p.start(QStringLiteral("sh"),
              {QStringLiteral("-c"),
               QStringLiteral("exec gst-launch-1.0 webrtcsink name=ws signaller::uri=ws://127.0.0.1:18443 "
                              "stun-server=stun://127.0.0.1:3478 video-caps=video/x-h264 "
                              "videotestsrc is-live=true pattern=ball ! "
                              "video/x-raw,width=640,height=480,framerate=15/1 ! ws.")});
      return p.waitForStarted();
    };
    QProcess sender;
    QVERIFY(startSender(sender));
    QTest::qWait(1500);

    QVideoSink sink;
    int frames = 0;
    QSize size;
    connect(&sink, &QVideoSink::videoFrameChanged, this, [&](const QVideoFrame& f) {
      ++frames;
      size = f.size();
    });
    v.setVideoSink(&sink);
    v.start(QStringLiteral("ws://127.0.0.1:18443"), QStringLiteral("none"), 2000, 8000);

    QTRY_COMPARE_WITH_TIMEOUT(v.state(), QStringLiteral("live"), 15000);
    QTRY_VERIFY_WITH_TIMEOUT(frames >= 30, 10000);
    QCOMPARE(size, QSize(640, 480));
    QTRY_VERIFY_WITH_TIMEOUT(v.fps() > 8.0, 5000);

    // The robot side vanishes: the view must notice (stall) and go to retrying.
    sender.kill();
    sender.waitForFinished(3000);
    QTRY_COMPARE_WITH_TIMEOUT(v.state(), QStringLiteral("retrying"), 10000);

    // It comes back: the view recovers on its own.
    QVERIFY(startSender(sender));
    const int before = frames;
    QTRY_COMPARE_WITH_TIMEOUT(v.state(), QStringLiteral("live"), 30000);
    QTRY_VERIFY_WITH_TIMEOUT(frames > before + 20, 10000);

    v.stop();
    QCOMPARE(v.state(), QStringLiteral("off"));
    sender.kill();
    sender.waitForFinished(3000);
    server.kill();
    server.waitForFinished(3000);
  }
};

QTEST_MAIN(TstWebrtc)
#include "tst_webrtc.moc"
