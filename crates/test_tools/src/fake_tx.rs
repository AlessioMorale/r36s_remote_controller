//! A fake ELRS TX module: answers pings and parameter requests, emits `OPENTX_SYNC` and
//! telemetry, and logs the frames it receives from the handset.
//!
//! [`ModuleModel`] is the protocol logic (frames in, frames out, no I/O) and is used directly
//! by unit tests. [`FakeTx`] wraps it around a byte stream (a pty in the `fake_tx` binary).

use elrs_crsf::{
    address, decode_bytes, encode, Battery, DeviceInfo, Frame, LinkStats, ParamChunk, ParamType,
    Parser,
};
use std::time::{Duration, Instant};

/// Payload bytes per 0x2B chunk (62-byte frame limit minus header, addresses, number, count)
pub const CHUNK_SIZE: usize = 56;

fn cstr(text: &str) -> Vec<u8> {
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    bytes
}

#[derive(Debug, Clone)]
enum Kind {
    Folder(Vec<u8>),
    Select { options: Vec<&'static str>, value: u8, unit: &'static str },
    Command { status: u8 },
    Info(&'static str),
}

#[derive(Debug, Clone)]
struct Param {
    parent: u8,
    name: &'static str,
    kind: Kind,
}

impl Param {
    fn encode(&self) -> Vec<u8> {
        let mut out = vec![self.parent];
        match &self.kind {
            Kind::Folder(children) => {
                out.push(ParamType::Folder.raw());
                out.extend(cstr(self.name));
                out.extend(children);
                out.push(0xFF);
            }
            Kind::Select { options, value, unit } => {
                out.push(ParamType::TextSelection.raw());
                out.extend(cstr(self.name));
                out.extend(cstr(&options.join(";")));
                out.extend([*value, 0, options.len() as u8 - 1, 0]);
                out.extend(cstr(unit));
            }
            Kind::Command { status } => {
                out.push(ParamType::Command.raw());
                out.extend(cstr(self.name));
                out.extend([*status, 20]);
                out.extend(cstr(""));
            }
            Kind::Info(text) => {
                out.push(ParamType::Info.raw());
                out.extend(cstr(self.name));
                out.extend(cstr(text));
            }
        }
        out
    }
}

/// Protocol logic of the module: no clock, no I/O
#[derive(Debug, Clone)]
pub struct ModuleModel {
    params: Vec<Param>,
    info: DeviceInfo,
    reply_counter: u32,
    drop_every: u32,
    /// Writes received, as (parameter number, value bytes)
    pub writes: Vec<(u8, Vec<u8>)>,
}

impl Default for ModuleModel {
    fn default() -> Self {
        // Layout modeled on an ExpressLRS TX module's Lua tree
        let params = vec![
            Param {
                parent: 0,
                name: "Packet Rate",
                kind: Kind::Select {
                    options: vec![
                        "50Hz(-115dBm)",
                        "100Hz Full(-112dBm)",
                        "150Hz(-112dBm)",
                        "250Hz(-108dBm)",
                        "333Hz Full(-105dBm)",
                        "500Hz(-105dBm)",
                    ],
                    value: 3,
                    unit: "",
                },
            },
            Param {
                parent: 0,
                name: "Telem Ratio",
                kind: Kind::Select {
                    options: vec!["Std", "Off", "1:128", "1:64", "1:32", "1:16", "1:8", "1:4", "1:2"],
                    value: 0,
                    unit: "",
                },
            },
            Param { parent: 0, name: "TX Power", kind: Kind::Folder(vec![4, 5]) },
            Param {
                parent: 3,
                name: "Max Power",
                kind: Kind::Select {
                    options: vec!["10", "25", "50", "100", "250"],
                    value: 3,
                    unit: "mW",
                },
            },
            Param {
                parent: 3,
                name: "Dynamic",
                kind: Kind::Select { options: vec!["Off", "Dyn", "AUX9"], value: 0, unit: "" },
            },
            Param { parent: 0, name: "Bind", kind: Kind::Command { status: 0 } },
            Param { parent: 0, name: "Version", kind: Kind::Info("3.5.3 ISRM") },
        ];
        let info = DeviceInfo {
            dest: address::HANDSET,
            origin: address::TX_MODULE,
            name: "ELRS 2.4GHz TX".into(),
            serial_number: 0x454C_5253,
            hardware_id: 0,
            firmware_id: 0x0003_0503,
            parameters_total: params.len() as u8,
            parameter_version: 0,
        };
        Self { params, info, reply_counter: 0, drop_every: 0, writes: Vec::new() }
    }
}

impl ModuleModel {
    pub fn device_info(&self) -> &DeviceInfo {
        &self.info
    }

