#pragma once

#include <QByteArray>
#include <QJsonObject>
#include <QList>

// Wire framing from docs/ipc.md: a 4-byte big-endian length, then that many
// bytes of UTF-8 JSON holding one object. Maximum message size 64 KiB.
namespace ipc {

constexpr quint32 kMaxMessageBytes = 64 * 1024;

QByteArray encode(const QJsonObject& msg);

class FrameDecoder {
 public:
  enum class Error { None, TooLarge, BadJson };

  // Appends bytes and moves every complete message to `out`. On error the
  // decoder stops consuming; the caller must drop the connection and reset().
  Error feed(const QByteArray& bytes, QList<QJsonObject>& out);
  void reset() { m_buf.clear(); }
  int buffered() const { return int(m_buf.size()); }

 private:
  QByteArray m_buf;
};

}  // namespace ipc
