//! Safe CRSF API for the handheld, backed by the robot's C++ library (`elrs_crsf_sys`).
//!
//! * [`Parser`] turns raw serial bytes into CRC-checked [`RawFrame`]s.
//! * [`decode`] turns a frame into a typed [`Frame`].
//! * [`encode`] builds frames for the TX module (sync `0xEE`) and, for test tools, frames a
//!   module would send (sync `0xEA`).
//! * [`ParamAssembler`] and [`parse_param_entry`] implement the chunked parameter protocol.

use elrs_crsf_sys::ffi;

pub use ffi::{Battery, DeviceInfo, LinkStats, OpenTxSync, ParamChunk, ParserStats, RawFrame};

/// CRSF device addresses (also used as sync bytes)
pub mod address {
    pub const BROADCAST: u8 = 0x00;
    pub const FLIGHT_CONTROLLER: u8 = 0xC8;
    /// The handset; frames from the TX module to us start with this
    pub const HANDSET: u8 = 0xEA;
    pub const RECEIVER: u8 = 0xEC;
    /// The TX module; frames from us to the module start with this
    pub const TX_MODULE: u8 = 0xEE;
    /// Origin used by the ExpressLRS Lua script for parameter requests
    pub const ELRS_LUA: u8 = 0xEF;
}

/// CRSF frame types used by the handheld
pub mod frame_type {
    pub const BATTERY_SENSOR: u8 = 0x08;
    pub const LINK_STATISTICS: u8 = 0x14;
    pub const RC_CHANNELS_PACKED: u8 = 0x16;
    pub const FLIGHT_MODE: u8 = 0x21;
    pub const DEVICE_PING: u8 = 0x28;
    pub const DEVICE_INFO: u8 = 0x29;
    pub const PARAMETER_SETTINGS_ENTRY: u8 = 0x2B;
    pub const PARAMETER_READ: u8 = 0x2C;
    pub const PARAMETER_WRITE: u8 = 0x2D;
    /// Carries the OPENTX_SYNC (0x10) timing correction as a sub-type
    pub const RADIO_ID: u8 = 0x3A;
}

pub const CHANNEL_COUNT: usize = 16;
pub const CHANNEL_MIN_US: u16 = 988;
pub const CHANNEL_MAX_US: u16 = 2012;

/// A decoded frame
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    RcChannels([u16; CHANNEL_COUNT]),
    LinkStatistics(LinkStats),
    Battery(Battery),
    FlightMode(String),
    OpenTxSync(OpenTxSync),
    DevicePing { dest: u8, origin: u8 },
    DeviceInfo(DeviceInfo),
    ParameterEntry(ParamChunk),
    ParameterRead { dest: u8, origin: u8, number: u8, chunk: u8 },
    ParameterWrite { dest: u8, origin: u8, number: u8, data: Vec<u8> },
    /// Valid CRC, but a type (or RADIO_ID sub-type) this crate does not decode
    Other { frame_type: u8 },
}

/// Streaming frame parser. Bytes that are not CRSF (boot messages, noise) are discarded.
pub struct Parser {
    inner: cxx::UniquePtr<ffi::Parser>,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    pub fn new() -> Self {
        Self { inner: ffi::new_parser() }
    }

    /// Feeds bytes; returns the frames completed by them
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<RawFrame> {
        ffi::parser_feed(self.inner.pin_mut(), bytes);
        let mut frames = Vec::new();
        let mut frame = RawFrame::default();
        while ffi::parser_next(self.inner.pin_mut(), &mut frame) {
            frames.push(std::mem::take(&mut frame));
        }
        frames
    }

    pub fn stats(&self) -> ParserStats {
        ffi::parser_stats(&self.inner)
    }
}

/// Decodes a complete frame (as produced by [`Parser`])
pub fn decode(frame: &RawFrame) -> Frame {
    decode_bytes(&frame.bytes)
}

