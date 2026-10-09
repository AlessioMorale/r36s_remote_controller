#include "WebrtcVideo.h"

#include <QElapsedTimer>
#include <QLoggingCategory>
#include <QMetaObject>
#include <cstring>

#if RC_UI_HAS_WEBRTC
#include <QVideoFrameFormat>
#include <gst/app/gstappsink.h>
#include <gst/gst.h>
#include <gst/video/video.h>
#endif

Q_LOGGING_CATEGORY(lcWebrtc, "rc.webrtc")

namespace {
qint64 nowMs() {
  static QElapsedTimer clock = [] {
    QElapsedTimer t;
    t.start();
    return t;
  }();
  return clock.elapsed();
}
}  // namespace

QString WebrtcVideo::buildPipeline(const QString& signallerUri, const QString& stunServer) {
  QString p = QStringLiteral("webrtcsrc signaller::uri=%1 connect-to-first-producer=true video-codecs=\"<H264>\"")
                  .arg(signallerUri);
  if (stunServer == QLatin1String("none"))
    p += QStringLiteral(" stun-server=stun://127.0.0.1:3478");
  else if (!stunServer.isEmpty())
    p += QStringLiteral(" stun-server=%1").arg(stunServer);
  p += QStringLiteral(
      " ! decodebin ! videoconvert ! video/x-raw,format=RGBA"
      " ! appsink name=rc_sink sync=false max-buffers=1 drop=true");
  return p;
}

#if RC_UI_HAS_WEBRTC

struct WebrtcVideo::Impl {
  GstElement* pipeline = nullptr;
  GstElement* sink = nullptr;
};

namespace {
bool gstReady() {
  static const bool ok = [] {
    gst_init(nullptr, nullptr);
    GstElementFactory* f = gst_element_factory_find("webrtcsrc");
    if (!f) return false;
    gst_object_unref(f);
    return true;
  }();
  return ok;
}

GstFlowReturn onSample(GstAppSink* sink, gpointer user) {
  auto* self = static_cast<WebrtcVideo*>(user);
  GstSample* sample = gst_app_sink_pull_sample(sink);
  if (!sample) return GST_FLOW_EOS;
  GstCaps* caps = gst_sample_get_caps(sample);
  GstBuffer* buf = gst_sample_get_buffer(sample);
  GstVideoInfo info;
  GstVideoFrame vf;
  if (caps && buf && gst_video_info_from_caps(&info, caps) &&
      gst_video_frame_map(&vf, &info, buf, GST_MAP_READ)) {
    const int w = GST_VIDEO_INFO_WIDTH(&info), h = GST_VIDEO_INFO_HEIGHT(&info);
    QVideoFrameFormat fmt(QSize(w, h), QVideoFrameFormat::Format_RGBA8888);
    QVideoFrame frame(fmt);
    if (frame.map(QVideoFrame::WriteOnly)) {
      const uchar* src = static_cast<const uchar*>(GST_VIDEO_FRAME_PLANE_DATA(&vf, 0));
      const int srcStride = GST_VIDEO_FRAME_PLANE_STRIDE(&vf, 0);
      const int rowBytes = qMin(w * 4, frame.bytesPerLine(0));
      for (int y = 0; y < h; ++y)
        std::memcpy(frame.bits(0) + y * frame.bytesPerLine(0), src + y * srcStride, size_t(rowBytes));
      frame.unmap();
      QMetaObject::invokeMethod(self, "deliverFrame", Qt::QueuedConnection, Q_ARG(QVideoFrame, frame));
    }
    gst_video_frame_unmap(&vf);
  }
  gst_sample_unref(sample);
  return GST_FLOW_OK;
}
}  // namespace

bool WebrtcVideo::available() const { return gstReady(); }

void WebrtcVideo::openPipeline() {
  GError* error = nullptr;
  const QByteArray desc = buildPipeline(m_uri, m_stun).toUtf8();
  qCInfo(lcWebrtc) << "pipeline:" << desc;
  GstElement* pipeline = gst_parse_launch(desc.constData(), &error);
  if (error) {
    const QString why = QString::fromUtf8(error->message);
    g_error_free(error);
    if (pipeline) gst_object_unref(pipeline);
    fail(QStringLiteral("pipeline: ") + why);
    return;
  }
  GstElement* sink = gst_bin_get_by_name(GST_BIN(pipeline), "rc_sink");
  GstAppSinkCallbacks cb{};
  cb.new_sample = &onSample;
  gst_app_sink_set_callbacks(GST_APP_SINK(sink), &cb, this, nullptr);
  m_impl = new Impl{pipeline, sink};
  m_frames = 0;
  m_lastFrameMs = 0;
  m_lastFpsFrames = 0;
  m_openedAtMs = nowMs();
  if (gst_element_set_state(pipeline, GST_STATE_PLAYING) == GST_STATE_CHANGE_FAILURE) {
    fail(QStringLiteral("pipeline failed to start"));
    return;
  }
  setState(QStringLiteral("connecting"));
}

void WebrtcVideo::closePipeline() {
  if (!m_impl) return;
  // Blocks until the streaming threads are done: no onSample after this.
  gst_element_set_state(m_impl->pipeline, GST_STATE_NULL);
  gst_object_unref(m_impl->sink);
  gst_object_unref(m_impl->pipeline);
  delete m_impl;
  m_impl = nullptr;
}

