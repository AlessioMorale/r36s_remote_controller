//! Raw `cxx` bindings over `elrs_joy_crsf_protocol` (design §3.5).
//!
//! The C++ library uses `std::optional`, `std::span`, templates and a `std::function`
//! callback, none of which `cxx` binds. `cpp/crsf_ffi.hpp` is a flat facade over it:
//! the frame callback becomes a poll (`parser_feed` + `parser_next`), payloads become the
//! shared structs below, and every decoder takes the complete frame bytes and returns
//! `false` instead of throwing or reading out of bounds. Use the safe `elrs_crsf` crate.

#[cxx::bridge(namespace = "elrs_crsf_ffi")]
pub mod ffi {
    /// One frame that passed the parser's sync, length and CRC checks
    #[derive(Debug, Clone, Default)]
    struct RawFrame {
        sync: u8,
        frame_type: u8,
        /// The whole frame: sync, length, type, payload, CRC
        bytes: Vec<u8>,
    }

    #[derive(Debug, Clone, Copy, Default, PartialEq)]
    struct ParserStats {
        total_bytes: u32,
        frames_decoded: u32,
        sync_errors: u32,
        length_errors: u32,
        crc_errors: u32,
    }

    #[derive(Debug, Clone, Copy, Default, PartialEq)]
    struct LinkStats {
        uplink_rssi_ant1: u8,
        uplink_rssi_ant2: u8,
        uplink_link_quality: u8,
        uplink_snr: i8,
        active_antenna: u8,
        rf_mode: u8,
        uplink_tx_power: u8,
        downlink_rssi: u8,
        downlink_link_quality: u8,
        downlink_snr: i8,
    }

    #[derive(Debug, Clone, Copy, Default, PartialEq)]
    struct Battery {
        voltage: f32,
        current: f32,
        used_mah: i32,
        percent: u8,
    }

    #[derive(Debug, Clone, Copy, Default, PartialEq)]
    struct OpenTxSync {
        dest: u8,
        origin: u8,
        /// LSB = 100 ns
        interval: u32,
        /// LSB = 100 ns, positive = frames arrive too early
        offset: i32,
    }

    #[derive(Debug, Clone, Default, PartialEq)]
    struct DeviceInfo {
        dest: u8,
        origin: u8,
        name: String,
        serial_number: u32,
        hardware_id: u32,
        firmware_id: u32,
        parameters_total: u8,
        parameter_version: u8,
    }

    /// One PARAMETER_SETTINGS_ENTRY frame (one chunk of an entry)
    #[derive(Debug, Clone, Default, PartialEq)]
    struct ParamChunk {
        dest: u8,
        origin: u8,
        number: u8,
        chunks_remaining: u8,
        data: Vec<u8>,
    }

    /// A decoded parameter entry; which fields are meaningful depends on `data_type`
    #[derive(Debug, Clone, Default, PartialEq)]
    struct ParamEntry {
        number: u8,
        parent: u8,
        data_type: u8,
        hidden: bool,
        name: String,
        value: i32,
        min: i32,
        max: i32,
        default_value: i32,
        decimal_point: u8,
        step: i32,
        unit: String,
        options: Vec<String>,
        text: String,
        max_length: u8,
        children: Vec<u8>,
        status: u8,
        timeout: u8,
    }

    unsafe extern "C++" {
        include!("cpp/crsf_ffi.hpp");

        type Parser;
        type ParamAssembler;

        fn new_parser() -> UniquePtr<Parser>;
        /// Feeds raw serial bytes; returns the number of frames now queued
        fn parser_feed(parser: Pin<&mut Parser>, bytes: &[u8]) -> usize;
        /// Pops the oldest queued frame into `out`; false when the queue is empty
        fn parser_next(parser: Pin<&mut Parser>, out: &mut RawFrame) -> bool;
        fn parser_stats(parser: &Parser) -> ParserStats;

        fn new_param_assembler() -> UniquePtr<ParamAssembler>;
        fn assembler_start(assembler: Pin<&mut ParamAssembler>, number: u8);
        /// Feeds one chunk; true when `out` holds the joined entry data
        fn assembler_feed(
            assembler: Pin<&mut ParamAssembler>,
            chunk: &ParamChunk,
            out: &mut Vec<u8>,
        ) -> bool;
        fn assembler_next_chunk(assembler: &ParamAssembler) -> u8;
        fn assembler_active(assembler: &ParamAssembler) -> bool;

        // Decoders: `frame` is a complete frame; false if invalid or of another type
        fn decode_rc_channels(frame: &[u8], out: &mut [u16]) -> bool;
        fn decode_link_stats(frame: &[u8], out: &mut LinkStats) -> bool;
        fn decode_battery(frame: &[u8], out: &mut Battery) -> bool;
        fn decode_flight_mode(frame: &[u8], out: &mut String) -> bool;
        fn decode_opentx_sync(frame: &[u8], out: &mut OpenTxSync) -> bool;
        fn decode_device_ping(frame: &[u8], dest: &mut u8, origin: &mut u8) -> bool;
        fn decode_device_info(frame: &[u8], out: &mut DeviceInfo) -> bool;
        fn decode_param_chunk(frame: &[u8], out: &mut ParamChunk) -> bool;
        fn decode_param_read(
            frame: &[u8],
            dest: &mut u8,
            origin: &mut u8,
            number: &mut u8,
            chunk: &mut u8,
        ) -> bool;
        fn decode_param_write(
            frame: &[u8],
            dest: &mut u8,
            origin: &mut u8,
            number: &mut u8,
            data: &mut Vec<u8>,
        ) -> bool;
        fn parse_param_entry(number: u8, data: &[u8], out: &mut ParamEntry) -> bool;

        // Encoders: return the complete frame
        fn encode_rc_channels(sync: u8, channels_us: &[u16]) -> Vec<u8>;
        fn encode_link_stats(sync: u8, stats: &LinkStats) -> Vec<u8>;
        fn encode_battery(sync: u8, battery: &Battery) -> Vec<u8>;
        fn encode_flight_mode(sync: u8, mode: &str) -> Vec<u8>;
        fn encode_opentx_sync(sync: u8, sync_payload: &OpenTxSync) -> Vec<u8>;
        fn encode_device_ping(sync: u8, dest: u8, origin: u8) -> Vec<u8>;
        fn encode_device_info(sync: u8, info: &DeviceInfo) -> Vec<u8>;
        fn encode_param_chunk(sync: u8, chunk: &ParamChunk) -> Vec<u8>;
        fn encode_param_read(sync: u8, dest: u8, origin: u8, number: u8, chunk: u8) -> Vec<u8>;
        fn encode_param_write(sync: u8, dest: u8, origin: u8, number: u8, data: &[u8]) -> Vec<u8>;
        fn encode_param_value(data_type: u8, value: i32) -> Vec<u8>;

        /// Decodes with the message class for the frame's type and re-encodes with the same
        /// sync byte; empty for unsupported types. Used by the golden-frame tests.
        fn reencode(frame: &[u8]) -> Vec<u8>;
    }
}

// The C++ objects have no thread affinity; each is used by one thread at a time.
unsafe impl Send for ffi::Parser {}
unsafe impl Send for ffi::ParamAssembler {}