/// Decodes complete frame bytes; malformed input decodes to `Other`
pub fn decode_bytes(bytes: &[u8]) -> Frame {
    let frame_type = if bytes.len() > 2 { bytes[2] } else { 0 };
    let decoded = match frame_type {
        frame_type::RC_CHANNELS_PACKED => {
            let mut channels = [0u16; CHANNEL_COUNT];
            ffi::decode_rc_channels(bytes, &mut channels).then_some(Frame::RcChannels(channels))
        }
        frame_type::LINK_STATISTICS => {
            let mut stats = LinkStats::default();
            ffi::decode_link_stats(bytes, &mut stats).then_some(Frame::LinkStatistics(stats))
        }
        frame_type::BATTERY_SENSOR => {
            let mut battery = Battery::default();
            ffi::decode_battery(bytes, &mut battery).then_some(Frame::Battery(battery))
        }
        frame_type::FLIGHT_MODE => {
            let mut mode = String::new();
            ffi::decode_flight_mode(bytes, &mut mode).then_some(Frame::FlightMode(mode))
        }
        frame_type::RADIO_ID => {
            let mut sync = OpenTxSync::default();
            ffi::decode_opentx_sync(bytes, &mut sync).then_some(Frame::OpenTxSync(sync))
        }
        frame_type::DEVICE_PING => {
            let (mut dest, mut origin) = (0, 0);
            ffi::decode_device_ping(bytes, &mut dest, &mut origin)
                .then_some(Frame::DevicePing { dest, origin })
        }
        frame_type::DEVICE_INFO => {
            let mut info = DeviceInfo::default();
            ffi::decode_device_info(bytes, &mut info).then_some(Frame::DeviceInfo(info))
        }
        frame_type::PARAMETER_SETTINGS_ENTRY => {
            let mut chunk = ParamChunk::default();
            ffi::decode_param_chunk(bytes, &mut chunk).then_some(Frame::ParameterEntry(chunk))
        }
        frame_type::PARAMETER_READ => {
            let (mut dest, mut origin, mut number, mut chunk) = (0, 0, 0, 0);
            ffi::decode_param_read(bytes, &mut dest, &mut origin, &mut number, &mut chunk)
                .then_some(Frame::ParameterRead { dest, origin, number, chunk })
        }
        frame_type::PARAMETER_WRITE => {
            let (mut dest, mut origin, mut number, mut data) = (0, 0, 0, Vec::new());
            ffi::decode_param_write(bytes, &mut dest, &mut origin, &mut number, &mut data)
                .then_some(Frame::ParameterWrite { dest, origin, number, data })
        }
        _ => None,
    };
    decoded.unwrap_or(Frame::Other { frame_type })
}

/// Re-encodes a frame with the library's message classes (golden-frame tests)
pub fn reencode(bytes: &[u8]) -> Option<Vec<u8>> {
    let out = ffi::reencode(bytes);
    (!out.is_empty()).then_some(out)
}

/// Frame builders. All return complete frames, ready to write to the UART.
pub mod encode {
    use super::{address, ffi, Battery, DeviceInfo, LinkStats, OpenTxSync, ParamChunk};

    /// RC channels to the TX module; values in µs, clamped to the CRSF range
    pub fn rc_channels(channels_us: &[u16; super::CHANNEL_COUNT]) -> Vec<u8> {
        let clamped: Vec<u16> = channels_us
            .iter()
            .map(|us| (*us).clamp(super::CHANNEL_MIN_US, super::CHANNEL_MAX_US))
            .collect();
        ffi::encode_rc_channels(address::TX_MODULE, &clamped)
    }

    pub fn device_ping() -> Vec<u8> {
        ffi::encode_device_ping(address::TX_MODULE, address::BROADCAST, address::HANDSET)
    }

    pub fn param_read(number: u8, chunk: u8) -> Vec<u8> {
        ffi::encode_param_read(address::TX_MODULE, address::TX_MODULE, address::ELRS_LUA, number, chunk)
    }

    pub fn param_write(number: u8, data: &[u8]) -> Vec<u8> {
        ffi::encode_param_write(address::TX_MODULE, address::TX_MODULE, address::ELRS_LUA, number, data)
    }

    /// Value bytes for a 0x2D write of a parameter of `data_type`
    pub fn param_value(data_type: u8, value: i32) -> Vec<u8> {
        ffi::encode_param_value(data_type, value)
    }

    // Module-side frames, for the fake TX module and tests

    pub fn link_stats(stats: &LinkStats) -> Vec<u8> {
        ffi::encode_link_stats(address::HANDSET, stats)
    }

    pub fn battery(battery: &Battery) -> Vec<u8> {
        ffi::encode_battery(address::HANDSET, battery)
    }

    pub fn flight_mode(mode: &str) -> Vec<u8> {
        ffi::encode_flight_mode(address::HANDSET, mode)
    }

