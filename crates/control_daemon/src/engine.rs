//! The daemon's brain, with no threads and no I/O: bytes from the TX module go in, the next
//! bytes for the module come out. The runtime (`runtime.rs`) calls it from the TX thread;
//! tests drive it directly with a fake module.
//!
//! The IPC side is non-blocking by construction: outgoing messages are `try_send`, incoming
//! requests are `try_recv`, so a stuck UI can never delay a frame (design R4).

use crate::config::Config;
use crate::histogram::Histogram;
use crate::mapping::{
    ButtonSource, Calibration, CalibrationRecorder, InputState, Mapping, CENTER_US,
};
use crate::params::{ModuleConfig, ParamEvent, ParamView};
use crate::protocol::{
    ack, AlarmEvent, ArmView, DeviceView, InputView, ModuleView, Out, OverrideView, ParamsMessage,
    Request, Snapshot, TxView,
};
use crate::safety::{Safety, SafetyInputs, SafetyOutput};
use crate::telemetry::{Alarm, AlarmInputs, Telemetry};
use crossbeam_channel::{Receiver, Sender, TrySendError};
use elrs_crsf::{decode, encode, Frame, Parser};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

const SNAPSHOT_PERIOD: Duration = Duration::from_millis(100);
const ALARM_PERIOD: Duration = Duration::from_millis(50);
const PARAMS_PERIOD: Duration = Duration::from_millis(250);
/// Requests handled per tick, so a flood cannot steal time from the frame
const MAX_REQUESTS_PER_TICK: usize = 8;

/// A request from a UI client with the queue to answer on
pub struct ClientRequest {
    pub request: Request,
    pub reply: Sender<Arc<String>>,
}

/// Counts presses of a button between ticks: rising edges of the key counter, or of the
/// level for D-pad hats
#[derive(Default)]
struct EdgeDetector {
    last_count: Option<u32>,
    last_level: bool,
}

impl EdgeDetector {
    fn update(&mut self, source: &ButtonSource, state: &InputState) -> u32 {
        let level = source.pressed(state);
        let delta = match source.presses(state) {
            Some(count) => {
                // The first tick only records the baseline, so a held button is not a press
                let delta = self.last_count.map_or(0, |last| count.wrapping_sub(last));
                self.last_count = Some(count);
                delta
            }
            None => u32::from(level && !self.last_level && self.last_count.is_some()),
        };
        if source.presses(state).is_none() {
            self.last_count.get_or_insert(0);
        }
        self.last_level = level;
        delta
    }
}

pub struct TickOutput {
    /// Bytes for the TX module: the RC frame, then at most one parameter frame
    pub bytes: Vec<u8>,
    /// Kernel timestamp of the newest input event, set on the first tick after it arrived
    pub input_event: Option<SystemTime>,
}

pub struct Engine {
    config: Config,
    mapping: Mapping,
    safety: Safety,
    telemetry: Telemetry,
    module: ModuleConfig,
    parser: Parser,

    period: Duration,
    /// Phase correction from OPENTX_SYNC, applied once at the next deadline (ns, signed)
    pending_offset_ns: Option<i64>,
    synced: bool,
    last_sync: Option<Instant>,

    arm_edge: EdgeDetector,
    turbo_edge: EdgeDetector,
    menu_edge: EdgeDetector,
    menu_button_edges: Vec<EdgeDetector>,
    last_input_seq: u64,

    calibration: Option<CalibrationRecorder>,
    calibration_requested: bool,
    calibration_path: PathBuf,
    device_present: bool,

    serial_ok: bool,
    channels: [u16; 16],
    last_output: Option<SafetyOutput>,
    frames_sent: u64,
    snapshot_seq: u64,
    last_snapshot: Option<Instant>,
    last_alarm_check: Option<Instant>,
    active_alarms: BTreeMap<&'static str, Alarm>,
    params_dirty: bool,
    last_params_sent: Option<Instant>,

    out: Sender<Out>,
    requests: Receiver<ClientRequest>,
    /// Writes waiting for the module's confirmation: engine id -> (client request id, reply)
    pending_writes: HashMap<u64, (i64, Sender<Arc<String>>)>,
    next_write_id: u64,

    pub latency: Arc<Histogram>,
    pub jitter: Arc<Histogram>,
}