void WebrtcVideo::tick() {
  const qint64 now = nowMs();
  if (m_wanted && !m_impl) {
    if (m_state == QLatin1String("retrying") && now >= m_retryAtMs) openPipeline();
    return;
  }
  if (!m_impl) return;

  // Bus: first error ends this attempt.
  GstBus* bus = gst_element_get_bus(m_impl->pipeline);
  QString error;
  while (GstMessage* msg = gst_bus_pop_filtered(bus, GstMessageType(GST_MESSAGE_ERROR | GST_MESSAGE_EOS))) {
    if (GST_MESSAGE_TYPE(msg) == GST_MESSAGE_ERROR && error.isEmpty()) {
      GError* err = nullptr;
      gchar* dbg = nullptr;
      gst_message_parse_error(msg, &err, &dbg);
      error = QString::fromUtf8(err->message);
      qCWarning(lcWebrtc) << "pipeline error from" << GST_OBJECT_NAME(msg->src) << error << (dbg ? dbg : "");
      g_clear_error(&err);
      g_free(dbg);
    } else if (GST_MESSAGE_TYPE(msg) == GST_MESSAGE_EOS && error.isEmpty()) {
      error = QStringLiteral("stream ended");
    }
    gst_message_unref(msg);
  }
  gst_object_unref(bus);
  if (!error.isEmpty()) return fail(error);

  const qint64 last = m_lastFrameMs.load();
  if (last == 0) {
    if (now - m_openedAtMs > m_connectTimeoutMs) fail(QStringLiteral("no video from the robot"));
  } else if (now - last > m_stallMs) {
    fail(QStringLiteral("video stalled"));
  }
}
#else  // built without GStreamer

struct WebrtcVideo::Impl {};
bool WebrtcVideo::available() const { return false; }
void WebrtcVideo::openPipeline() { fail(QStringLiteral("built without GStreamer")); }
void WebrtcVideo::closePipeline() {}
void WebrtcVideo::tick() {}
#endif

WebrtcVideo::WebrtcVideo(QObject* parent) : QObject(parent) {
#if RC_UI_HAS_WEBRTC
  qRegisterMetaType<QVideoFrame>("QVideoFrame");
#endif
  m_timer.setInterval(200);
  connect(&m_timer, &QTimer::timeout, this, [this] {
    tick();
    const qint64 now = nowMs();
    if (m_lastFpsMs == 0) m_lastFpsMs = now;
    if (now - m_lastFpsMs >= 1000) {
      const quint64 n = m_frames.load();
      const double fps = double(n - m_lastFpsFrames) * 1000.0 / double(now - m_lastFpsMs);
      m_lastFpsFrames = n;
      m_lastFpsMs = now;
      if (qAbs(fps - m_fps) > 0.05) {
        m_fps = fps;
        emit fpsChanged();
      }
    }
    // A connection that stayed live for 30 s earns a fresh backoff.
    if (m_state == QLatin1String("live") && now - m_liveSinceMs > 30000) {
      m_failures = 0;
      m_backoffMs = 1000;
    }
  });
}

WebrtcVideo::~WebrtcVideo() { closePipeline(); }

int WebrtcVideo::retryInMs() const {
  return m_state == QLatin1String("retrying") ? int(qMax<qint64>(0, m_retryAtMs - nowMs())) : 0;
}

void WebrtcVideo::setVideoSink(QObject* sink) {
  if (m_sink == sink) return;
  m_sink = sink;
  emit videoSinkChanged();
}

void WebrtcVideo::start(const QString& signallerUri, const QString& stunServer, int stallMs,
                        int connectTimeoutMs) {
  const bool same = m_wanted && signallerUri == m_uri && stunServer == m_stun;
  m_stallMs = stallMs;
  m_connectTimeoutMs = connectTimeoutMs;
  if (same) return;
  closePipeline();
  m_uri = signallerUri;
  m_stun = stunServer;
  m_wanted = true;
  m_failures = 0;
  m_backoffMs = 1000;
  if (!available()) {
    setState(QStringLiteral("unavailable"), QStringLiteral("webrtcsrc plugin not available"));
    return;
  }
  m_timer.start();
  openPipeline();
}

void WebrtcVideo::stop() {
  m_wanted = false;
  m_timer.stop();
  closePipeline();
  if (m_fps != 0.0) {
    m_fps = 0.0;
    emit fpsChanged();
  }
  setState(QStringLiteral("off"));
}

void WebrtcVideo::fail(const QString& why) {
  qCWarning(lcWebrtc) << "video failed:" << why;
  closePipeline();
  ++m_failures;
  m_retryAtMs = nowMs() + m_backoffMs;
  m_backoffMs = qMin(15000, m_backoffMs * 2);
  setState(QStringLiteral("retrying"), why);
}

void WebrtcVideo::setState(const QString& state, const QString& reason) {
  if (state == m_state && reason == m_reason) return;
  m_state = state;
  m_reason = reason;
  emit stateChanged();
}

#if RC_UI_HAS_WEBRTC
// Queued from the streaming thread.
void WebrtcVideo::deliverFrame(const QVideoFrame& frame) {
  if (!m_wanted || !m_impl) return;
  m_frames.fetch_add(1);
  m_lastFrameMs = nowMs();
  if (m_state != QLatin1String("live")) {
    m_liveSinceMs = nowMs();
    setState(QStringLiteral("live"));
  }
  if (auto* sink = qobject_cast<QVideoSink*>(m_sink.data())) sink->setVideoFrame(frame);
  emit frameReceived();
}
#endif
