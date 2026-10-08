// Flat C++ facade over elrs_joy_crsf_protocol for the cxx bridge in src/lib.rs.
// SPDX-License-Identifier: mit
#include "cpp/crsf_ffi.hpp"

#include <optional>
#include <string>
#include <vector>

#include "elrs_crsf_sys/src/lib.rs.h"
#include "elrs_joy_crsf_protocol/crsf/messages.hpp"

namespace elrs_crsf_ffi
{
namespace crsf = elrs_joy_crsf_protocol::crsf;

namespace
{
// Every decoder goes through this check, so the library's accessors (which assume a
// well-formed frame) never see a short or inconsistent buffer.
std::optional<crsf::Message::Frame> checked_frame(rust::Slice<const uint8_t> bytes)
{
  if (bytes.size() < 4 || bytes.size() != static_cast<size_t>(bytes[1]) + 2) {
    return std::nullopt;
  }
  std::vector<uint8_t> data(bytes.begin(), bytes.end());
  if (crsf::Message::parse_message(data).validation_status != crsf::ValidationStatus::OK) {
    return std::nullopt;
  }
  return crsf::Message::Frame{.data = std::move(data)};
}

rust::Vec<uint8_t> to_rust(const std::vector<uint8_t> & data)
{
  rust::Vec<uint8_t> out;
  out.reserve(data.size());
  for (const uint8_t byte : data) {
    out.push_back(byte);
  }
  return out;
}

std::vector<uint8_t> to_std(rust::Slice<const uint8_t> data)
{
  return std::vector<uint8_t>(data.begin(), data.end());
}

crsf::Address addr(uint8_t value) { return static_cast<crsf::Address>(value); }
uint8_t raw(crsf::Address value) { return static_cast<uint8_t>(value); }

crsf::ExtendedHeader header(uint8_t dest, uint8_t origin)
{
  return {.ext_src_addr = addr(origin), .ext_dest_addr = addr(dest)};
}

LinkStats from_payload(const crsf::LinkStatisticsPayload & p)
{
  return LinkStats{
    p.uplinkRssiAnt1,
    p.uplinkRssiAnt2,
    p.uplinkLinkQuality,
    p.uplinkSnr,
    p.activeAntenna,
    p.rfMode,
    static_cast<uint8_t>(p.uplinkTxPower),
    p.downlinkRssi,
    p.downlinkLinkQuality,
    p.downlinkSnr};
}
}  // namespace

Parser::Parser()
: packets_([this](const crsf::Message::Frame & frame) {
    if (queue_.size() >= MAX_QUEUED) {
      queue_.pop_front();
    }
    queue_.push_back(frame);
  })
{
}

size_t Parser::feed(rust::Slice<const uint8_t> bytes)
{
  for (const uint8_t byte : bytes) {
    packets_.process_byte(byte);
  }
  return queue_.size();
}

bool Parser::next(RawFrame & out)
{
  if (queue_.empty()) {
    return false;
  }
  const auto frame = std::move(queue_.front());
  queue_.pop_front();
  out.sync = frame.get_sync();
  out.frame_type = static_cast<uint8_t>(frame.get_type());
  out.bytes = to_rust(frame.data);
  return true;
}

std::unique_ptr<Parser> new_parser() { return std::make_unique<Parser>(); }

size_t parser_feed(Parser & parser, rust::Slice<const uint8_t> bytes) { return parser.feed(bytes); }

bool parser_next(Parser & parser, RawFrame & out) { return parser.next(out); }

ParserStats parser_stats(const Parser & parser)
{
  const auto & s = parser.stats();
  return ParserStats{
    s.total_bytes_processed, s.frames_decoded, s.sync_errors, s.length_errors, s.crc_errors};
}

std::unique_ptr<ParamAssembler> new_param_assembler() { return std::make_unique<ParamAssembler>(); }

void assembler_start(ParamAssembler & assembler, uint8_t number) { assembler.inner.start(number); }

bool assembler_feed(ParamAssembler & assembler, const ParamChunk & chunk, rust::Vec<uint8_t> & out)
{
  crsf::ParameterEntryPayload payload;
  payload.ext_header = header(chunk.dest, chunk.origin);
  payload.parameter_number = chunk.number;
  payload.chunks_remaining = chunk.chunks_remaining;
  payload.data.assign(chunk.data.begin(), chunk.data.end());
  const auto joined = assembler.inner.feed(payload);
  if (!joined) {
    return false;
  }
  out = to_rust(*joined);
  return true;
}

uint8_t assembler_next_chunk(const ParamAssembler & assembler)
{
  return assembler.inner.next_chunk();
}

bool assembler_active(const ParamAssembler & assembler) { return assembler.inner.active(); }

bool decode_rc_channels(rust::Slice<const uint8_t> frame, rust::Slice<uint16_t> out)
{
  const auto checked = checked_frame(frame);
  if (!checked || out.size() < 16) {
    return false;
  }
  const auto message = crsf::RCChannelsMessage::from_frame(*checked);
  if (!message) {
    return false;
  }
  for (size_t i = 0; i < 16; ++i) {
    out[i] = message->payload.channels[i];
  }
  return true;
}

bool decode_link_stats(rust::Slice<const uint8_t> frame, LinkStats & out)
{
  const auto checked = checked_frame(frame);
  if (!checked) {
    return false;
  }
  const auto message = crsf::LinkStatisticsMessage::from_frame(*checked);
  if (!message) {
    return false;
  }
  out = from_payload(message->payload);
  return true;
}

bool decode_battery(rust::Slice<const uint8_t> frame, Battery & out)
{
  const auto checked = checked_frame(frame);
  if (!checked) {
    return false;
  }
  const auto message = crsf::BatterySensorMessage::from_frame(*checked);
  if (!message) {
    return false;
  }
  out = Battery{
    message->payload.voltage, message->payload.current, message->payload.usedCapacity,
    message->payload.batteryPercent};
  return true;
}

bool decode_flight_mode(rust::Slice<const uint8_t> frame, rust::String & out)
{
  const auto checked = checked_frame(frame);
  if (!checked) {
    return false;
  }
  const auto message = crsf::FlightModeMessage::from_frame(*checked);
  if (!message) {
    return false;
  }
  // rust::String requires UTF-8; anything outside printable ASCII becomes '?'
  std::string text = message->payload.mode;
  for (char & c : text) {
    if (c < 0x20 || c > 0x7E) {
      c = '?';
    }
  }
  out = rust::String(text);
  return true;
}

bool decode_opentx_sync(rust::Slice<const uint8_t> frame, OpenTxSync & out)
{
  const auto checked = checked_frame(frame);
  if (!checked) {
    return false;
  }
  const auto message = crsf::OpenTxSyncMessage::from_frame(*checked);
  if (!message) {
    return false;
  }
  out = OpenTxSync{
    raw(message->payload.ext_header.ext_dest_addr), raw(message->payload.ext_header.ext_src_addr),
    message->payload.update_interval, message->payload.offset};
  return true;
}

bool decode_device_ping(rust::Slice<const uint8_t> frame, uint8_t & dest, uint8_t & origin)
{
  const auto checked = checked_frame(frame);
  if (!checked) {
    return false;
  }
  const auto message = crsf::DevicePingMessage::from_frame(*checked);
  if (!message) {
    return false;
  }
  dest = raw(message->payload.ext_header.ext_dest_addr);
  origin = raw(message->payload.ext_header.ext_src_addr);
  return true;
}

bool decode_device_info(rust::Slice<const uint8_t> frame, DeviceInfo & out)
{
  const auto checked = checked_frame(frame);
  if (!checked) {
    return false;
  }
  const auto message = crsf::DeviceInfoMessage::from_frame(*checked);
  if (!message) {
    return false;
  }
  std::string name = message->payload.device_name;
  for (char & c : name) {
    if (c < 0x20 || c > 0x7E) {
      c = '?';
    }
  }
  const auto & p = message->payload;
  out.dest = raw(p.ext_header.ext_dest_addr);
  out.origin = raw(p.ext_header.ext_src_addr);
  out.name = rust::String(name);
  out.serial_number = p.serial_number;
  out.hardware_id = p.hardware_id;
  out.firmware_id = p.firmware_id;
  out.parameters_total = p.parameters_total;
  out.parameter_version = p.parameter_version;
  return true;
}

bool decode_param_chunk(rust::Slice<const uint8_t> frame, ParamChunk & out)
{
  const auto checked = checked_frame(frame);
  if (!checked) {
    return false;
  }
  const auto message = crsf::ParameterEntryMessage::from_frame(*checked);
  if (!message) {
    return false;
  }
  const auto & p = message->payload;
  out.dest = raw(p.ext_header.ext_dest_addr);
  out.origin = raw(p.ext_header.ext_src_addr);
  out.number = p.parameter_number;
  out.chunks_remaining = p.chunks_remaining;
  out.data = to_rust(p.data);
  return true;
}

bool decode_param_read(
  rust::Slice<const uint8_t> frame, uint8_t & dest, uint8_t & origin, uint8_t & number,
  uint8_t & chunk)
{
  const auto checked = checked_frame(frame);
  if (!checked) {
    return false;
  }
  const auto message = crsf::ParameterReadMessage::from_frame(*checked);
  if (!message) {
    return false;
  }
  dest = raw(message->payload.ext_header.ext_dest_addr);
  origin = raw(message->payload.ext_header.ext_src_addr);
  number = message->payload.parameter_number;
  chunk = message->payload.chunk_number;
  return true;
}

bool decode_param_write(
  rust::Slice<const uint8_t> frame, uint8_t & dest, uint8_t & origin, uint8_t & number,
  rust::Vec<uint8_t> & data)
{
  const auto checked = checked_frame(frame);
  if (!checked) {
    return false;
  }
  const auto message = crsf::ParameterWriteMessage::from_frame(*checked);
  if (!message) {
    return false;
  }
  dest = raw(message->payload.ext_header.ext_dest_addr);
  origin = raw(message->payload.ext_header.ext_src_addr);
  number = message->payload.parameter_number;
  data = to_rust(message->payload.data);
  return true;
}

bool parse_param_entry(uint8_t number, rust::Slice<const uint8_t> data, ParamEntry & out)
{
  const auto info = crsf::parse_parameter_entry(number, to_std(data));
  if (!info) {
    return false;
  }
  const auto ascii = [](std::string text) {
    for (char & c : text) {
      if (static_cast<unsigned char>(c) > 0x7E) {
        c = '?';
      }
    }
    return rust::String(text);
  };
  out.number = info->number;
  out.parent = info->parent;
  out.data_type = static_cast<uint8_t>(info->type);
  out.hidden = info->hidden;
  out.name = ascii(info->name);
  out.value = info->value;
  out.min = info->min;
  out.max = info->max;
  out.default_value = info->default_value;
  out.decimal_point = info->decimal_point;
  out.step = info->step;
  out.unit = ascii(info->unit);
  out.options.clear();
  for (const auto & option : info->options) {
    out.options.push_back(ascii(option));
  }
  out.text = ascii(info->text);
  out.max_length = info->max_length;
  out.children = to_rust(info->children);
  out.status = static_cast<uint8_t>(info->status);
  out.timeout = info->timeout;
  return true;
}

rust::Vec<uint8_t> encode_rc_channels(uint8_t sync, rust::Slice<const uint16_t> channels_us)
{
  crsf::RCChannelsMessage message;
  for (size_t i = 0; i < message.payload.channels.size(); ++i) {
    message.payload.channels[i] = i < channels_us.size() ? channels_us[i] : 1500;
  }
  return to_rust(message.to_frame(addr(sync)).data);
}

rust::Vec<uint8_t> encode_link_stats(uint8_t sync, const LinkStats & s)
{
  crsf::LinkStatisticsMessage message;
  message.payload = {
    s.uplink_rssi_ant1,
    s.uplink_rssi_ant2,
    s.uplink_link_quality,
    s.uplink_snr,
    s.active_antenna,
    s.rf_mode,
    static_cast<crsf::RFPower>(s.uplink_tx_power),
    s.downlink_rssi,
    s.downlink_link_quality,
    s.downlink_snr};
  return to_rust(message.to_frame(addr(sync)).data);
}

rust::Vec<uint8_t> encode_battery(uint8_t sync, const Battery & battery)
{
  crsf::BatterySensorMessage message;
  message.payload = {battery.voltage, battery.current, battery.used_mah, battery.percent};
  return to_rust(message.to_frame(addr(sync)).data);
}

rust::Vec<uint8_t> encode_flight_mode(uint8_t sync, rust::Str mode)
{
  crsf::FlightModeMessage message;
  message.payload.mode = std::string(mode);
  return to_rust(message.to_frame(addr(sync)).data);
}

rust::Vec<uint8_t> encode_opentx_sync(uint8_t sync, const OpenTxSync & payload)
{
  crsf::OpenTxSyncMessage message;
  message.payload.ext_header = header(payload.dest, payload.origin);
  message.payload.update_interval = payload.interval;
  message.payload.offset = payload.offset;
  return to_rust(message.to_frame(addr(sync)).data);
}

rust::Vec<uint8_t> encode_device_ping(uint8_t sync, uint8_t dest, uint8_t origin)
{
  crsf::DevicePingMessage message;
  message.payload.ext_header = header(dest, origin);
  return to_rust(message.to_frame(addr(sync)).data);
}

rust::Vec<uint8_t> encode_device_info(uint8_t sync, const DeviceInfo & info)
{
  crsf::DeviceInfoMessage message;
  message.payload.ext_header = header(info.dest, info.origin);
  message.payload.device_name = std::string(info.name);
  message.payload.serial_number = info.serial_number;
  message.payload.hardware_id = info.hardware_id;
  message.payload.firmware_id = info.firmware_id;
  message.payload.parameters_total = info.parameters_total;
  message.payload.parameter_version = info.parameter_version;
  return to_rust(message.to_frame(addr(sync)).data);
}

rust::Vec<uint8_t> encode_param_chunk(uint8_t sync, const ParamChunk & chunk)
{
  crsf::ParameterEntryMessage message;
  message.payload.ext_header = header(chunk.dest, chunk.origin);
  message.payload.parameter_number = chunk.number;
  message.payload.chunks_remaining = chunk.chunks_remaining;
  message.payload.data.assign(chunk.data.begin(), chunk.data.end());
  return to_rust(message.to_frame(addr(sync)).data);
}

rust::Vec<uint8_t> encode_param_read(
  uint8_t sync, uint8_t dest, uint8_t origin, uint8_t number, uint8_t chunk)
{
  crsf::ParameterReadMessage message;
  message.payload.ext_header = header(dest, origin);
  message.payload.parameter_number = number;
  message.payload.chunk_number = chunk;
  return to_rust(message.to_frame(addr(sync)).data);
}

rust::Vec<uint8_t> encode_param_write(
  uint8_t sync, uint8_t dest, uint8_t origin, uint8_t number, rust::Slice<const uint8_t> data)
{
  crsf::ParameterWriteMessage message;
  message.payload.ext_header = header(dest, origin);
  message.payload.parameter_number = number;
  message.payload.data = to_std(data);
  return to_rust(message.to_frame(addr(sync)).data);
}

rust::Vec<uint8_t> encode_param_value(uint8_t data_type, int32_t value)
{
  return to_rust(
    crsf::encode_parameter_value(static_cast<crsf::ParameterDataType>(data_type & 0x7F), value));
}

rust::Vec<uint8_t> reencode(rust::Slice<const uint8_t> frame)
{
  const auto checked = checked_frame(frame);
  if (!checked) {
    return {};
  }
  const auto sync = addr(checked->get_sync());
  std::optional<crsf::Message::Frame> out;
  switch (checked->get_type()) {
    case crsf::MessageType::RC_CHANNELS_PACKED:
      if (auto m = crsf::RCChannelsMessage::from_frame(*checked)) out = m->to_frame(sync);
      break;
    case crsf::MessageType::LINK_STATISTICS:
      if (auto m = crsf::LinkStatisticsMessage::from_frame(*checked)) out = m->to_frame(sync);
      break;
    case crsf::MessageType::BATTERY_SENSOR:
      if (auto m = crsf::BatterySensorMessage::from_frame(*checked)) out = m->to_frame(sync);
      break;
    case crsf::MessageType::FLIGHT_MODE:
      if (auto m = crsf::FlightModeMessage::from_frame(*checked)) out = m->to_frame(sync);
      break;
    case crsf::MessageType::RADIO_ID:
      if (auto m = crsf::OpenTxSyncMessage::from_frame(*checked)) out = m->to_frame(sync);
      break;
    case crsf::MessageType::DEVICE_PING:
      if (auto m = crsf::DevicePingMessage::from_frame(*checked)) out = m->to_frame(sync);
      break;
    case crsf::MessageType::DEVICE_INFO:
      if (auto m = crsf::DeviceInfoMessage::from_frame(*checked)) out = m->to_frame(sync);
      break;
    case crsf::MessageType::PARAMETER_SETTINGS_ENTRY:
      if (auto m = crsf::ParameterEntryMessage::from_frame(*checked)) out = m->to_frame(sync);
      break;
    case crsf::MessageType::PARAMETER_READ:
      if (auto m = crsf::ParameterReadMessage::from_frame(*checked)) out = m->to_frame(sync);
      break;
    case crsf::MessageType::PARAMETER_WRITE:
      if (auto m = crsf::ParameterWriteMessage::from_frame(*checked)) out = m->to_frame(sync);
      break;
    default:
      break;
  }
  return out ? to_rust(out->data) : rust::Vec<uint8_t>{};
}

}  // namespace elrs_crsf_ffi
