//! Reads CRSF frames from the TX module's UART and prints the ones that pass the CRC (plan
//! T1.3: "reads valid CRSF frames from the TX module, including LINK_STATISTICS or
//! DEVICE_INFO after a ping"). Run it with the daemon stopped: the port is exclusive.
//!
//! ```text
//! crsf_capture --port /dev/elrs_tx --baud 921600 --ping --seconds 10
//! ```

use anyhow::{Context, Result};
use clap::Parser;
use elrs_crsf::{decode, encode, Frame};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::time::{Duration, Instant};

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "/dev/elrs_tx")]
    port: String,
    /// Baud rate; 0 for a pty on macOS
    #[arg(long, default_value_t = 921_600)]
    baud: u32,
    #[arg(long, default_value_t = 10)]
    seconds: u64,
    /// Send a DEVICE_PING at the start and once a second
    #[arg(long)]
    ping: bool,
    /// Print only a summary at the end
    #[arg(long)]
    quiet: bool,
    /// Exit with an error unless LINK_STATISTICS or DEVICE_INFO was seen
    #[arg(long)]
    require_telemetry: bool,
}

fn describe(frame: &Frame) -> String {
    match frame {
        Frame::LinkStatistics(s) => format!(
            "LINK_STATISTICS lq={} rssi=-{} snr={} power_idx={}",
            s.uplink_link_quality, s.uplink_rssi_ant1, s.uplink_snr, s.uplink_tx_power
        ),
        Frame::Battery(b) => format!("BATTERY {:.1} V {:.1} A {}%", b.voltage, b.current, b.percent),
        Frame::FlightMode(m) => format!("FLIGHT_MODE '{m}'"),
        Frame::OpenTxSync(s) => format!("OPENTX_SYNC interval={} µs offset={}", s.interval / 10, s.offset / 10),
        Frame::DeviceInfo(d) => format!(
            "DEVICE_INFO '{}' serial={:#x} fw={:#x} params={}",
            d.name, d.serial_number, d.firmware_id, d.parameters_total
        ),
        Frame::ParameterEntry(c) => format!("PARAMETER_ENTRY #{} chunks_remaining={}", c.number, c.chunks_remaining),
        other => format!("{other:?}"),
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let mut port = serialport::new(&args.port, args.baud)
        .timeout(Duration::from_millis(50))
        .open()
        .with_context(|| format!("open {} at {} baud", args.port, args.baud))?;
    let mut parser = elrs_crsf::Parser::new();
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    let mut buffer = [0u8; 256];
    let start = Instant::now();
    let mut last_ping: Option<Instant> = None;

    while start.elapsed() < Duration::from_secs(args.seconds) {
        if args.ping && last_ping.is_none_or(|at| at.elapsed() >= Duration::from_secs(1)) {
            last_ping = Some(Instant::now());
            port.write_all(&encode::device_ping())?;
        }
        let n = match port.read(&mut buffer) {
            Ok(n) => n,
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => 0,
            Err(error) => return Err(error.into()),
        };
        for raw in parser.feed(&buffer[..n]) {
            let frame = decode(&raw);
            let text = describe(&frame);
            let kind = text.split_whitespace().next().unwrap_or("?").to_string();
            *counts.entry(kind).or_default() += 1;
            if !args.quiet {
                println!("{:8.3}  {text}", start.elapsed().as_secs_f64());
            }
        }
    }

    let stats = parser.stats();
    println!(
        "\n{} bytes, {} valid frames, {} CRC errors, {} sync errors (bytes outside frames), {} length errors",
        stats.total_bytes, stats.frames_decoded, stats.crc_errors, stats.sync_errors, stats.length_errors
    );
    for (kind, count) in &counts {
        println!("  {kind:<18} {count}");
    }
    if args.require_telemetry && !counts.contains_key("LINK_STATISTICS") && !counts.contains_key("DEVICE_INFO") {
        anyhow::bail!("no LINK_STATISTICS or DEVICE_INFO received");
    }
    Ok(())
}