impl Engine {
    pub fn new(
        config: Config,
        mapping: Mapping,
        out: Sender<Out>,
        requests: Receiver<ClientRequest>,
    ) -> Self {
        let period = Duration::from_micros(u64::from(config.tx.default_interval_us));
        Self {
            safety: Safety::new((&config.safety).into()),
            telemetry: Telemetry::new(config.telemetry.clone()),
            calibration_path: config.input.calibration.clone(),
            menu_button_edges: mapping.menu_buttons.iter().map(|_| EdgeDetector::default()).collect(),
            mapping,
            module: ModuleConfig::new(),
            parser: Parser::new(),
            period,
            pending_offset_ns: None,
            synced: false,
            last_sync: None,
            arm_edge: EdgeDetector::default(),
            turbo_edge: EdgeDetector::default(),
            menu_edge: EdgeDetector::default(),
            last_input_seq: 0,
            calibration: None,
            calibration_requested: false,
            device_present: true,
            serial_ok: false,
            channels: [CENTER_US; 16],
            last_output: None,
            frames_sent: 0,
            snapshot_seq: 0,
            last_snapshot: None,
            last_alarm_check: None,
            active_alarms: BTreeMap::new(),
            params_dirty: false,
            last_params_sent: None,
            out,
            requests,
            pending_writes: HashMap::new(),
            next_write_id: 1,
            latency: Arc::default(),
            jitter: Arc::default(),
            config,
        }
    }

    pub fn period(&self) -> Duration {
        self.period
    }

    pub fn synced(&self) -> bool {
        self.synced
    }

    pub fn set_serial_ok(&mut self, ok: bool) {
        self.serial_ok = ok;
    }

    pub fn safety(&self) -> &Safety {
        &self.safety
    }

    pub fn channels(&self) -> &[u16; 16] {
        &self.channels
    }

    pub fn telemetry(&self) -> &Telemetry {
        &self.telemetry
    }

    pub fn module(&self) -> &ModuleConfig {
        &self.module
    }

    pub fn apply_calibration(&mut self, calibration: &Calibration) {
        self.mapping.apply_calibration(calibration);
    }

    // ---- receiving --------------------------------------------------------------------------

    /// Feeds bytes read from the TX module's UART
    pub fn on_rx(&mut self, bytes: &[u8], now: Instant) {
        for raw in self.parser.feed(bytes) {
            let frame = decode(&raw);
            if let Frame::OpenTxSync(sync) = &frame {
                self.on_sync(sync.interval, sync.offset, now);
            }
            self.telemetry.on_frame(&frame, now);
            self.module.on_frame(&frame, now);
        }
    }

    fn on_sync(&mut self, interval_100ns: u32, offset_100ns: i32, now: Instant) {
        let interval_us = (interval_100ns / 10).clamp(
            self.config.tx.min_interval_us,
            self.config.tx.max_interval_us,
        );
        self.period = Duration::from_micros(u64::from(interval_us));
        // Positive: frames arrived too early, so the next one waits
        self.pending_offset_ns = Some(i64::from(offset_100ns) * 100);
        self.synced = true;
        self.last_sync = Some(now);
    }

    /// The next frame deadline after `last`: one period later, shifted by the module's last
    /// phase correction
    pub fn next_deadline(&mut self, last: Instant) -> Instant {
        let mut next = last + self.period;
        if let Some(offset_ns) = self.pending_offset_ns.take() {
            // Never move more than half a period in one step
            let limit = (self.period.as_nanos() / 2) as i64;
            let offset_ns = offset_ns.clamp(-limit, limit);
            next = if offset_ns >= 0 {
                next + Duration::from_nanos(offset_ns as u64)
            } else {
                next - Duration::from_nanos(offset_ns.unsigned_abs())
            };
        }
        next
    }

    pub fn record_latency(&self, event: SystemTime, written: SystemTime) {
        if let Ok(delay) = written.duration_since(event) {
            self.latency.record_us(delay.as_micros() as u64);
        }
    }

    /// Records how far the actual period was from the expected one
    pub fn record_period(&self, actual: Duration) {
        let error = actual.abs_diff(self.period);
        self.jitter.record_us(error.as_micros() as u64);
    }

    // ---- transmitting -----------------------------------------------------------------------

