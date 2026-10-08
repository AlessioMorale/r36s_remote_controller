// Flat C++ facade over elrs_joy_crsf_protocol for the cxx bridge in src/lib.rs.
// SPDX-License-Identifier: mit
#pragma once

#include <cstdint>
#include <deque>
#include <memory>

#include "elrs_joy_crsf_protocol/crsf/packets.hpp"
#include "elrs_joy_crsf_protocol/crsf/parameter.hpp"
#include "rust/cxx.h"

namespace elrs_crsf_ffi
{
struct RawFrame;
struct ParserStats;
struct LinkStats;
struct Battery;
struct OpenTxSync;
struct DeviceInfo;
struct ParamChunk;
struct ParamEntry;

class Parser
{
public:
  Parser();
  size_t feed(rust::Slice<const uint8_t> bytes);
  bool next(RawFrame & out);
  const elrs_joy_crsf_protocol::crsf::Packets::Statistics & stats() const
  {
    return packets_.get_statistics();
  }

private:
  // Bounds the queue if the caller stops polling; oldest frames are dropped first
  static constexpr size_t MAX_QUEUED = 256;
  elrs_joy_crsf_protocol::crsf::Packets packets_;
  std::deque<elrs_joy_crsf_protocol::crsf::Message::Frame> queue_;
};

class ParamAssembler
{
public:
  elrs_joy_crsf_protocol::crsf::ParameterChunkAssembler inner;
};

std::unique_ptr<Parser> new_parser();
size_t parser_feed(Parser & parser, rust::Slice<const uint8_t> bytes);
bool parser_next(Parser & parser, RawFrame & out);
ParserStats parser_stats(const Parser & parser);

std::unique_ptr<ParamAssembler> new_param_assembler();
void assembler_start(ParamAssembler & assembler, uint8_t number);
bool assembler_feed(ParamAssembler & assembler, const ParamChunk & chunk, rust::Vec<uint8_t> & out);
uint8_t assembler_next_chunk(const ParamAssembler & assembler);
bool assembler_active(const ParamAssembler & assembler);

bool decode_rc_channels(rust::Slice<const uint8_t> frame, rust::Slice<uint16_t> out);
bool decode_link_stats(rust::Slice<const uint8_t> frame, LinkStats & out);
bool decode_battery(rust::Slice<const uint8_t> frame, Battery & out);
bool decode_flight_mode(rust::Slice<const uint8_t> frame, rust::String & out);
bool decode_opentx_sync(rust::Slice<const uint8_t> frame, OpenTxSync & out);
bool decode_device_ping(rust::Slice<const uint8_t> frame, uint8_t & dest, uint8_t & origin);
bool decode_device_info(rust::Slice<const uint8_t> frame, DeviceInfo & out);
bool decode_param_chunk(rust::Slice<const uint8_t> frame, ParamChunk & out);
bool decode_param_read(
  rust::Slice<const uint8_t> frame, uint8_t & dest, uint8_t & origin, uint8_t & number,
  uint8_t & chunk);
bool decode_param_write(
  rust::Slice<const uint8_t> frame, uint8_t & dest, uint8_t & origin, uint8_t & number,
  rust::Vec<uint8_t> & data);
bool parse_param_entry(uint8_t number, rust::Slice<const uint8_t> data, ParamEntry & out);

rust::Vec<uint8_t> encode_rc_channels(uint8_t sync, rust::Slice<const uint16_t> channels_us);
rust::Vec<uint8_t> encode_link_stats(uint8_t sync, const LinkStats & stats);
rust::Vec<uint8_t> encode_battery(uint8_t sync, const Battery & battery);
rust::Vec<uint8_t> encode_flight_mode(uint8_t sync, rust::Str mode);
rust::Vec<uint8_t> encode_opentx_sync(uint8_t sync, const OpenTxSync & sync_payload);
rust::Vec<uint8_t> encode_device_ping(uint8_t sync, uint8_t dest, uint8_t origin);
rust::Vec<uint8_t> encode_device_info(uint8_t sync, const DeviceInfo & info);
rust::Vec<uint8_t> encode_param_chunk(uint8_t sync, const ParamChunk & chunk);
rust::Vec<uint8_t> encode_param_read(
  uint8_t sync, uint8_t dest, uint8_t origin, uint8_t number, uint8_t chunk);
rust::Vec<uint8_t> encode_param_write(
  uint8_t sync, uint8_t dest, uint8_t origin, uint8_t number, rust::Slice<const uint8_t> data);
rust::Vec<uint8_t> encode_param_value(uint8_t data_type, int32_t value);

rust::Vec<uint8_t> reencode(rust::Slice<const uint8_t> frame);

}  // namespace elrs_crsf_ffi
