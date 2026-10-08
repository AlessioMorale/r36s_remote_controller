//! The engine against the fake TX module, in-process with a simulated clock.

use control_daemon::config::Config;
use control_daemon::engine::{ClientRequest, Engine};
use control_daemon::mapping::{InputState, Mapping, MappingConfig};
use control_daemon::protocol::{Out, Request};
use crossbeam_channel::{bounded, Receiver, Sender};
use elrs_crsf::{address, decode_bytes, Frame, Parser};
use std::sync::Arc;
use std::time::{Duration, Instant};
use test_tools::fake_tx::FakeTx;

// evdev codes of the R36S mapping (config/mapping.toml)
const ABS_X: u16 = 0x00;
const ABS_RX: u16 = 0x03;
const ABS_RY: u16 = 0x04;
const BTN_TL: u16 = 0x136;
const BTN_TR: u16 = 0x137;
const BTN_TR2: u16 = 0x139;
const BTN_SELECT: u16 = 0x13a;
const BTN_EAST: u16 = 0x131;

fn mapping() -> Mapping {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/config/mapping.toml")).unwrap();
    Mapping::new(toml::from_str::<MappingConfig>(&text).unwrap()).unwrap()
}

struct Rig {
    engine: Engine,
    tx: FakeTx,
    input: InputState,
    now: Instant,
    outputs: Receiver<Out>,
    requests: Sender<ClientRequest>,
    replies: (Sender<Arc<String>>, Receiver<Arc<String>>),
    /// What the module received, per tick: the RC channels, if a frame arrived
    rc_per_tick: Vec<Option<[u16; 16]>>,
}

impl Rig {
    fn new() -> Self {
        Self::with_config(Config::default())
    }

    fn with_config(config: Config) -> Self {
        let (out_tx, outputs) = bounded(4096);
        let (requests, req_rx) = bounded(64);
        let now = Instant::now();
        let mut engine = Engine::new(config, mapping(), out_tx, req_rx);
        engine.set_serial_ok(true);
        let mut input = InputState::default();
        input.device_present = true;
        Self {
            engine,
            tx: FakeTx::new(now),
            input,
            now,
            outputs,
            requests,
            replies: bounded(64),
            rc_per_tick: Vec::new(),
        }
    }

    /// One TX period: tick the engine, hand its bytes to the module, feed back the replies
    fn step(&mut self) {
        self.now += self.engine.period();
        let output = self.engine.tick(self.now, &self.input);
        let before = self.tx.rc_log.len();
        let reply = self.tx.poll(&output.bytes, self.now);
        self.rc_per_tick.push((self.tx.rc_log.len() > before).then(|| self.tx.rc_log.last().unwrap().channels));
        if !reply.is_empty() {
            self.engine.on_rx(&reply, self.now);
        }
    }

    fn run_ms(&mut self, ms: u64) {
        let end = self.now + Duration::from_millis(ms);
        while self.now < end {
            self.step();
        }
    }

    fn last_rc(&self) -> [u16; 16] {
        self.tx.rc_log.last().expect("no RC frame received").channels
    }

    fn press(&mut self, code: u16, down: bool) {
        self.input.set_key(code, down);
        self.input.seq += 1;
    }

    fn deflect_right_stick(&mut self, x: i32, y: i32) {
        self.input.set_axis(ABS_RX, x);
        self.input.set_axis(ABS_RY, y);
        self.input.seq += 1;
    }

    fn arm(&mut self) {
        self.run_ms(50);
        self.press(BTN_TL, true);
        self.press(BTN_TR, true);
        self.run_ms(1100);
        self.press(BTN_TL, false);
        self.press(BTN_TR, false);
        self.run_ms(50);
    }

    fn alarms(&self) -> Vec<(String, bool)> {
        self.outputs
            .try_iter()
            .filter_map(|o| match o {
                Out::Alarm(a) => Some((a.id.to_string(), a.active)),
                _ => None,
            })
            .collect()
    }

    fn request(&mut self, json: &str) -> Option<serde_json::Value> {
        let request = Request::parse(json).ok()?;
        self.requests.send(ClientRequest { request, reply: self.replies.0.clone() }).unwrap();
        self.step();
        self.replies.1.try_recv().ok().map(|text| serde_json::from_str(&text).unwrap())
    }
}