    /// Builds this period's bytes for the TX module
    pub fn tick(&mut self, now: Instant, input: &InputState) -> TickOutput {
        // Button edges, always tracked so nothing queues up while the menu is closed
        let arm_held = self.mapping.arm.pressed(input);
        let _ = self.arm_edge.update(&self.mapping.arm, input);
        let turbo_presses = self.turbo_edge.update(&self.mapping.turbo, input);
        let menu_presses = self.menu_edge.update(&self.mapping.menu, input);
        let menu_events: Vec<&'static str> = self
            .mapping
            .menu_buttons
            .iter()
            .zip(&mut self.menu_button_edges)
            .filter_map(|((name, source), edge)| (edge.update(source, input) > 0).then_some(*name))
            .collect();

        self.device_present = input.device_present;
        self.handle_requests(now);

        let safety_input = SafetyInputs {
            device_present: input.device_present,
            arm_held,
            deadman_held: self.mapping.deadman.pressed(input),
            sticks_neutral: self.mapping.sticks_neutral(input),
            turbo_presses,
            menu_presses,
        };
        let decision = self.safety.update(now, safety_input);

        if decision.menu_open {
            for name in menu_events {
                self.emit(Out::MenuInput(name));
            }
            if self.calibration_requested {
                // The center is read now, with the sticks released
                self.calibration_requested = false;
                self.calibration = Some(CalibrationRecorder::start(&self.mapping, input));
            }
            if let Some(recorder) = &mut self.calibration {
                recorder.sample(input);
            }
        } else {
            self.calibration = None;
            self.calibration_requested = false;
        }

        let mut bytes = Vec::new();
        self.channels = self.build_channels(input, &decision);
        if decision.send_frames {
            bytes.extend(encode::rc_channels(&self.channels));
            self.frames_sent += 1;
        }

        self.module.tick(now);
        if let Some(frame) = self.module.take_outbox().into_iter().next() {
            bytes.extend(frame);
        }
        self.handle_param_events();

        self.publish(now, &decision);
        self.last_output = Some(decision);

        let input_event = (input.seq != self.last_input_seq).then_some(input.last_event).flatten();
        self.last_input_seq = input.seq;
        TickOutput { bytes, input_event }
    }

    fn build_channels(&self, input: &InputState, decision: &SafetyOutput) -> [u16; 16] {
        let mut channels = [CENTER_US; 16];
        if !decision.force_neutral {
            channels[..4].copy_from_slice(&self.mapping.axis_channels(input));
            for (channel, value) in decision.overrides.iter().enumerate() {
                if let Some(value) = value {
                    channels[channel] = *value;
                }
            }
        }
        channels[4] = self.mapping.aux_us(decision.aux1);
        channels[5] = self.mapping.aux_us(decision.turbo);
        channels[6] = self.mapping.aux_us(false);
        channels[7] = self.mapping.aux_us(false);
        channels
    }

    // ---- IPC --------------------------------------------------------------------------------

    fn emit(&self, message: Out) {
        // Dropping is fine: the UI gets the next snapshot in 100 ms. Never block here.
        if let Err(TrySendError::Disconnected(_)) = self.out.try_send(message) {
            // The IPC server is gone; control continues regardless
        }
    }

    fn handle_requests(&mut self, now: Instant) {
        for _ in 0..MAX_REQUESTS_PER_TICK {
            let Ok(ClientRequest { request, reply }) = self.requests.try_recv() else { break };
            let id = request.id();
            let result = self.handle_request(request, &reply, now);
            // Writes answer later, when the module confirms
            if let Some(result) = result {
                let _ = reply.try_send(Arc::new(ack(id, &result)));
            }
        }
    }

    /// `None`: the ack is deferred
    fn handle_request(
        &mut self,
        request: Request,
        reply: &Sender<Arc<String>>,
        now: Instant,
    ) -> Option<Result<(), String>> {
        Some(match request {
            // Answered by the IPC server before it reaches the engine
            Request::GetState { .. } => Ok(()),
            Request::ParamRefresh { .. } => {
                self.module.refresh();
                Ok(())
            }
            Request::ParamWrite { id, number, value } => {
                let engine_id = self.next_write_id;
                self.next_write_id += 1;
                match self.module.write(engine_id, number, value) {
                    Ok(()) => {
                        self.pending_writes.insert(engine_id, (id, reply.clone()));
                        return None;
                    }
                    Err(error) => Err(error),
                }
            }
            Request::OverrideSet { channel, value_us, ttl_ms, .. } => self
                .safety
                .set_override(now, channel, value_us, Duration::from_millis(ttl_ms))
                .map_err(|e| e.to_string()),
            Request::OverrideClear { channel, .. } => {
                self.safety.clear_override(channel);
                Ok(())
            }
            Request::MenuClose { .. } => {
                self.safety.close_menu();
                Ok(())
            }
            Request::CalibrationStart { .. } => {
                if !self.safety.menu_open() {
                    Err("calibration is only possible with the menu open".into())
                } else {
                    self.calibration_requested = true;
                    Ok(())
                }
            }
            Request::CalibrationFinish { save, .. } => match self.calibration.take() {
                None => {
                    self.calibration_requested = false;
                    Err("no calibration run in progress".into())
                }
                Some(recorder) if save => self.finish_calibration(recorder),
                Some(_) => Ok(()),
            },
        })
    }

