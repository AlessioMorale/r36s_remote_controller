//! Threads around the engine: serial RX, the real-time TX loop, and serial reconnection.

use crate::config::{Config, SerialConfig, TxConfig};
use crate::engine::Engine;
use crate::input::SharedInput;
use crate::mapping::InputState;
use anyhow::{Context, Result};
use crossbeam_channel::{bounded, Receiver, Sender};
use serialport::{DataBits, FlowControl, Parity, SerialPort, StopBits};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

const REOPEN_INTERVAL: Duration = Duration::from_millis(500);
/// Spin for the last stretch before a deadline; sleeping alone overshoots by the timer slack
const SPIN_WINDOW: Duration = Duration::from_micros(150);

type RxChunk = (Instant, Vec<u8>);

fn open_serial(config: &SerialConfig) -> Result<(Box<dyn SerialPort>, Box<dyn SerialPort>)> {
    let port = serialport::new(&config.port, config.baud)
        .data_bits(DataBits::Eight)
        .parity(Parity::None)
        .stop_bits(StopBits::One)
        .flow_control(FlowControl::None)
        .timeout(Duration::from_millis(5))
        .open()
        .with_context(|| format!("open {} at {} baud", config.port, config.baud))?;
    let reader = port.try_clone().context("clone serial port")?;
    Ok((port, reader))
}

/// Reads the UART until it fails, forwarding bytes with their arrival time. Bytes that are
/// not CRSF (boot messages) are the parser's problem; they are never interpreted here.
fn spawn_rx(mut reader: Box<dyn SerialPort>, tx: Sender<RxChunk>, alive: Arc<AtomicBool>) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("serial-rx".into())
        .spawn(move || {
            let mut buffer = [0u8; 256];
            while alive.load(Ordering::Relaxed) {
                match reader.read(&mut buffer) {
                    Ok(0) => {}
                    Ok(n) => {
                        // Dropping on overflow is fine: the TX loop drains every period
                        let _ = tx.try_send((Instant::now(), buffer[..n].to_vec()));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {}
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(error) => {
                        log::error!("serial read: {error}");
                        alive.store(false, Ordering::Relaxed);
                    }
                }
            }
        })
        .expect("spawn serial rx thread")
}

/// Real-time scheduling and memory locking for the calling thread/process (Linux)
#[cfg(target_os = "linux")]
pub fn prepare_realtime(config: &TxConfig) {
    if config.mlock {
        // SAFETY: plain syscall without pointers
        if unsafe { libc::mlockall(libc::MCL_CURRENT | libc::MCL_FUTURE) } != 0 {
            log::warn!("mlockall failed: {}", std::io::Error::last_os_error());
        }
    }
    set_thread_priority(config.rt_priority);
}

#[cfg(not(target_os = "linux"))]
pub fn prepare_realtime(_config: &TxConfig) {
    log::warn!("real-time scheduling is only available on Linux");
}

/// SCHED_FIFO for the calling thread; 0 leaves it alone
#[cfg(target_os = "linux")]
pub fn set_thread_priority(priority: i32) {
    if priority <= 0 {
        return;
    }
    let param = libc::sched_param { sched_priority: priority };
    // SAFETY: `param` is a valid sched_param and pthread_self() is the calling thread
    let result = unsafe { libc::pthread_setschedparam(libc::pthread_self(), libc::SCHED_FIFO, &param) };
    if result != 0 {
        log::warn!(
            "SCHED_FIFO {priority} failed: {} (needs CAP_SYS_NICE / LimitRTPRIO)",
            std::io::Error::from_raw_os_error(result)
        );
    }
}

#[cfg(not(target_os = "linux"))]
pub fn set_thread_priority(_priority: i32) {}

fn sleep_until(deadline: Instant) {
    let now = Instant::now();
    if deadline > now + SPIN_WINDOW {
        std::thread::sleep(deadline - now - SPIN_WINDOW);
    }
    while Instant::now() < deadline {
        std::hint::spin_loop();
    }
}

pub struct TxLoop {
    pub engine: Engine,
    pub config: Config,
    pub input: SharedInput,
    pub shutdown: Arc<AtomicBool>,
}

impl TxLoop {
    pub fn spawn(self) -> JoinHandle<()> {
        std::thread::Builder::new()
            .name("tx".into())
            .spawn(move || self.run())
            .expect("spawn tx thread")
    }

    fn run(mut self) {
        // The process is locked and the TX thread real-time before the first frame
        prepare_realtime(&self.config.tx);

        let (chunk_tx, chunk_rx): (Sender<RxChunk>, Receiver<RxChunk>) = bounded(512);
        let alive = Arc::new(AtomicBool::new(false));
        let mut port: Option<Box<dyn SerialPort>> = None;
        let mut rx_thread: Option<JoinHandle<()>> = None;
        let mut last_open_attempt: Option<Instant> = None;

        let mut local_input = InputState::default();
        let mut deadline = Instant::now() + self.engine.period();
        let mut last_tick: Option<Instant> = None;

        while !self.shutdown.load(Ordering::Relaxed) {
            sleep_until(deadline);
            let now = Instant::now();

            // (Re)open the UART without ever blocking for long
            if !alive.load(Ordering::Relaxed) {
                port = None;
                if let Some(thread) = rx_thread.take() {
                    let _ = thread.join();
                }
                if last_open_attempt.is_none_or(|at| now.duration_since(at) >= REOPEN_INTERVAL) {
                    last_open_attempt = Some(now);
                    match open_serial(&self.config.serial) {
                        Ok((writer, reader)) => {
                            log::info!("serial {} open", self.config.serial.port);
                            alive.store(true, Ordering::Relaxed);
                            rx_thread = Some(spawn_rx(reader, chunk_tx.clone(), alive.clone()));
                            port = Some(writer);
                        }
                        Err(error) => log::debug!("{error:#}"),
                    }
                }
            }
            self.engine.set_serial_ok(alive.load(Ordering::Relaxed) && port.is_some());

            while let Ok((at, bytes)) = chunk_rx.try_recv() {
                self.engine.on_rx(&bytes, at);
            }
            // Never wait for the input thread: reuse the last state if it holds the lock
            if let Ok(shared) = self.input.try_lock() {
                local_input.clone_from(&shared);
            }

            let output = self.engine.tick(now, &local_input);
            if let Some(writer) = port.as_mut() {
                if let Err(error) = writer.write_all(&output.bytes) {
                    log::error!("serial write: {error}");
                    alive.store(false, Ordering::Relaxed);
                } else if let Some(event) = output.input_event {
                    self.engine.record_latency(event, SystemTime::now());
                }
            }
            if let Some(previous) = last_tick.replace(now) {
                self.engine.record_period(now.duration_since(previous));
            }

            deadline = self.engine.next_deadline(deadline);
            let after = Instant::now();
            if deadline <= after {
                // Overran: resynchronize instead of bursting frames to catch up
                deadline = after + self.engine.period();
            }
        }
        alive.store(false, Ordering::Relaxed);
        if let Some(thread) = rx_thread {
            let _ = thread.join();
        }
    }
}