#[test]
fn startup_is_silent_until_sticks_are_neutral() {
    let mut rig = Rig::new();
    rig.deflect_right_stick(1800, 0);
    rig.run_ms(500);
    assert!(rig.tx.rc_log.is_empty(), "no RC frame may be sent with a stick off-centre");
    rig.deflect_right_stick(0, 0);
    rig.run_ms(50);
    assert!(!rig.tx.rc_log.is_empty());
    // Disarmed from the first frame on
    assert_eq!(rig.tx.rc_log.first().unwrap().channels[4], 1000);
}

#[test]
fn channel_contract() {
    let mut rig = Rig::new();
    rig.run_ms(100);
    assert_eq!(&rig.last_rc()[..8], &[1500, 1500, 1500, 1500, 1000, 1000, 1000, 1000]);

    // Right stick right + up is aileron / elevator high; deflection scales to 1000..2000
    rig.deflect_right_stick(1800, -1800);
    rig.run_ms(20);
    assert_eq!(&rig.last_rc()[..2], &[2000, 2000]);
    rig.deflect_right_stick(-1800, 1800);
    rig.run_ms(20);
    assert_eq!(&rig.last_rc()[..2], &[1000, 1000]);
    rig.input.set_axis(ABS_X, 1800);
    rig.deflect_right_stick(0, 0);
    rig.run_ms(20);
    assert_eq!(rig.last_rc()[3], 2000, "left stick X is the rudder channel");
    rig.input.set_axis(ABS_X, 0);
    rig.run_ms(20);

    // AUX1 follows arm + deadman; AUX2 is the turbo toggle
    rig.arm();
    assert_eq!(rig.last_rc()[4], 1000, "armed but R1 released");
    rig.press(BTN_TR, true);
    rig.run_ms(20);
    assert_eq!(rig.last_rc()[4], 2000);
    rig.press(BTN_TR2, true);
    rig.press(BTN_TR2, false);
    rig.run_ms(20);
    assert_eq!(rig.last_rc()[5], 2000);
    rig.press(BTN_TR, false);
    rig.run_ms(20);
    assert_eq!(rig.last_rc()[4], 1000);
    assert_eq!(&rig.last_rc()[6..8], &[1000, 1000]);
}

#[test]
fn menu_forces_neutral_and_aux1_low() {
    let mut rig = Rig::new();
    rig.arm();
    rig.press(BTN_TR, true);
    rig.deflect_right_stick(1800, -1800);
    rig.run_ms(20);
    assert_eq!(rig.last_rc()[..5], [2000, 2000, 1500, 1500, 2000]);

    rig.press(BTN_SELECT, true);
    rig.press(BTN_SELECT, false);
    rig.run_ms(20);
    let ch = rig.last_rc();
    assert_eq!(ch[..5], [1500, 1500, 1500, 1500, 1000], "menu open: neutral, AUX1 low");

    // D-pad / button events reach the UI while the menu is open, and drive nothing
    rig.press(BTN_EAST, true);
    rig.press(BTN_EAST, false);
    rig.run_ms(20);
    let menu_inputs: Vec<_> = rig
        .outputs
        .try_iter()
        .filter_map(|o| matches!(&o, Out::MenuInput(b) if *b == "a").then_some(o))
        .collect();
    assert_eq!(menu_inputs.len(), 1);
    assert_eq!(rig.last_rc()[..5], [1500, 1500, 1500, 1500, 1000]);
}

#[test]
fn frame_period_follows_the_module_sync() {
    let mut rig = Rig::new();
    rig.run_ms(500);
    assert!(rig.engine.synced());
    assert_eq!(rig.engine.period(), Duration::from_micros(4000));

    // The module switches to 150 Hz, then to 500 Hz; the period follows within 1 s
    for (interval_100ns, expected_us) in [(66_667, 6666), (20_000, 2000), (40_000, 4000)] {
        rig.tx.sync_interval_100ns = interval_100ns;
        rig.run_ms(1000);
        assert_eq!(rig.engine.period(), Duration::from_micros(expected_us), "{interval_100ns}");
    }
    // Absurd intervals are clamped, never obeyed
    rig.tx.sync_interval_100ns = 10;
    rig.run_ms(300);
    assert_eq!(rig.engine.period(), Duration::from_micros(1000));
    rig.tx.sync_interval_100ns = u32::MAX;
    rig.run_ms(300);
    assert_eq!(rig.engine.period(), Duration::from_micros(50_000));
}

