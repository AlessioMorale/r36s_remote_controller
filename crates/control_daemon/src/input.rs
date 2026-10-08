//! Gamepad input: evdev devices opened with an exclusive grab, merged into one [`InputState`].
//!
//! Input events only update shared state; they never drive frame timing (design §3.1). If a
//! device disappears, `device_present` goes false and the safety logic disarms.

use crate::config::InputConfig;
use crate::mapping::InputState;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

pub type SharedInput = Arc<Mutex<InputState>>;

#[cfg(target_os = "linux")]
pub use linux::spawn;

/// Without evdev (desktop builds) there is no gamepad: the daemon stays disarmed
#[cfg(not(target_os = "linux"))]
pub fn spawn(_config: InputConfig, _shared: SharedInput, shutdown: Arc<AtomicBool>) -> JoinHandle<()> {
    std::thread::spawn(move || {
        log::warn!("no evdev on this platform: input unavailable");
        while !shutdown.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    })
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use evdev::{Device, EventType, InputEvent};
    use std::os::fd::AsRawFd;
    use std::time::{Duration, SystemTime};

    const RESCAN: Duration = Duration::from_millis(500);

    struct Opened {
        path: String,
        device: Device,
    }

    pub fn spawn(config: InputConfig, shared: SharedInput, shutdown: Arc<AtomicBool>) -> JoinHandle<()> {
        std::thread::Builder::new()
            .name("input".into())
            .spawn(move || run(config, shared, shutdown))
            .expect("spawn input thread")
    }

    fn matches(pattern: &str, path: &str, device: &Device) -> bool {
        path == pattern || device.name().is_some_and(|name| name.contains(pattern))
    }

    /// One device per configured pattern; None unless every pattern is present
    fn open_all(config: &InputConfig) -> Option<Vec<Opened>> {
        let mut found: Vec<Option<Opened>> = config.devices.iter().map(|_| None).collect();
        for (path, device) in evdev::enumerate() {
            let path = path.display().to_string();
            for (slot, pattern) in found.iter_mut().zip(&config.devices) {
                if slot.is_none() && matches(pattern, &path, &device) {
                    // Opened a second time for the next pattern would grab twice: take it once
                    *slot = Some(Opened { path: path.clone(), device });
                    break;
                }
            }
        }
        let mut opened: Vec<Opened> = found.into_iter().collect::<Option<_>>()?;
        for entry in &mut opened {
            if config.grab {
                if let Err(error) = entry.device.grab() {
                    log::warn!("grab {}: {error}", entry.path);
                    return None;
                }
            }
            if let Err(error) = entry.device.set_nonblocking(true) {
                log::warn!("nonblocking {}: {error}", entry.path);
                return None;
            }
        }
        Some(opened)
    }

    /// Loads the device's current axes and keys so a stick held off-centre at start is seen
    fn load_initial_state(state: &mut InputState, device: &Device) {
        if let Ok(abs) = device.get_abs_state() {
            for (code, info) in abs.iter().enumerate() {
                if let Some(supported) = device.supported_absolute_axes() {
                    if supported.contains(evdev::AbsoluteAxisCode(code as u16)) {
                        state.set_axis(code as u16, info.value);
                    }
                }
            }
        }
        if let Ok(keys) = device.get_key_state() {
            for key in keys.iter() {
                state.set_key(key.code(), true);
            }
        }
    }

    fn apply(state: &mut InputState, event: &InputEvent) {
        match event.event_type() {
            EventType::KEY => state.set_key(event.code(), event.value() != 0),
            EventType::ABSOLUTE => state.set_axis(event.code(), event.value()),
            _ => return,
        }
        state.seq += 1;
        state.last_event = Some(event.timestamp());
    }

    fn run(config: InputConfig, shared: SharedInput, shutdown: Arc<AtomicBool>) {
        while !shutdown.load(Ordering::Relaxed) {
            let Some(mut devices) = open_all(&config) else {
                std::thread::sleep(RESCAN);
                continue;
            };
            log::info!(
                "input devices: {}",
                devices.iter().map(|d| d.path.as_str()).collect::<Vec<_>>().join(", ")
            );
            {
                let mut state = shared.lock().unwrap();
                for entry in &devices {
                    load_initial_state(&mut state, &entry.device);
                }
                state.device_present = true;
                state.seq += 1;
                state.last_event = Some(SystemTime::now());
            }
            if let Err(error) = read_until_lost(&mut devices, &shared, &shutdown) {
                log::warn!("input lost: {error}");
            }
            let mut state = shared.lock().unwrap();
            state.device_present = false;
            state.release_all();
            state.seq += 1;
        }
    }

    fn read_until_lost(
        devices: &mut [Opened],
        shared: &SharedInput,
        shutdown: &AtomicBool,
    ) -> std::io::Result<()> {
        while !shutdown.load(Ordering::Relaxed) {
            let mut fds: Vec<libc::pollfd> = devices
                .iter()
                .map(|d| libc::pollfd { fd: d.device.as_raw_fd(), events: libc::POLLIN, revents: 0 })
                .collect();
            // SAFETY: `fds` is a valid array of `fds.len()` pollfd structures
            let ready = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, 100) };
            if ready < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            for (entry, fd) in devices.iter_mut().zip(&fds) {
                if fd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                    return Err(std::io::Error::other(format!("{} hung up", entry.path)));
                }
                if fd.revents & libc::POLLIN == 0 {
                    continue;
                }
                match entry.device.fetch_events() {
                    Ok(events) => {
                        let mut state = shared.lock().unwrap();
                        for event in events {
                            apply(&mut state, &event);
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(error) => return Err(error),
                }
            }
        }
        Ok(())
    }
}