    pub fn parameter_count(&self) -> usize {
        self.params.len()
    }

    /// Current option index of the named selection parameter
    pub fn value_of(&self, name: &str) -> Option<i32> {
        self.params.iter().find(|p| p.name == name).and_then(|p| match p.kind {
            Kind::Select { value, .. } => Some(i32::from(value)),
            _ => None,
        })
    }

    /// Simulates a lossy link: every n-th reply is dropped (0 = never)
    pub fn drop_every_nth_reply(&mut self, n: u32) {
        self.drop_every = n;
    }

    fn entry_chunk(&self, requester: u8, number: u8, chunk: u8) -> Option<ParamChunk> {
        let data = if number == 0 || usize::from(number) > self.params.len() {
            // Out of range marks the end of the list
            vec![0, ParamType::OutOfRange.raw()]
        } else {
            self.params[usize::from(number) - 1].encode()
        };
        let chunks = data.chunks(CHUNK_SIZE).collect::<Vec<_>>();
        let part = chunks.get(usize::from(chunk))?;
        Some(ParamChunk {
            dest: requester,
            origin: address::TX_MODULE,
            number,
            chunks_remaining: (chunks.len() - 1 - usize::from(chunk)) as u8,
            data: part.to_vec(),
        })
    }

    fn deliver(&mut self, frame: Vec<u8>, out: &mut Vec<Vec<u8>>) {
        self.reply_counter += 1;
        if self.drop_every != 0 && self.reply_counter % self.drop_every == 0 {
            return;
        }
        out.push(frame);
    }