#[test]
fn phase_offset_shifts_one_deadline_once() {
    let mut rig = Rig::new();
    rig.run_ms(300);
    let t0 = rig.now;
    let base = rig.engine.next_deadline(t0);
    assert_eq!(base, t0 + Duration::from_micros(4000));
    // Module says frames arrive 120 µs too early: the next deadline is 120 µs later, once
    let sync = elrs_crsf::encode::opentx_sync(40_000, 1_200);
    rig.engine.on_rx(&sync, t0);
    assert_eq!(rig.engine.next_deadline(t0), t0 + Duration::from_micros(4120));
    assert_eq!(rig.engine.next_deadline(t0), t0 + Duration::from_micros(4000));
    // Negative = late: sooner
    rig.engine.on_rx(&elrs_crsf::encode::opentx_sync(40_000, -500), t0);
    assert_eq!(rig.engine.next_deadline(t0), t0 + Duration::from_micros(3950));
}

#[test]
fn elrs_lost_alarm_within_one_second_of_module_power_loss() {
    let mut rig = Rig::new();
    // Until the first link statistics arrive the link counts as lost; it clears quickly
    rig.run_ms(1000);
    let startup = rig.alarms();
    assert!(startup.contains(&("elrs_lost".to_string(), false)), "{startup:?}");
    rig.run_ms(500);
    assert!(rig.alarms().is_empty(), "no alarms while the link is healthy");

    rig.tx.muted = true;
    let start = rig.now;
    let mut raised_after = None;
    while rig.now < start + Duration::from_millis(2000) && raised_after.is_none() {
        rig.step();
        if rig.alarms().iter().any(|(id, active)| id == "elrs_lost" && *active) {
            raised_after = Some(rig.now - start);
        }
    }
    let delay = raised_after.expect("alarm never raised");
    assert!(delay <= Duration::from_millis(1000), "alarm took {delay:?}");

    // And clears when the module comes back
    rig.tx.muted = false;
    rig.run_ms(500);
    assert!(rig.alarms().iter().any(|(id, active)| id == "elrs_lost" && !*active));
}

