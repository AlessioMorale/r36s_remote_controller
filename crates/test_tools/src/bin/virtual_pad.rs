//! Virtual R36S gamepad. `virtual_pad script.txt` plays a script (see test_tools::script)
//! and exits; with no file it reads steps from stdin, so tests can drive it live.

#[cfg(target_os = "linux")]
fn main() -> anyhow::Result<()> {
    use clap::Parser;
    use std::io::BufRead;
    use test_tools::{script, virtual_pad::VirtualPad};

    #[derive(Parser)]
    struct Args {
        script: Option<std::path::PathBuf>,
        #[arg(long, default_value = "R36S virtual gamepad")]
        name: String,
        /// Keep the device alive this many seconds after the script ends
        #[arg(long, default_value_t = 0)]
        linger_s: u64,
    }
    let args = Args::parse();
    let mut pad = VirtualPad::create(&args.name)?;
    eprintln!("virtual pad: {}", pad.device_node().unwrap_or_default());
    match args.script {
        Some(path) => pad.play(&script::parse(&std::fs::read_to_string(path)?)?)?,
        None => {
            for line in std::io::stdin().lock().lines() {
                pad.play(&script::parse(&line?)?)?;
            }
        }
    }
    std::thread::sleep(std::time::Duration::from_secs(args.linger_s));
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("virtual_pad needs Linux (uinput)");
    std::process::exit(2);
}
