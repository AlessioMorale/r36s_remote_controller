//! `ctl`: command-line client for the daemon's UI socket, for testing (plan T3.7).
//!
//! ```text
//! ctl watch [--count N] [--only telemetry|alarm|params|menu_input]   print messages
//! ctl rate                                                            measure telemetry rate
//! ctl state                                                           one telemetry snapshot
//! ctl params                                                          print the parameter tree
//! ctl param set NUMBER VALUE                                          write a module parameter
//! ctl param refresh
//! ctl override set CHANNEL VALUE_US TTL_MS | override clear [CHANNEL]
//! ctl menu-close | calibrate start | calibrate finish [--save]
//! ctl raw '{"type":...,"id":1}'                                       send any request
//! ```

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use control_daemon::protocol::{read_message, write_message};
use serde_json::{json, Value};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "/run/rc/control.sock", global = true)]
    socket: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Watch {
        #[arg(long)]
        count: Option<usize>,
        #[arg(long)]
        only: Option<String>,
    },
    Rate {
        #[arg(long, default_value_t = 3)]
        seconds: u64,
    },
    State,
    Params,
    Param {
        #[command(subcommand)]
        action: ParamAction,
    },
    Override {
        #[command(subcommand)]
        action: OverrideAction,
    },
    MenuClose,
    Calibrate {
        #[command(subcommand)]
        action: CalibrateAction,
    },
    Raw {
        json: String,
    },
}

#[derive(Subcommand)]
enum ParamAction {
    Set { number: u8, value: i32 },
    Refresh,
}

#[derive(Subcommand)]
enum OverrideAction {
    Set { channel: usize, value_us: u16, ttl_ms: u64 },
    Clear { channel: Option<usize> },
}

#[derive(Subcommand)]
enum CalibrateAction {
    Start,
    Finish {
        #[arg(long)]
        save: bool,
    },
}

fn connect(path: &PathBuf) -> Result<UnixStream> {
    let stream = UnixStream::connect(path).with_context(|| format!("connect {}", path.display()))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    Ok(stream)
}

fn read_json(stream: &mut UnixStream) -> Result<Value> {
    Ok(serde_json::from_str(&read_message(stream)?)?)
}

/// Sends a request and waits for its ack, skipping broadcasts
fn request(stream: &mut UnixStream, body: Value) -> Result<Value> {
    let id = body["id"].as_i64().unwrap_or(1);
    write_message(stream, &body.to_string())?;
    loop {
        let message = read_json(stream)?;
        if message["type"] == "ack" && message["id"] == id {
            return Ok(message);
        }
    }
}

fn run_request(args: &Args, body: Value) -> Result<()> {
    let mut stream = connect(&args.socket)?;
    let ack = request(&mut stream, body)?;
    println!("{ack}");
    if ack["ok"] != true {
        std::process::exit(1);
    }
    Ok(())
}

fn wait_for(stream: &mut UnixStream, kind: &str, timeout: Duration) -> Result<Value> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        let message = read_json(stream)?;
        if message["type"] == kind {
            return Ok(message);
        }
    }
    bail!("no '{kind}' message within {timeout:?}")
}

fn main() -> Result<()> {
    let args = Args::parse();
    match &args.command {
        Command::Watch { count, only } => {
            let mut stream = connect(&args.socket)?;
            stream.set_read_timeout(None)?;
            let mut seen = 0;
            loop {
                let message = read_json(&mut stream)?;
                if only.as_deref().is_some_and(|kind| message["type"] != kind) {
                    continue;
                }
                println!("{message}");
                seen += 1;
                if count.is_some_and(|limit| seen >= limit) {
                    return Ok(());
                }
            }
        }
        Command::Rate { seconds } => {
            let mut stream = connect(&args.socket)?;
            let start = Instant::now();
            let mut count = 0u32;
            while start.elapsed() < Duration::from_secs(*seconds) {
                if read_json(&mut stream)?["type"] == "telemetry" {
                    count += 1;
                }
            }
            println!("{:.2} Hz ({count} snapshots in {:.1} s)", f64::from(count) / start.elapsed().as_secs_f64(), start.elapsed().as_secs_f64());
            Ok(())
        }
        Command::State => {
            let mut stream = connect(&args.socket)?;
            println!("{}", wait_for(&mut stream, "telemetry", Duration::from_secs(5))?);
            Ok(())
        }
        Command::Params => {
            let mut stream = connect(&args.socket)?;
            println!("{}", wait_for(&mut stream, "params", Duration::from_secs(10))?);
            Ok(())
        }
        Command::Param { action: ParamAction::Set { number, value } } => run_request(
            &args,
            json!({"type": "param_write", "id": 1, "number": number, "value": value}),
        ),
        Command::Param { action: ParamAction::Refresh } => {
            run_request(&args, json!({"type": "param_refresh", "id": 1}))
        }
        Command::Override { action: OverrideAction::Set { channel, value_us, ttl_ms } } => run_request(
            &args,
            json!({"type": "override_set", "id": 1, "channel": channel, "value_us": value_us, "ttl_ms": ttl_ms}),
        ),
        Command::Override { action: OverrideAction::Clear { channel } } => {
            run_request(&args, json!({"type": "override_clear", "id": 1, "channel": channel}))
        }
        Command::MenuClose => run_request(&args, json!({"type": "menu_close", "id": 1})),
        Command::Calibrate { action: CalibrateAction::Start } => {
            run_request(&args, json!({"type": "calibration_start", "id": 1}))
        }
        Command::Calibrate { action: CalibrateAction::Finish { save } } => {
            run_request(&args, json!({"type": "calibration_finish", "id": 1, "save": save}))
        }
        Command::Raw { json } => {
            let body: Value = serde_json::from_str(json).context("raw: invalid JSON")?;
            run_request(&args, body)
        }
    }
}