    fn finish_calibration(&mut self, recorder: CalibrationRecorder) -> Result<(), String> {
        let calibration = recorder.finish();
        if calibration.axes.is_empty() {
            return Err("no stick movement recorded".into());
        }
        let text = toml::to_string(&calibration).map_err(|e| e.to_string())?;
        if let Some(dir) = self.calibration_path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(&self.calibration_path, text)
            .map_err(|e| format!("{}: {e}", self.calibration_path.display()))?;
        self.mapping.apply_calibration(&calibration);
        Ok(())
    }

    fn handle_param_events(&mut self) {
        for event in self.module.take_events() {
            match event {
                ParamEvent::TreeUpdated => self.params_dirty = true,
                ParamEvent::WriteDone { request_id, result } => {
                    if let Some((id, reply)) = self.pending_writes.remove(&request_id) {
                        let _ = reply.try_send(Arc::new(ack(id, &result)));
                    }
                    self.params_dirty = true;
                }
            }
        }
    }

    fn publish(&mut self, now: Instant, decision: &SafetyOutput) {
        let due = |last: Option<Instant>, period: Duration| {
            last.is_none_or(|at| now.saturating_duration_since(at) >= period)
        };

        if due(self.last_alarm_check, ALARM_PERIOD) {
            self.last_alarm_check = Some(now);
            self.update_alarms(now);
        }
        if self.params_dirty && due(self.last_params_sent, PARAMS_PERIOD) {
            self.params_dirty = false;
            self.last_params_sent = Some(now);
            self.emit(Out::Params(Box::new(self.params_message())));
        }
        if due(self.last_snapshot, SNAPSHOT_PERIOD) {
            self.last_snapshot = Some(now);
            let snapshot = self.snapshot(now, decision);
            self.emit(Out::Telemetry(Box::new(snapshot)));
        }
    }

    fn update_alarms(&mut self, now: Instant) {
        let current = self.telemetry.alarms(
            now,
            AlarmInputs { serial_ok: self.serial_ok, device_present: self.device_present },
        );
        // The text of an alarm changes while it lasts ("last update 1.2 s ago"); only a new
        // alarm or a change of level is an event
        for (id, alarm) in &current {
            if self.active_alarms.get(id).is_none_or(|old| old.level != alarm.level) {
                self.emit(Out::Alarm(AlarmEvent::raised(alarm)));
            }
        }
        for (id, alarm) in &self.active_alarms {
            if !current.contains_key(id) {
                self.emit(Out::Alarm(AlarmEvent::cleared(alarm)));
            }
        }
        self.active_alarms = current;
    }

    fn params_message(&self) -> ParamsMessage {
        ParamsMessage {
            kind: "params",
            complete: self.module.complete(),
            device: self.module.device().map(|d| DeviceView {
                name: d.name.clone(),
                serial: d.serial_number,
                firmware: d.firmware_id,
                parameters_total: d.parameters_total,
            }),
            params: self.module.params().values().map(ParamView::from).collect(),
        }
    }

    fn snapshot(&mut self, now: Instant, decision: &SafetyOutput) -> Snapshot {
        self.snapshot_seq += 1;
        Snapshot {
            kind: "telemetry",
            seq: self.snapshot_seq,
            link: self.telemetry.link(now),
            battery: self.telemetry.battery(now),
            status: self.telemetry.status(now),
            arm: ArmView { state: decision.arm_state, deadman: decision.deadman, aux1: decision.aux1 },
            input: InputView {
                mode: if decision.menu_open { "menu" } else { "drive" },
                device: self.device_present,
                turbo: decision.turbo,
            },
            channels: self.channels,
            tx: TxView {
                serial_open: self.serial_ok,
                synced: self.synced
                    && self.last_sync.is_some_and(|at| now.saturating_duration_since(at) < Duration::from_secs(1)),
                frame_period_us: self.period.as_micros() as u64,
                frames: self.frames_sent,
                input_latency_p99_us: self.latency.percentile_us(99.0),
                period_jitter_p99_us: self.jitter.percentile_us(99.0),
            },
            module: ModuleView {
                name: self.module.device().map(|d| d.name.clone()),
                connected: self.module.connected(now),
            },
            overrides: self
                .safety
                .override_status(now)
                .into_iter()
                .map(|(channel, value_us, left)| OverrideView {
                    channel,
                    value_us,
                    expires_in_ms: left.as_millis() as u64,
                })
                .collect(),
            calibrating: self.calibration.is_some() || self.calibration_requested,
        }
    }

    /// The current parameters message and active alarms, for a client that just connected
    pub fn current_state(&self) -> (ParamsMessage, Vec<AlarmEvent>) {
        (self.params_message(), self.active_alarms.values().map(AlarmEvent::raised).collect())
    }
}
