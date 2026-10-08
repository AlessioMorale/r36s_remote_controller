//! Messages of the daemon ↔ UI socket (docs/ipc.md) and the length-prefixed framing.

use crate::params::ParamView;
use crate::safety::ArmState;
use crate::telemetry::{Alarm, AlarmLevel, BatteryView, LinkView, StatusView};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;

// ---- framing -------------------------------------------------------------------------------

/// Writes one message: 4-byte big-endian length, then the JSON text
pub fn write_message(out: &mut impl Write, json: &str) -> std::io::Result<()> {
    if json.len() > MAX_MESSAGE_BYTES {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "message too large"));
    }
    let mut buffer = Vec::with_capacity(4 + json.len());
    buffer.extend_from_slice(&(json.len() as u32).to_be_bytes());
    buffer.extend_from_slice(json.as_bytes());
    out.write_all(&buffer)
}

/// Reads one message; an oversized length or invalid UTF-8 is an error (close the connection)
pub fn read_message(input: &mut impl Read) -> std::io::Result<String> {
    let mut length = [0u8; 4];
    input.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_MESSAGE_BYTES {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "message too large"));
    }
    let mut body = vec![0u8; length];
    input.read_exact(&mut body)?;
    String::from_utf8(body)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid UTF-8"))
}

// ---- daemon -> UI --------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ArmView {
    pub state: ArmState,
    pub deadman: bool,
    pub aux1: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct InputView {
    /// `drive` or `menu`
    pub mode: &'static str,
    pub device: bool,
    pub turbo: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TxView {
    pub serial_open: bool,
    pub synced: bool,
    pub frame_period_us: u64,
    pub frames: u64,
    pub input_latency_p99_us: Option<u64>,
    pub period_jitter_p99_us: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModuleView {
    pub name: Option<String>,
    pub connected: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct OverrideView {
    pub channel: usize,
    pub value_us: u16,
    pub expires_in_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub seq: u64,
    pub link: Option<LinkView>,
    pub battery: Option<BatteryView>,
    pub status: Option<StatusView>,
    pub arm: ArmView,
    pub input: InputView,
    pub channels: [u16; 16],
    pub tx: TxView,
    pub module: ModuleView,
    pub overrides: Vec<OverrideView>,
    /// A stick calibration run is recording
    pub calibrating: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AlarmEvent {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub id: &'static str,
    pub level: AlarmLevel,
    pub active: bool,
    pub message: String,
}

impl AlarmEvent {
    pub fn raised(alarm: &Alarm) -> Self {
        Self {
            kind: "alarm",
            id: alarm.id,
            level: alarm.level,
            active: true,
            message: alarm.message.clone(),
        }
    }

    pub fn cleared(alarm: &Alarm) -> Self {
        Self {
            kind: "alarm",
            id: alarm.id,
            level: alarm.level,
            active: false,
            message: "cleared".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceView {
    pub name: String,
    pub serial: u32,
    pub firmware: u32,
    pub parameters_total: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsMessage {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub complete: bool,
    pub device: Option<DeviceView>,
    pub params: Vec<ParamView>,
}

/// What the engine hands to the IPC server; serialization happens off the TX thread
#[derive(Debug, Clone)]
pub enum Out {
    Telemetry(Box<Snapshot>),
    Alarm(AlarmEvent),
    Params(Box<ParamsMessage>),
    MenuInput(&'static str),
}

pub fn hello() -> String {
    serde_json::json!({
        "type": "hello",
        "version": PROTOCOL_VERSION,
        "daemon": env!("CARGO_PKG_VERSION"),
    })
    .to_string()
}

pub fn menu_input(button: &str) -> String {
    serde_json::json!({"type": "menu_input", "button": button}).to_string()
}

pub fn ack(id: i64, result: &Result<(), String>) -> String {
    match result {
        Ok(()) => serde_json::json!({"type": "ack", "id": id, "ok": true}),
        Err(error) => serde_json::json!({"type": "ack", "id": id, "ok": false, "error": error}),
    }
    .to_string()
}

// ---- UI -> daemon --------------------------------------------------------------------------

/// Requests the engine handles. `get_state` is answered by the IPC server itself.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    GetState { id: i64 },
    ParamRefresh { id: i64 },
    ParamWrite { id: i64, number: u8, value: i32 },
    OverrideSet { id: i64, channel: usize, value_us: u16, ttl_ms: u64 },
    OverrideClear { id: i64, channel: Option<usize> },
    MenuClose { id: i64 },
    CalibrationStart { id: i64 },
    CalibrationFinish { id: i64, save: bool },
}

impl Request {
    pub fn id(&self) -> i64 {
        match self {
            Self::GetState { id }
            | Self::ParamRefresh { id }
            | Self::ParamWrite { id, .. }
            | Self::OverrideSet { id, .. }
            | Self::OverrideClear { id, .. }
            | Self::MenuClose { id }
            | Self::CalibrationStart { id }
            | Self::CalibrationFinish { id, .. } => *id,
        }
    }

    /// Parses a request; on failure returns the ack to send back (id 0 if none was given)
    pub fn parse(json: &str) -> Result<Self, String> {
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|e| ack(0, &Err(format!("invalid JSON: {e}"))))?;
        let id = value.get("id").and_then(serde_json::Value::as_i64);
        let Some(id) = id else {
            return Err(ack(0, &Err("request needs an integer id".into())));
        };
        serde_json::from_value(value).map_err(|e| ack(id, &Err(format!("bad request: {e}"))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framing_round_trip_and_limits() {
        let mut buffer = Vec::new();
        write_message(&mut buffer, "{\"type\":\"x\"}").unwrap();
        assert_eq!(&buffer[..4], &[0, 0, 0, 12]);
        assert_eq!(read_message(&mut buffer.as_slice()).unwrap(), "{\"type\":\"x\"}");

        let oversized = ((MAX_MESSAGE_BYTES + 1) as u32).to_be_bytes();
        assert!(read_message(&mut oversized.as_slice()).is_err());
        assert!(write_message(&mut Vec::new(), &"x".repeat(MAX_MESSAGE_BYTES + 1)).is_err());
        let truncated = [0u8, 0, 0, 5, b'a'];
        assert!(read_message(&mut truncated.as_slice()).is_err());
        let bad_utf8 = [0u8, 0, 0, 1, 0xFF];
        assert!(read_message(&mut bad_utf8.as_slice()).is_err());
    }

    #[test]
    fn parses_requests() {
        assert_eq!(
            Request::parse(r#"{"type":"param_write","id":7,"number":5,"value":3}"#).unwrap(),
            Request::ParamWrite { id: 7, number: 5, value: 3 }
        );
        assert_eq!(
            Request::parse(r#"{"type":"override_clear","id":1}"#).unwrap(),
            Request::OverrideClear { id: 1, channel: None }
        );
    }

    #[test]
    fn unknown_requests_are_rejected_with_an_ack() {
        for json in [
            r#"{"type":"arm","id":3}"#,
            r#"{"type":"set_aux1","id":4,"value":1}"#,
            r#"{"type":"override_set","id":5,"channel":4,"value_us":2000}"#,
            r#"{"type":"param_write","id":6}"#,
            r#"{"id":8}"#,
            r#"{"type":"menu_close"}"#,
            "not json",
        ] {
            let ack = Request::parse(json).unwrap_err();
            let value: serde_json::Value = serde_json::from_str(&ack).unwrap();
            assert_eq!(value["type"], "ack", "{json}");
            assert_eq!(value["ok"], false, "{json}");
        }
    }
}
