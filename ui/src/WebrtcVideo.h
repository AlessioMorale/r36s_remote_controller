#pragma once

#include <QObject>
#include <QPointer>
#include <QString>
#include <QTimer>
#include <atomic>

#if RC_UI_HAS_WEBRTC
#include <QVideoFrame>
#include <QVideoSink>
#endif

// Native WebRTC video receiver (design §3.3 alternative to the Lichtblick view).
//
// Runs `webrtcsrc ! decodebin ! videoconvert ! appsink` (gst-plugins-rs webrtcsrc, which Linux
// distributions do not package: see tools/webrtc_spike) against the robot's signalling server and
// hands each decoded frame to a QVideoSink (QML: VideoOutput.videoSink). It never blocks the GUI
// thread on the network: the pipeline lives in its own GStreamer threads and frames are posted to
// the GUI thread as shared QVideoFrames.
//
// States: off (stopped), connecting (pipeline up, no frame yet), live (frames flowing),
// retrying (failed or stalled, waiting out an exponential backoff 1 s, 2 s, ... 15 s),
// unavailable (built without GStreamer, or the webrtcsrc plugin is missing).
class WebrtcVideo : public QObject {
  Q_OBJECT
  // A QVideoSink (QML: VideoOutput.videoSink); typed QObject* so the header needs no Qt Multimedia.
  Q_PROPERTY(QObject* videoSink READ videoSink WRITE setVideoSink NOTIFY videoSinkChanged)
  Q_PROPERTY(QString state READ state NOTIFY stateChanged)
  Q_PROPERTY(QString reason READ reason NOTIFY stateChanged)
  Q_PROPERTY(double fps READ fps NOTIFY fpsChanged)
  Q_PROPERTY(int retryInMs READ retryInMs NOTIFY stateChanged)
  Q_PROPERTY(bool available READ available CONSTANT)

 public:
  explicit WebrtcVideo(QObject* parent = nullptr);
  ~WebrtcVideo() override;

  // Pipeline description for gst_parse_launch (public for tests). An empty `stunServer` leaves the
  // plugin default (a public STUN server); "none" disables STUN (host candidates only).
  static QString buildPipeline(const QString& signallerUri, const QString& stunServer);

  QObject* videoSink() const { return m_sink; }
  void setVideoSink(QObject* sink);
  QString state() const { return m_state; }
  QString reason() const { return m_reason; }
  double fps() const { return m_fps; }
  int retryInMs() const;
  bool available() const;

  // Starts (or restarts with new settings) streaming from the signalling server. Idempotent.
  Q_INVOKABLE void start(const QString& signallerUri, const QString& stunServer = QString(),
                         int stallMs = 3000, int connectTimeoutMs = 10000);
  Q_INVOKABLE void stop();

#if RC_UI_HAS_WEBRTC
  // Posted by the GStreamer streaming thread (queued); public only for that.
  Q_INVOKABLE void deliverFrame(const QVideoFrame& frame);
#endif

 signals:
  void videoSinkChanged();
  void stateChanged();
  void fpsChanged();
  void frameReceived();  // GUI thread, once per delivered frame (tests)

 private:
  void tick();
  void openPipeline();
  void closePipeline();
  void fail(const QString& why);
  void setState(const QString& state, const QString& reason = QString());

  QPointer<QObject> m_sink;
  QString m_state = QStringLiteral("off");
  QString m_reason;
  double m_fps = 0.0;

  QString m_uri;
  QString m_stun;
  int m_stallMs = 3000;
  int m_connectTimeoutMs = 10000;
  bool m_wanted = false;
  int m_failures = 0;
  int m_backoffMs = 1000;
  qint64 m_retryAtMs = 0;
  qint64 m_openedAtMs = 0;
  qint64 m_liveSinceMs = 0;
  QTimer m_timer;
  qint64 m_lastFpsMs = 0;
  quint64 m_lastFpsFrames = 0;

  // Written by the GStreamer streaming thread.
  std::atomic<quint64> m_frames{0};
  std::atomic<qint64> m_lastFrameMs{0};

  struct Impl;
  Impl* m_impl = nullptr;
  friend struct Impl;
};
