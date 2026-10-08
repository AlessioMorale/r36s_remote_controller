//! The C++ library's golden frames (deps/elrs_joy/elrs_joy_crsf_protocol/test/fixtures),
//! read through the bindings: each must parse to one frame, decode, and re-encode byte-exact.

use elrs_crsf::{decode_bytes, parse_param_entry, reencode, Frame, ParamAssembler, ParamType, Parser};
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../deps/elrs_joy/elrs_joy_crsf_protocol/test/fixtures")
}

fn load(name: &str) -> Vec<u8> {
    let text = std::fs::read_to_string(fixtures_dir().join(format!("{name}.hex")))
        .unwrap_or_else(|e| panic!("fixture {name}: {e}"));
    text.lines()
        .filter(|line| !line.starts_with('#'))
        .flat_map(|line| line.split_whitespace())
        .map(|token| u8::from_str_radix(token, 16).unwrap())
        .collect()
}

fn single(name: &str) -> Frame {
    let bytes = load(name);
    let frames = Parser::new().feed(&bytes);
    assert_eq!(frames.len(), 1, "{name} must parse to one frame");
    assert_eq!(frames[0].bytes, bytes, "{name}");
    assert_eq!(reencode(&bytes).as_deref(), Some(bytes.as_slice()), "{name} re-encode");
    decode_bytes(&bytes)
}

#[test]
fn every_fixture_parses_and_reencodes() {
    let mut count = 0;
    for entry in std::fs::read_dir(fixtures_dir()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|ext| ext == "hex") {
            let name = path.file_stem().unwrap().to_str().unwrap().to_owned();
            assert!(!matches!(single(&name), Frame::Other { .. }), "{name} not decoded");
            count += 1;
        }
    }
    assert!(count >= 14, "only {count} fixtures found");
}

#[test]
fn rc_channels() {
    let Frame::RcChannels(ch) = single("rc_channels_to_tx") else { panic!() };
    assert_eq!(&ch[..6], &[1500, 2000, 1000, 1500, 2000, 1000]);
    // Encoding the same values must give the same bytes
    assert_eq!(elrs_crsf::encode::rc_channels(&ch), load("rc_channels_to_tx"));
}

#[test]
fn link_statistics() {
    let Frame::LinkStatistics(s) = single("link_statistics") else { panic!() };
    assert_eq!((s.uplink_rssi_ant1, s.uplink_link_quality, s.uplink_snr), (64, 100, 9));
    assert_eq!((s.uplink_tx_power, s.downlink_link_quality, s.downlink_snr), (3, 97, 8));
}

#[test]
fn battery_and_status() {
    let Frame::Battery(b) = single("battery_sensor") else { panic!() };
    assert!((b.voltage - 15.6).abs() < 1e-4 && (b.current - 1.2).abs() < 1e-4);
    assert_eq!((b.used_mah, b.percent), (450, 72));
    assert_eq!(single("flight_mode"), Frame::FlightMode("RDY".into()));
    assert_eq!(single("flight_mode_fault"), Frame::FlightMode("FLT:MOTOR_L".into()));
}

#[test]
fn opentx_sync() {
    let Frame::OpenTxSync(s) = single("opentx_sync") else { panic!() };
    assert_eq!((s.interval, s.offset), (40000, -1200));
    assert_eq!(elrs_crsf::encode::opentx_sync(40000, -1200), load("opentx_sync"));
}

#[test]
fn device_discovery() {
    assert_eq!(single("device_ping"), Frame::DevicePing { dest: 0x00, origin: 0xEA });
    assert_eq!(elrs_crsf::encode::device_ping(), load("device_ping"));
    let Frame::DeviceInfo(info) = single("device_info") else { panic!() };
    assert_eq!(info.name, "ELRS TX 2400");
    assert_eq!(info.parameters_total, 25);
}

#[test]
fn parameters() {
    assert_eq!(elrs_crsf::encode::param_read(1, 0), load("parameter_read"));
    let value = elrs_crsf::encode::param_value(ParamType::TextSelection.raw(), 3);
    assert_eq!(elrs_crsf::encode::param_write(5, &value), load("parameter_write"));

    let mut assembler = ParamAssembler::new();
    assembler.start(1);
    let mut joined = None;
    for chunk in 0..3 {
        assert_eq!(assembler.next_chunk(), chunk);
        let Frame::ParameterEntry(c) = single(&format!("parameter_entry_chunk{chunk}")) else {
            panic!()
        };
        joined = assembler.feed(&c);
    }
    let entry = parse_param_entry(1, &joined.unwrap()).unwrap();
    assert_eq!(entry.kind, ParamType::TextSelection);
    assert_eq!(entry.raw.name, "Packet Rate");
    assert_eq!(entry.raw.options.len(), 6);
    assert_eq!(entry.raw.value, 3);
}