#[test]
fn no_ipc_request_can_arm_or_raise_aux1() {
    let mut rig = Rig::new();
    rig.run_ms(100);
    let requests = [
        r#"{"type":"arm","id":1}"#.to_string(),
        r#"{"type":"set_aux1","id":2,"value":1}"#.to_string(),
        r#"{"type":"override_set","id":3,"channel":4,"value_us":2000,"ttl_ms":1000}"#.to_string(),
        r#"{"type":"override_set","id":4,"channel":15,"value_us":2000,"ttl_ms":1000}"#.to_string(),
        r#"{"type":"param_write","id":5,"number":255,"value":1}"#.to_string(),
        r#"{"type":"menu_close","id":6}"#.to_string(),
        r#"{"type":"calibration_start","id":7}"#.to_string(),
        r#"{"type":"override_set","id":8,"channel":0,"value_us":1700,"ttl_ms":500}"#.to_string(),
    ];
    for json in &requests {
        let _ = rig.request(json);
        assert_eq!(rig.last_rc()[4], 1000, "AUX1 must stay low after {json}");
        assert_eq!(rig.engine.safety().arm_state(), control_daemon::safety::ArmState::Disarmed);
    }

    // Sweep every channel/value combination of override_set and a spread of param writes
    for channel in 0..16 {
        for value in [0u16, 1000, 1500, 2000, 2012, 65535] {
            let json = format!(
                r#"{{"type":"override_set","id":9,"channel":{channel},"value_us":{value},"ttl_ms":1000}}"#
            );
            let _ = rig.request(&json);
            assert_eq!(rig.last_rc()[4], 1000, "{json}");
            assert_eq!(rig.last_rc()[5], 1000, "{json}");
        }
    }
    assert_eq!(rig.engine.safety().arm_state(), control_daemon::safety::ArmState::Disarmed);

    // Rejected requests were answered with ok = false
    let reply = rig.request(r#"{"type":"override_set","id":10,"channel":4,"value_us":2000,"ttl_ms":100}"#).unwrap();
    assert_eq!(reply["ok"], false);
}

#[test]
fn override_applies_to_axes_only_and_expires() {
    let mut rig = Rig::new();
    rig.run_ms(100);
    let reply = rig.request(r#"{"type":"override_set","id":1,"channel":2,"value_us":1600,"ttl_ms":100}"#).unwrap();
    assert_eq!(reply["ok"], true);
    rig.run_ms(20);
    assert_eq!(rig.last_rc()[2], 1600);
    rig.run_ms(200);
    assert_eq!(rig.last_rc()[2], 1500, "expired without refresh");
}

#[test]
fn parameter_tree_and_confirmed_write() {
    let mut rig = Rig::new();
    rig.run_ms(3000);
    assert!(rig.engine.module().complete());
    let params = rig
        .outputs
        .try_iter()
        .filter_map(|o| match o {
            Out::Params(p) => Some(p),
            _ => None,
        })
        .last()
        .expect("params message");
    assert!(params.complete);
    let power = params.params.iter().find(|p| p.name == "Max Power").unwrap();
    assert_eq!(power.options.as_ref().unwrap().len(), 5);

    let json = format!(r#"{{"type":"param_write","id":42,"number":{},"value":1}}"#, power.number);
    assert!(rig.request(&json).is_none(), "ack is deferred until the module confirms");
    rig.run_ms(500);
    let ack: serde_json::Value = serde_json::from_str(&rig.replies.1.try_recv().unwrap()).unwrap();
    assert_eq!((ack["id"].as_i64(), ack["ok"].as_bool()), (Some(42), Some(true)), "{ack}");
    assert_eq!(rig.tx.model.value_of("Max Power"), Some(1));
}

#[test]
fn param_traffic_never_interrupts_rc_frames() {
    let mut rig = Rig::new();
    rig.run_ms(3000);
    // Every tick after the first RC frame carried one
    let first = rig.rc_per_tick.iter().position(Option::is_some).unwrap();
    assert!(rig.rc_per_tick[first..].iter().all(Option::is_some));
}

#[test]
fn calibration_run_saves_and_applies() {
    let dir = std::env::temp_dir().join(format!("rc-calib-{}", std::process::id()));
    let mut config = Config::default();
    config.input.calibration = dir.join("calibration.toml");
    let mut rig = Rig::with_config(config);
    rig.run_ms(100);
    assert_eq!(
        rig.request(r#"{"type":"calibration_start","id":1}"#).unwrap()["ok"],
        false,
        "needs the menu"
    );
    rig.press(BTN_SELECT, true);
    rig.press(BTN_SELECT, false);
    rig.run_ms(20);
    assert_eq!(rig.request(r#"{"type":"calibration_start","id":2}"#).unwrap()["ok"], true);
    rig.run_ms(20);
    // Move the right stick through a range narrower than the placeholder 1800
    for x in [-900, 0, 900] {
        rig.deflect_right_stick(x, 0);
        rig.run_ms(20);
    }
    rig.deflect_right_stick(0, 0);
    assert_eq!(
        rig.request(r#"{"type":"calibration_finish","id":3,"save":true}"#).unwrap()["ok"],
        true
    );
    let saved = std::fs::read_to_string(dir.join("calibration.toml")).unwrap();
    assert!(saved.contains("ABS_RX") && saved.contains("-900") && saved.contains("900"), "{saved}");

    // Leave the menu: 900 is now full deflection
    rig.press(BTN_SELECT, true);
    rig.press(BTN_SELECT, false);
    rig.deflect_right_stick(900, 0);
    rig.run_ms(20);
    assert_eq!(rig.last_rc()[0], 2000);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn rc_frames_use_the_module_address_and_decode() {
    let mut rig = Rig::new();
    rig.run_ms(50);
    let bytes = elrs_crsf::encode::rc_channels(&rig.last_rc());
    assert_eq!(bytes[0], address::TX_MODULE);
    let frames = Parser::new().feed(&bytes);
    assert!(matches!(decode_bytes(&frames[0].bytes), Frame::RcChannels(_)));
}
