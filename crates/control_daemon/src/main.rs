//! `control_daemon`: gamepad -> CRSF -> ELRS TX module, with a UI socket (design §3.1).

use anyhow::{Context, Result};
use clap::Parser;
use control_daemon::config::Config;
use control_daemon::engine::Engine;
use control_daemon::mapping::{Calibration, InputState, Mapping, MappingConfig};
use control_daemon::runtime::TxLoop;
use control_daemon::{input, ipc};
use crossbeam_channel::bounded;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// Daemon configuration; built-in defaults are used if the file does not exist
    #[arg(long, default_value = "/etc/rc/daemon.toml")]
    config: PathBuf,
    /// Override the serial port (e.g. a fake_tx pty)
    #[arg(long)]
    serial: Option<String>,
    /// Override the baud rate. Use 0 for a pty on macOS (serialport skips the speed ioctl)
    #[arg(long)]
    baud: Option<u32>,
    /// Override the IPC socket path
    #[arg(long)]
    socket: Option<PathBuf>,
    /// Override the mapping file
    #[arg(long)]
    mapping: Option<PathBuf>,
}

static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: libc::c_int) {
    SHUTDOWN_REQUESTED.store(true, Ordering::Relaxed);
}

fn load_mapping(config: &Config, override_path: Option<PathBuf>) -> Result<Mapping> {
    let path = override_path.unwrap_or_else(|| config.input.mapping.clone());
    let text = std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let mut mapping = Mapping::new(
        toml::from_str::<MappingConfig>(&text).with_context(|| format!("parse {}", path.display()))?,
    )?;
    match std::fs::read_to_string(&config.input.calibration) {
        Ok(text) => {
            let calibration: Calibration = toml::from_str(&text)
                .with_context(|| format!("parse {}", config.input.calibration.display()))?;
            mapping.apply_calibration(&calibration);
            log::info!("calibration loaded from {}", config.input.calibration.display());
        }
        Err(_) => log::info!("no calibration file: using the mapping file's stick ranges"),
    }
    Ok(mapping)
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();

    let mut config = if args.config.exists() {
        Config::load(&args.config)?
    } else {
        log::warn!("{} not found: using defaults", args.config.display());
        Config::default()
    };
    if let Some(port) = args.serial {
        config.serial.port = port;
    }
    if let Some(baud) = args.baud {
        config.serial.baud = baud;
    }
    if let Some(socket) = args.socket {
        config.ipc.socket_path = socket;
    }
    let mapping = load_mapping(&config, args.mapping)?;

    // SAFETY: the handler only stores to an atomic
    unsafe {
        libc::signal(libc::SIGINT, on_signal as *const () as usize);
        libc::signal(libc::SIGTERM, on_signal as *const () as usize);
    }
    let shutdown = Arc::new(AtomicBool::new(false));

    let shared_input = Arc::new(Mutex::new(InputState::default()));
    let input_thread = input::spawn(config.input.clone(), shared_input.clone(), shutdown.clone());

    let (out_tx, out_rx) = bounded(256);
    let (request_tx, request_rx) = bounded(64);
    let server = ipc::start(&config.ipc.socket_path, out_rx, request_tx, shutdown.clone())
        .context("start IPC server")?;

    let engine = Engine::new(config.clone(), mapping, out_tx, request_rx);
    let tx_thread = TxLoop { engine, config, input: shared_input, shutdown: shutdown.clone() }.spawn();
    log::info!("control_daemon started, disarmed");

    while !SHUTDOWN_REQUESTED.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(100));
    }
    log::info!("shutting down");
    shutdown.store(true, Ordering::Relaxed);
    let _ = tx_thread.join();
    let _ = input_thread.join();
    for thread in server.threads {
        let _ = thread.join();
    }
    Ok(())
}
