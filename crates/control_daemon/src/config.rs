//! Daemon configuration (`config/daemon.toml`). Every field has a default so a missing file
//! still gives a working daemon on the handheld.

use crate::safety::SafetyConfig;
use crate::telemetry::TelemetryConfig;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub serial: SerialConfig,
    pub input: InputConfig,
    pub ipc: IpcConfig,
    pub safety: SafetyToml,
    pub telemetry: TelemetryConfig,
    pub tx: TxConfig,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct SerialConfig {
    /// Symlink created by the udev rule (systemd/99-elrs-tx.rules)
    pub port: String,
    /// Standard Linux rate, set identically on the TX module (design §11)
    pub baud: u32,
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self { port: "/dev/elrs_tx".into(), baud: 921_600 }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct InputConfig {
    /// Substrings of evdev device names (or /dev/input paths). All must be present.
    pub devices: Vec<String>,
    pub mapping: PathBuf,
    /// Written by the menu's calibration run; applied over the mapping file
    pub calibration: PathBuf,
    /// EVIOCGRAB the devices so no other process sees the events
    pub grab: bool,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            devices: vec!["adc-joystick".into(), "gpio-keys".into()],
            mapping: "/etc/rc/mapping.toml".into(),
            calibration: "/var/lib/rc/calibration.toml".into(),
            grab: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct IpcConfig {
    pub socket_path: PathBuf,
}

impl Default for IpcConfig {
    fn default() -> Self {
        Self { socket_path: "/run/rc/control.sock".into() }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct SafetyToml {
    pub arm_hold_ms: u64,
    pub override_max_delta_us: u16,
    pub override_max_ttl_ms: u64,
    pub override_disarmed_max_ttl_ms: u64,
}

impl Default for SafetyToml {
    fn default() -> Self {
        let d = SafetyConfig::default();
        Self {
            arm_hold_ms: d.arm_hold.as_millis() as u64,
            override_max_delta_us: d.override_max_delta_us,
            override_max_ttl_ms: d.override_max_ttl.as_millis() as u64,
            override_disarmed_max_ttl_ms: d.override_disarmed_max_ttl.as_millis() as u64,
        }
    }
}

impl From<&SafetyToml> for SafetyConfig {
    fn from(t: &SafetyToml) -> Self {
        Self {
            arm_hold: Duration::from_millis(t.arm_hold_ms),
            override_max_delta_us: t.override_max_delta_us,
            override_max_ttl: Duration::from_millis(t.override_max_ttl_ms),
            override_disarmed_max_ttl: Duration::from_millis(t.override_disarmed_max_ttl_ms),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct TxConfig {
    /// Frame period until the module's OPENTX_SYNC says otherwise (4000 µs = 250 Hz)
    pub default_interval_us: u32,
    pub min_interval_us: u32,
    pub max_interval_us: u32,
    /// SCHED_FIFO priority of the TX thread; 0 disables real-time scheduling
    pub rt_priority: i32,
    /// mlockall the process so the TX loop never page-faults
    pub mlock: bool,
}

impl Default for TxConfig {
    fn default() -> Self {
        Self {
            default_interval_us: 4000,
            min_interval_us: 1000,
            max_interval_us: 50_000,
            rt_priority: 50,
            mlock: true,
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        use anyhow::Context;
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("read {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parse {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_partial_files() {
        assert_eq!(toml::from_str::<Config>("").unwrap(), Config::default());
        let c: Config = toml::from_str("[serial]\nbaud = 460800\n[telemetry]\nlq_alarm = 40").unwrap();
        assert_eq!(c.serial.baud, 460_800);
        assert_eq!(c.serial.port, "/dev/elrs_tx");
        assert_eq!(c.telemetry.lq_alarm, 40);
        assert!(toml::from_str::<Config>("[serial]\nbaudrate = 1").is_err(), "typos are errors");
    }

    #[test]
    fn shipped_config_matches_defaults() {
        let c: Config = toml::from_str(include_str!("../config/daemon.toml")).unwrap();
        assert_eq!(c, Config::default());
    }
}
