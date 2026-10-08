//! The whole daemon on real threads: pty "UART" + fake TX module, the TX loop, the IPC socket
//! and a client. The gamepad is injected into the shared input state (the evdev reader is the
//! only part not exercised; it needs a uinput device, see test_tools::virtual_pad).

use control_daemon::config::Config;
use control_daemon::engine::Engine;
use control_daemon::input::SharedInput;
use control_daemon::mapping::{InputState, Mapping, MappingConfig};
use control_daemon::protocol::{read_message, write_message};
use control_daemon::runtime::TxLoop;
use control_daemon::ipc;
use crossbeam_channel::bounded;
use serde_json::{json, Value};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use test_tools::{fake_tx::FakeTx, pty::Pty};

const BTN_TL: u16 = 0x136;
const BTN_TR: u16 = 0x137;
const ABS_RX: u16 = 0x03;

struct System {
    fake: Arc<Mutex<FakeTx>>,
    input: SharedInput,
    socket: std::path::PathBuf,
    shutdown: Arc<AtomicBool>,
    threads: Vec<std::thread::JoinHandle<()>>,
    _pty: Arc<Pty>,
}

impl System {
    fn start(name: &str) -> Self {
        let pty = Arc::new(Pty::open().unwrap());
        let shutdown = Arc::new(AtomicBool::new(false));
        let fake = Arc::new(Mutex::new(FakeTx::new(Instant::now())));
        let mut threads = Vec::new();

        {
            let (pty, fake, shutdown) = (pty.clone(), fake.clone(), shutdown.clone());
            threads.push(std::thread::spawn(move || {
                while !shutdown.load(Ordering::Relaxed) {
                    let out = fake.lock().unwrap().poll(&pty.read_available(), Instant::now());
                    if !out.is_empty() {
                        pty.write(&out);
                    }
                    std::thread::sleep(Duration::from_micros(300));
                }
            }));
        }

        let mut config = Config::default();
        config.serial.port = pty.slave_path.display().to_string();
        // macOS serialport only accepts a pty with baud 0 (it skips the speed ioctl)
        config.serial.baud = if cfg!(target_os = "macos") { 0 } else { 115_200 };
        config.tx.mlock = false;
        config.tx.rt_priority = 0;
        let socket = std::path::PathBuf::from(format!("/tmp/rc-e2e-{}-{name}.sock", std::process::id()));
        config.ipc.socket_path = socket.clone();

        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/config/mapping.toml")).unwrap();
        let mapping = Mapping::new(toml::from_str::<MappingConfig>(&text).unwrap()).unwrap();
        let input: SharedInput = Arc::new(Mutex::new(InputState::default()));
        {
            let mut state = input.lock().unwrap();
            state.device_present = true;
            state.seq += 1;
        }

        let (out_tx, out_rx) = bounded(256);
        let (req_tx, req_rx) = bounded(64);
        let server = ipc::start(&socket, out_rx, req_tx, shutdown.clone()).unwrap();
        threads.extend(server.threads);
        let engine = Engine::new(config.clone(), mapping, out_tx, req_rx);
        threads.push(TxLoop { engine, config, input: input.clone(), shutdown: shutdown.clone() }.spawn());

        Self { fake, input, socket, shutdown, threads, _pty: pty }
    }

