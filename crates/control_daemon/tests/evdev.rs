//! The evdev reader against a uinput virtual gamepad (Linux, needs /dev/uinput: run as root
//! or in the `input` group, with RC_TEST_UINPUT=1). Skipped otherwise.
#![cfg(target_os = "linux")]

use control_daemon::config::InputConfig;
use control_daemon::input;
use control_daemon::mapping::InputState;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use test_tools::virtual_pad::VirtualPad;

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

#[test]
fn reads_grabs_and_notices_the_device_disappearing() {
    if std::env::var_os("RC_TEST_UINPUT").is_none() {
        eprintln!("skipped: set RC_TEST_UINPUT=1 (needs /dev/uinput)");
        return;
    }
    let name = format!("RC test pad {}", std::process::id());
    let mut pad = VirtualPad::create(&name).expect("create virtual pad");
    std::thread::sleep(Duration::from_millis(300)); // let udev/devtmpfs create the node

    let shared = Arc::new(Mutex::new(InputState::default()));
    let shutdown = Arc::new(AtomicBool::new(false));
    let config = InputConfig { devices: vec![name.clone()], grab: true, ..Default::default() };
    let thread = input::spawn(config, shared.clone(), shutdown.clone());

    assert!(wait_until(Duration::from_secs(5), || shared.lock().unwrap().device_present));

    // Buttons and axes arrive, with kernel timestamps
    pad.key(0x136, 1).unwrap(); // BTN_TL
    pad.abs(0x03, 1500).unwrap(); // ABS_RX
    assert!(wait_until(Duration::from_secs(2), || {
        let state = shared.lock().unwrap();
        state.key(0x136) && state.axes[3] == 1500 && state.last_event.is_some()
    }));

    // The grab is exclusive: another reader (evtest) sees nothing while the daemon holds it
    let (_, mut other) = evdev::enumerate()
        .find(|(_, device)| device.name() == Some(name.as_str()))
        .expect("virtual pad node");
    other.set_nonblocking(true).unwrap();
    let seen: usize = other.fetch_events().map(|events| events.count()).unwrap_or(0);
    assert_eq!(seen, 0, "events leaked past EVIOCGRAB");
    pad.key(0x138, 1).unwrap();
    assert!(wait_until(Duration::from_secs(2), || shared.lock().unwrap().key(0x138)));
    let seen: usize = other.fetch_events().map(|events| events.count()).unwrap_or(0);
    assert_eq!(seen, 0, "events leaked past EVIOCGRAB");
    pad.key(0x138, 0).unwrap();
    drop(other);

    pad.key(0x136, 0).unwrap();
    assert!(wait_until(Duration::from_secs(2), || !shared.lock().unwrap().key(0x136)));
    assert_eq!(shared.lock().unwrap().presses(0x136), 1);

    // The device vanishing: not present, nothing left pressed
    pad.key(0x137, 1).unwrap();
    assert!(wait_until(Duration::from_secs(2), || shared.lock().unwrap().key(0x137)));
    drop(pad);
    assert!(wait_until(Duration::from_secs(5), || !shared.lock().unwrap().device_present));
    assert!(!shared.lock().unwrap().key(0x137));

    shutdown.store(true, Ordering::Relaxed);
    thread.join().unwrap();
}
