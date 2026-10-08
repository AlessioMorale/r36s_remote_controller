//! Fake ELRS TX module on a pty.
//!
//! `fake_tx --link /tmp/elrs_tx` creates the pty and a symlink the daemon can open as its
//! serial port. Received RC frames and other traffic are logged.

use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use test_tools::{fake_tx::FakeTx, pty::Pty};

#[derive(Parser)]
struct Args {
    /// Symlink to create for the daemon's serial port
    #[arg(long, default_value = "/tmp/elrs_tx")]
    link: PathBuf,
    /// RC frame interval announced in OPENTX_SYNC, in microseconds (4000 = 250 Hz)
    #[arg(long, default_value_t = 4000)]
    interval_us: u32,
    /// Replay telemetry frames from a file (one hex frame per line) instead of generating them
    #[arg(long)]
    replay: Option<PathBuf>,
    /// Stop answering after this many seconds (simulates power loss); 0 = never
    #[arg(long, default_value_t = 0)]
    die_after_s: u64,
    /// Log every received frame
    #[arg(long)]
    verbose: bool,
}

fn parse_hex_line(line: &str) -> Option<Vec<u8>> {
    let line = line.split('#').next()?.trim();
    if line.is_empty() {
        return None;
    }
    line.split_whitespace().map(|t| u8::from_str_radix(t, 16).ok()).collect()
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();

    let pty = Pty::open()?;
    let _ = std::fs::remove_file(&args.link);
    std::os::unix::fs::symlink(&pty.slave_path, &args.link)
        .with_context(|| format!("symlink {}", args.link.display()))?;
    log::info!("fake TX module on {} -> {}", args.link.display(), pty.slave_path.display());

    let start = Instant::now();
    let mut tx = FakeTx::new(start);
    tx.sync_interval_100ns = args.interval_us * 10;
    if let Some(path) = &args.replay {
        let text = std::fs::read_to_string(path)?;
        tx.replay = text.lines().filter_map(parse_hex_line).collect();
        log::info!("replaying {} frames", tx.replay.len());
    }

    let mut reported = 0u64;
    loop {
        let now = Instant::now();
        if args.die_after_s != 0 && !tx.muted && tx.uptime(now) > Duration::from_secs(args.die_after_s) {
            log::warn!("module power lost");
            tx.muted = true;
        }
        let out = tx.poll(&pty.read_available(), now);
        if !out.is_empty() {
            pty.write(&out);
        }
        if args.verbose && tx.frames_received != reported {
            reported = tx.frames_received;
            if let Some(rc) = tx.rc_log.last() {
                log::info!("{} frames, last RC {:?}", reported, &rc.channels[..8]);
            }
        } else if tx.frames_received >= reported + 250 {
            reported = tx.frames_received;
            log::info!("{} frames received, {} RC frames", reported, tx.rc_log.len());
        }
        if tx.rc_log.len() > 100_000 {
            tx.rc_log.drain(..50_000);
        }
        std::thread::sleep(Duration::from_micros(500));
    }
}