    pub fn opentx_sync(interval_100ns: u32, offset_100ns: i32) -> Vec<u8> {
        let payload = OpenTxSync {
            dest: address::HANDSET,
            origin: address::TX_MODULE,
            interval: interval_100ns,
            offset: offset_100ns,
        };
        ffi::encode_opentx_sync(address::HANDSET, &payload)
    }

    pub fn device_info(info: &DeviceInfo) -> Vec<u8> {
        ffi::encode_device_info(address::HANDSET, info)
    }

    pub fn param_chunk(chunk: &ParamChunk) -> Vec<u8> {
        ffi::encode_param_chunk(address::HANDSET, chunk)
    }
}

/// Parameter data types (bit 7 of the wire value is the hidden flag)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamType {
    Uint8,
    Int8,
    Uint16,
    Int16,
    Uint32,
    Int32,
    Float,
    TextSelection,
    String,
    Folder,
    Info,
    Command,
    OutOfRange,
}

impl ParamType {
    pub fn from_raw(raw: u8) -> Option<Self> {
        Some(match raw & 0x7F {
            0x00 => Self::Uint8,
            0x01 => Self::Int8,
            0x02 => Self::Uint16,
            0x03 => Self::Int16,
            0x04 => Self::Uint32,
            0x05 => Self::Int32,
            0x08 => Self::Float,
            0x09 => Self::TextSelection,
            0x0A => Self::String,
            0x0B => Self::Folder,
            0x0C => Self::Info,
            0x0D => Self::Command,
            0x7F => Self::OutOfRange,
            _ => return None,
        })
    }

    pub fn raw(self) -> u8 {
        match self {
            Self::Uint8 => 0x00,
            Self::Int8 => 0x01,
            Self::Uint16 => 0x02,
            Self::Int16 => 0x03,
            Self::Uint32 => 0x04,
            Self::Int32 => 0x05,
            Self::Float => 0x08,
            Self::TextSelection => 0x09,
            Self::String => 0x0A,
            Self::Folder => 0x0B,
            Self::Info => 0x0C,
            Self::Command => 0x0D,
            Self::OutOfRange => 0x7F,
        }
    }

    /// Name used in the IPC schema (docs/ipc.md)
    pub fn name(self) -> &'static str {
        match self {
            Self::Uint8 => "uint8",
            Self::Int8 => "int8",
            Self::Uint16 => "uint16",
            Self::Int16 => "int16",
            Self::Uint32 => "uint32",
            Self::Int32 => "int32",
            Self::Float => "float",
            Self::TextSelection => "text_selection",
            Self::String => "string",
            Self::Folder => "folder",
            Self::Info => "info",
            Self::Command => "command",
            Self::OutOfRange => "out_of_range",
        }
    }
}

/// A decoded parameter entry
#[derive(Debug, Clone, PartialEq)]
pub struct ParamEntry {
    pub kind: ParamType,
    pub raw: ffi::ParamEntry,
}

/// Decodes the joined data of a parameter entry
pub fn parse_param_entry(number: u8, data: &[u8]) -> Option<ParamEntry> {
    let mut raw = ffi::ParamEntry::default();
    if !ffi::parse_param_entry(number, data, &mut raw) {
        return None;
    }
    Some(ParamEntry { kind: ParamType::from_raw(raw.data_type)?, raw })
}

/// Joins the chunks of one parameter entry; chunks must arrive in order
pub struct ParamAssembler {
    inner: cxx::UniquePtr<ffi::ParamAssembler>,
}

impl Default for ParamAssembler {
    fn default() -> Self {
        Self::new()
    }
}

impl ParamAssembler {
    pub fn new() -> Self {
        Self { inner: ffi::new_param_assembler() }
    }

    pub fn start(&mut self, number: u8) {
        ffi::assembler_start(self.inner.pin_mut(), number);
    }

    /// Returns the joined data once the last chunk is in
    pub fn feed(&mut self, chunk: &ParamChunk) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        ffi::assembler_feed(self.inner.pin_mut(), chunk, &mut out).then_some(out)
    }

    pub fn next_chunk(&self) -> u8 {
        ffi::assembler_next_chunk(&self.inner)
    }

    pub fn active(&self) -> bool {
        ffi::assembler_active(&self.inner)
    }
}
