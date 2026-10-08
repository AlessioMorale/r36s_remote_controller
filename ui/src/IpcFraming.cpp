#include "IpcFraming.h"

#include <QJsonDocument>
#include <QtEndian>

namespace ipc {

QByteArray encode(const QJsonObject& msg) {
  const QByteArray body = QJsonDocument(msg).toJson(QJsonDocument::Compact);
  QByteArray out(4, Qt::Uninitialized);
  qToBigEndian<quint32>(quint32(body.size()), reinterpret_cast<uchar*>(out.data()));
  out.append(body);
  return out;
}

FrameDecoder::Error FrameDecoder::feed(const QByteArray& bytes, QList<QJsonObject>& out) {
  m_buf.append(bytes);
  qsizetype pos = 0;
  Error err = Error::None;
  while (m_buf.size() - pos >= 4) {
    const quint32 len = qFromBigEndian<quint32>(reinterpret_cast<const uchar*>(m_buf.constData() + pos));
    if (len > kMaxMessageBytes) {
      err = Error::TooLarge;
      break;
    }
    if (m_buf.size() - pos - 4 < qsizetype(len)) break;
    QJsonParseError pe{};
    const auto doc = QJsonDocument::fromJson(m_buf.mid(pos + 4, len), &pe);
    if (pe.error != QJsonParseError::NoError || !doc.isObject()) {
      err = Error::BadJson;
      break;
    }
    out.append(doc.object());
    pos += 4 + len;
  }
  m_buf.remove(0, pos);
  return err;
}

}  // namespace ipc
