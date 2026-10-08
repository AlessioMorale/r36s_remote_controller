//! Scripts for the virtual gamepad: one step per line.
//!
//! ```text
//! # comment
//! wait 500            # milliseconds
//! key BTN_TL 1        # press (1) or release (0); names or numbers
//! abs ABS_RX 1800     # absolute axis value
//! ```

use anyhow::{bail, Context, Result};

// The same name tables as the daemon's mapping file, without a dependency cycle
#[path = "../../control_daemon/src/keycodes.rs"]
#[allow(dead_code)]
mod keycodes;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Wait(u64),
    Key { code: u16, value: i32 },
    Abs { code: u16, value: i32 },
}

pub fn parse(text: &str) -> Result<Vec<Step>> {
    let mut steps = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        let at = || format!("line {}: '{line}'", index + 1);
        let step = match fields.as_slice() {
            ["wait", ms] => Step::Wait(ms.parse().with_context(at)?),
            ["key", name, value] => Step::Key {
                code: keycodes::key(name).with_context(|| format!("{}: unknown key", at()))?,
                value: value.parse().with_context(at)?,
            },
            ["abs", name, value] => Step::Abs {
                code: keycodes::axis(name).with_context(|| format!("{}: unknown axis", at()))?,
                value: value.parse().with_context(at)?,
            },
            _ => bail!("{}: expected 'wait MS', 'key NAME 0|1' or 'abs NAME VALUE'", at()),
        };
        steps.push(step);
    }
    Ok(steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_steps() {
        let steps = parse("# arm\nkey BTN_TL 1\n\nabs ABS_RX -1800 # full left\nwait 250\n").unwrap();
        assert_eq!(
            steps,
            vec![
                Step::Key { code: 0x136, value: 1 },
                Step::Abs { code: 3, value: -1800 },
                Step::Wait(250)
            ]
        );
        assert!(parse("key NOPE 1").is_err());
        assert!(parse("jump 3").is_err());
    }
}