    fn client(&self) -> UnixStream {
        for _ in 0..50 {
            if let Ok(stream) = UnixStream::connect(&self.socket) {
                stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                return stream;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("daemon socket never appeared");
    }

    fn rc_count(&self) -> usize {
        self.fake.lock().unwrap().rc_log.len()
    }

    fn set_key(&self, code: u16, down: bool) {
        let mut state = self.input.lock().unwrap();
        state.set_key(code, down);
        state.seq += 1;
    }

    /// Mean RC frame period over the last `window` frames
    fn recent_period(&self, window: usize) -> Duration {
        let fake = self.fake.lock().unwrap();
        let log = &fake.rc_log[fake.rc_log.len() - window..];
        (log[window - 1].at - log[0].at) / (window as u32 - 1)
    }
}

impl Drop for System {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
        let _ = std::fs::remove_file(&self.socket);
    }
}

fn wait_until(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

fn next_of(stream: &mut UnixStream, kind: &str) -> Value {
    for _ in 0..500 {
        let message: Value = serde_json::from_str(&read_message(stream).unwrap()).unwrap();
        if message["type"] == kind {
            return message;
        }
    }
    panic!("no {kind} message");
}

fn request(stream: &mut UnixStream, body: Value) -> Value {
    let id = body["id"].clone();
    write_message(stream, &body.to_string()).unwrap();
    for _ in 0..500 {
        let message: Value = serde_json::from_str(&read_message(stream).unwrap()).unwrap();
        if message["type"] == "ack" && message["id"] == id {
            return message;
        }
    }
    panic!("no ack");
}

#[test]
fn frames_flow_and_period_follows_sync_changes() {
    let system = System::start("sync");
    assert!(wait_until(Duration::from_secs(5), || system.rc_count() > 100), "no RC frames");
    let period = system.recent_period(100);
    assert!(
        period > Duration::from_micros(3500) && period < Duration::from_micros(4600),
        "default period {period:?}"
    );

    // The module changes its rate; the period converges within 1 s
    system.fake.lock().unwrap().sync_interval_100ns = 100_000;
    std::thread::sleep(Duration::from_millis(1100));
    let start = system.rc_count();
    assert!(wait_until(Duration::from_secs(3), || system.rc_count() > start + 40));
    let period = system.recent_period(40);
    assert!(
        period > Duration::from_micros(9000) && period < Duration::from_micros(11_500),
        "10 ms period expected, got {period:?}"
    );
}

#[test]
fn killing_a_client_never_affects_the_tx_loop() {
    let system = System::start("kill");
    assert!(wait_until(Duration::from_secs(5), || system.rc_count() > 100));

    let mut clients: Vec<UnixStream> = (0..4).map(|_| system.client()).collect();
    // A client that never reads: its queue fills and it is dropped
    let _stalled = system.client();
    for client in &mut clients {
        let _ = next_of(client, "telemetry");
    }
    let before = system.recent_period(100);

    for client in clients.drain(..) {
        let _ = client.shutdown(std::net::Shutdown::Both);
        drop(client);
    }
    std::thread::sleep(Duration::from_millis(500));
    let start = system.rc_count();
    std::thread::sleep(Duration::from_secs(1));
    let frames = system.rc_count() - start;
    let after = system.recent_period(100);

    assert!(frames > 150, "only {frames} frames in 1 s after the clients died");
    let drift = before.abs_diff(after);
    assert!(drift < Duration::from_micros(800), "period {before:?} -> {after:?}");
}

#[test]
fn telemetry_reaches_clients_at_10_hz_and_params_load() {
    let system = System::start("telemetry");
    let mut client = system.client();
    let hello = next_of(&mut client, "hello");
    assert_eq!(hello["version"], 1);

    let start = Instant::now();
    let mut count = 0;
    let mut params = None;
    while start.elapsed() < Duration::from_secs(3) {
        let message: Value = serde_json::from_str(&read_message(&mut client).unwrap()).unwrap();
        match message["type"].as_str() {
            Some("telemetry") => count += 1,
            Some("params") if message["complete"] == true => params = Some(message),
            _ => {}
        }
    }
    assert!((26..=34).contains(&count), "{count} snapshots in 3 s");
    assert_eq!(params.expect("complete params message")["device"]["name"], "ELRS 2.4GHz TX");

    let telemetry = next_of(&mut client, "telemetry");
    assert_eq!(telemetry["link"]["lq"], 100);
    assert_eq!(telemetry["status"]["text"], "RDY");
    assert_eq!(telemetry["tx"]["synced"], true);
    assert_eq!(telemetry["module"]["connected"], true);
}

#[test]
fn param_write_is_confirmed_by_the_module() {
    let system = System::start("write");
    let mut client = system.client();
    let params = loop {
        let message = next_of(&mut client, "params");
        if message["complete"] == true {
            break message;
        }
    };
    let power = params["params"].as_array().unwrap().iter().find(|p| p["name"] == "Max Power").unwrap();
    let ack = request(&mut client, json!({"type":"param_write","id":11,"number":power["number"],"value":0}));
    assert_eq!(ack["ok"], true, "{ack}");
    assert_eq!(system.fake.lock().unwrap().model.value_of("Max Power"), Some(0));
    let ack = request(&mut client, json!({"type":"param_write","id":12,"number":power["number"],"value":99}));
    assert_eq!(ack["ok"], false);
}

#[test]
fn ipc_cannot_arm_but_the_gamepad_can() {
    let system = System::start("arm");
    let mut client = system.client();
    assert!(wait_until(Duration::from_secs(5), || system.rc_count() > 50));

    for body in [
        json!({"type":"arm","id":1}),
        json!({"type":"set_aux1","id":2}),
        json!({"type":"override_set","id":3,"channel":4,"value_us":2000,"ttl_ms":500}),
        json!({"type":"override_set","id":4,"channel":0,"value_us":2000,"ttl_ms":500}),
    ] {
        let _ = request(&mut client, body);
    }
    std::thread::sleep(Duration::from_millis(100));
    {
        let fake = system.fake.lock().unwrap();
        assert!(
            fake.rc_log.iter().all(|rc| rc.channels[4] == 1000),
            "AUX1 went high without the arm gesture"
        );
    }

    // Now the real gesture: L1 + R1 for 1 s, release, R1 again
    system.set_key(BTN_TL, true);
    system.set_key(BTN_TR, true);
    assert!(wait_until(Duration::from_secs(3), || next_of(&mut client, "telemetry")["arm"]["state"] == "armed"));
    system.set_key(BTN_TL, false);
    system.set_key(BTN_TR, false);
    std::thread::sleep(Duration::from_millis(100));
    system.set_key(BTN_TR, true);
    assert!(wait_until(Duration::from_secs(2), || system.fake.lock().unwrap().rc_log.last().unwrap().channels[4] == 2000));
    {
        let mut state = system.input.lock().unwrap();
        state.set_axis(ABS_RX, 1800);
        state.seq += 1;
    }
    assert!(wait_until(Duration::from_secs(2), || system.fake.lock().unwrap().rc_log.last().unwrap().channels[0] == 2000));

    // The gamepad disappearing disarms at once
    {
        let mut state = system.input.lock().unwrap();
        state.device_present = false;
        state.seq += 1;
    }
    assert!(wait_until(Duration::from_secs(2), || {
        let fake = system.fake.lock().unwrap();
        let last = fake.rc_log.last().unwrap();
        last.channels[4] == 1000 && last.channels[0] == 1500
    }));
}

#[test]
fn module_power_loss_raises_the_loud_alarm_within_a_second() {
    let system = System::start("alarm");
    let mut client = system.client();
    assert!(wait_until(Duration::from_secs(5), || system.rc_count() > 50));
    std::thread::sleep(Duration::from_millis(500));
    let _ = next_of(&mut client, "telemetry");

    let cut = Instant::now();
    system.fake.lock().unwrap().muted = true;
    let alarm = loop {
        let message = next_of(&mut client, "alarm");
        if message["id"] == "elrs_lost" && message["active"] == true {
            break message;
        }
    };
    assert_eq!(alarm["level"], "loud");
    assert!(cut.elapsed() < Duration::from_millis(1100), "alarm after {:?}", cut.elapsed());

    // A client connecting now gets the active alarm in its initial state
    let mut late = system.client();
    let replayed = loop {
        let message = next_of(&mut late, "alarm");
        if message["id"] == "elrs_lost" {
            break message;
        }
    };
    assert_eq!(replayed["active"], true);
}