    /// Frames the module sends in answer to a frame from the handset
    pub fn handle(&mut self, frame: &Frame) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        match frame {
            Frame::DevicePing { dest, .. } if *dest == address::BROADCAST || *dest == address::TX_MODULE => {
                let info = self.info.clone();
                self.deliver(encode::device_info(&info), &mut out);
            }
            Frame::ParameterRead { dest, origin, number, chunk } if *dest == address::TX_MODULE => {
                if let Some(entry) = self.entry_chunk(*origin, *number, *chunk) {
                    self.deliver(encode::param_chunk(&entry), &mut out);
                }
            }
            Frame::ParameterWrite { dest, number, data, .. } if *dest == address::TX_MODULE => {
                self.writes.push((*number, data.clone()));
                if let (Some(param), Some(&value)) =
                    (self.params.get_mut(usize::from(*number).wrapping_sub(1)), data.first())
                {
                    match &mut param.kind {
                        Kind::Select { options, value: current, .. } => {
                            if usize::from(value) < options.len() {
                                *current = value;
                            }
                        }
                        Kind::Command { status } => *status = if value == 1 { 2 } else { 0 },
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        out
    }
}

/// Telemetry the fake module generates
#[derive(Debug, Clone)]
pub struct TelemetrySource {
    pub link: LinkStats,
    pub battery: Battery,
    pub status: String,
}

impl Default for TelemetrySource {
    fn default() -> Self {
        Self {
            link: LinkStats {
                uplink_rssi_ant1: 64,
                uplink_rssi_ant2: 70,
                uplink_link_quality: 100,
                uplink_snr: 9,
                rf_mode: 7,
                uplink_tx_power: 3,
                downlink_rssi: 66,
                downlink_link_quality: 97,
                downlink_snr: 8,
                ..Default::default()
            },
            battery: Battery { voltage: 15.6, current: 1.2, used_mah: 450, percent: 72 },
            status: "RDY".into(),
        }
    }
}

/// An RC frame the module received, with its arrival time
#[derive(Debug, Clone)]
pub struct ReceivedRc {
    pub at: Instant,
    pub channels: [u16; 16],
}

/// The module on a byte stream. Call [`FakeTx::poll`] regularly with received bytes and the
/// current time; the returned bytes go back to the handset.
pub struct FakeTx {
    pub model: ModuleModel,
    pub telemetry: TelemetrySource,
    /// RC frame interval announced in OPENTX_SYNC, in 100 ns units (default 4 ms = 250 Hz)
    pub sync_interval_100ns: u32,
    /// Emit OPENTX_SYNC every this long (None disables)
    pub sync_period: Option<Duration>,
    /// Telemetry is sent at this period (None disables)
    pub telemetry_period: Option<Duration>,
    /// Frames to replay instead of generated telemetry, one per telemetry tick
    pub replay: Vec<Vec<u8>>,
    parser: Parser,
    pub rc_log: Vec<ReceivedRc>,
    pub frames_received: u64,
    started: Instant,
    last_sync: Option<Instant>,
    last_telemetry: Option<Instant>,
    replay_index: usize,
    telemetry_round: u64,
    /// Silences all output (simulates the module losing power)
    pub muted: bool,
}

impl FakeTx {
    pub fn new(now: Instant) -> Self {
        Self {
            model: ModuleModel::default(),
            telemetry: TelemetrySource::default(),
            sync_interval_100ns: 40_000,
            sync_period: Some(Duration::from_millis(100)),
            telemetry_period: Some(Duration::from_millis(100)),
            replay: Vec::new(),
            parser: Parser::new(),
            rc_log: Vec::new(),
            frames_received: 0,
            started: now,
            last_sync: None,
            last_telemetry: None,
            replay_index: 0,
            telemetry_round: 0,
            muted: false,
        }
    }

    pub fn uptime(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.started)
    }

    pub fn poll(&mut self, received: &[u8], now: Instant) -> Vec<u8> {
        let mut out: Vec<Vec<u8>> = Vec::new();
        for raw in self.parser.feed(received) {
            self.frames_received += 1;
            let frame = decode_bytes(&raw.bytes);
            if let Frame::RcChannels(channels) = frame {
                self.rc_log.push(ReceivedRc { at: now, channels });
                continue;
            }
            if !self.muted {
                out.extend(self.model.handle(&frame));
            }
        }

        if !self.muted {
            if let Some(period) = self.sync_period {
                if self.last_sync.is_none_or(|at| now.saturating_duration_since(at) >= period) {
                    self.last_sync = Some(now);
                    out.push(encode::opentx_sync(self.sync_interval_100ns, 0));
                }
            }
            if let Some(period) = self.telemetry_period {
                if self.last_telemetry.is_none_or(|at| now.saturating_duration_since(at) >= period) {
                    self.last_telemetry = Some(now);
                    out.extend(self.next_telemetry());
                }
            }
        }
        out.concat()
    }

    fn next_telemetry(&mut self) -> Vec<Vec<u8>> {
        if !self.replay.is_empty() {
            let frame = self.replay[self.replay_index % self.replay.len()].clone();
            self.replay_index += 1;
            return vec![frame];
        }
        // Link statistics every tick, battery every 10th, status every 5th: the real
        // module interleaves them within its telemetry ratio
        let round = self.telemetry_round;
        self.telemetry_round += 1;
        let mut frames = vec![encode::link_stats(&self.telemetry.link)];
        if round % 10 == 1 {
            frames.push(encode::battery(&self.telemetry.battery));
        }
        if round % 5 == 2 {
            frames.push(encode::flight_mode(&self.telemetry.status));
        }
        frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_ping_and_parameter_reads() {
        let now = Instant::now();
        let mut tx = FakeTx::new(now);
        tx.sync_period = None;
        tx.telemetry_period = None;
        let out = tx.poll(&encode::device_ping(), now);
        let mut parser = Parser::new();
        let frames = parser.feed(&out);
        assert_eq!(frames.len(), 1);
        let Frame::DeviceInfo(info) = decode_bytes(&frames[0].bytes) else { panic!() };
        assert_eq!(info.parameters_total as usize, tx.model.parameter_count());

        let out = tx.poll(&encode::param_read(1, 0), now);
        let Frame::ParameterEntry(chunk) = decode_bytes(&parser.feed(&out)[0].bytes) else { panic!() };
        assert_eq!((chunk.number, chunk.origin), (1, address::TX_MODULE));
        assert!(chunk.chunks_remaining > 0, "Packet Rate is a multi-chunk entry");
    }

    #[test]
    fn emits_sync_and_telemetry_and_logs_rc() {
        let t0 = Instant::now();
        let mut tx = FakeTx::new(t0);
        let out = tx.poll(&encode::rc_channels(&[1500; 16]), t0);
        assert_eq!(tx.rc_log.len(), 1);
        let kinds: Vec<_> = Parser::new().feed(&out).iter().map(|f| f.frame_type).collect();
        assert!(kinds.contains(&elrs_crsf::frame_type::RADIO_ID));
        assert!(kinds.contains(&elrs_crsf::frame_type::LINK_STATISTICS));
        // Nothing new within the period
        assert!(tx.poll(&[], t0 + Duration::from_millis(10)).is_empty());
    }

    #[test]
    fn muted_module_is_silent() {
        let t0 = Instant::now();
        let mut tx = FakeTx::new(t0);
        tx.muted = true;
        assert!(tx.poll(&encode::device_ping(), t0).is_empty());
    }
}
