//! Telemetry received over ELRS, with timestamps, staleness and alarms (design §3.1, §3.2).

use elrs_crsf::{Battery, Frame, LinkStats};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct TelemetryConfig {
    /// LINK_STATISTICS older than this: link lost
    pub link_stale_ms: u64,
    pub battery_stale_ms: u64,
    pub status_stale_ms: u64,
    /// Uplink LQ below this raises "ELRS degraded"
    pub lq_alarm: u8,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self { link_stale_ms: 800, battery_stale_ms: 3000, status_stale_ms: 3000, lq_alarm: 50 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AlarmLevel {
    Quiet,
    Visible,
    Loud,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Alarm {
    pub id: &'static str,
    pub level: AlarmLevel,
    pub message: String,
}

/// Conditions outside the telemetry that also raise alarms
#[derive(Debug, Clone, Copy, Default)]
pub struct AlarmInputs {
    pub serial_ok: bool,
    pub device_present: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LinkView {
    pub lq: u8,
    pub rssi_dbm: i32,
    pub snr_db: i8,
    pub tx_power_mw: u32,
    pub rf_mode: u8,
    pub downlink_lq: u8,
    pub downlink_rssi_dbm: i32,
    pub age_ms: u64,
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BatteryView {
    pub voltage: f32,
    pub current: f32,
    pub used_mah: i32,
    pub percent: u8,
    pub age_ms: u64,
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StatusView {
    pub text: String,
    pub age_ms: u64,
    pub stale: bool,
}

/// CRSF RF power index to mW (crsf_joy_node uses the same table)
pub fn tx_power_mw(index: u8) -> u32 {
    match index {
        1 => 10,
        2 => 25,
        3 => 100,
        4 => 500,
        5 => 1000,
        6 => 2000,
        7 => 250,
        8 => 50,
        _ => 0,
    }
}

#[derive(Debug, Clone, Default)]
pub struct Telemetry {
    pub config: TelemetryConfig,
    link: Option<(LinkStats, Instant)>,
    battery: Option<(Battery, Instant)>,
    status: Option<(String, Instant)>,
}

impl Telemetry {
    pub fn new(config: TelemetryConfig) -> Self {
        Self { config, ..Default::default() }
    }

    /// Records a telemetry frame; returns false for frames that are not telemetry
    pub fn on_frame(&mut self, frame: &Frame, now: Instant) -> bool {
        match frame {
            Frame::LinkStatistics(stats) => self.link = Some((*stats, now)),
            Frame::Battery(battery) => self.battery = Some((*battery, now)),
            Frame::FlightMode(text) => self.status = Some((text.clone(), now)),
            _ => return false,
        }
        true
    }

    fn age(now: Instant, at: Instant) -> Duration {
        now.saturating_duration_since(at)
    }

    fn is_stale(now: Instant, at: Instant, ms: u64) -> bool {
        Self::age(now, at) > Duration::from_millis(ms)
    }

    pub fn link(&self, now: Instant) -> Option<LinkView> {
        self.link.map(|(s, at)| LinkView {
            lq: s.uplink_link_quality,
            // Values are -dBm; the stronger antenna has the smaller value, 0 = no antenna 2
            rssi_dbm: -i32::from(match s.uplink_rssi_ant2 {
                0 => s.uplink_rssi_ant1,
                ant2 => s.uplink_rssi_ant1.min(ant2),
            }),
            snr_db: s.uplink_snr,
            tx_power_mw: tx_power_mw(s.uplink_tx_power),
            rf_mode: s.rf_mode,
            downlink_lq: s.downlink_link_quality,
            downlink_rssi_dbm: -i32::from(s.downlink_rssi),
            age_ms: Self::age(now, at).as_millis() as u64,
            stale: Self::is_stale(now, at, self.config.link_stale_ms),
        })
    }

    pub fn battery(&self, now: Instant) -> Option<BatteryView> {
        self.battery.map(|(b, at)| BatteryView {
            voltage: (b.voltage * 10.0).round() / 10.0,
            current: (b.current * 10.0).round() / 10.0,
            used_mah: b.used_mah,
            percent: b.percent,
            age_ms: Self::age(now, at).as_millis() as u64,
            stale: Self::is_stale(now, at, self.config.battery_stale_ms),
        })
    }

    pub fn status(&self, now: Instant) -> Option<StatusView> {
        self.status.as_ref().map(|(text, at)| (text, *at)).map(|(text, at)| StatusView {
            text: text.clone(),
            age_ms: Self::age(now, at).as_millis() as u64,
            stale: Self::is_stale(now, at, self.config.status_stale_ms),
        })
    }

    /// Active alarms by id
    pub fn alarms(&self, now: Instant, inputs: AlarmInputs) -> BTreeMap<&'static str, Alarm> {
        let mut alarms = BTreeMap::new();
        let mut raise = |id, level, message: String| {
            alarms.insert(id, Alarm { id, level, message });
        };

        if !inputs.serial_ok {
            raise("serial_error", AlarmLevel::Loud, "TX module UART not available".into());
        }
        if !inputs.device_present {
            raise("input_lost", AlarmLevel::Loud, "Gamepad lost: disarmed".into());
        }

        let link = self.link(now);
        let link_up = match &link {
            None => {
                raise("elrs_lost", AlarmLevel::Loud, "ELRS link lost: no link statistics".into());
                false
            }
            Some(l) if l.stale || l.lq == 0 => {
                raise(
                    "elrs_lost",
                    AlarmLevel::Loud,
                    format!("ELRS link lost: last update {:.1} s ago", l.age_ms as f64 / 1000.0),
                );
                false
            }
            Some(l) => {
                if l.lq < self.config.lq_alarm {
                    raise("elrs_degraded", AlarmLevel::Loud, format!("ELRS degraded: LQ {}%", l.lq));
                }
                true
            }
        };

        // Robot-side telemetry only matters while the link is up; otherwise elrs_lost says it
        if link_up {
            if self.battery(now).is_none_or(|b| b.stale) {
                raise("battery_stale", AlarmLevel::Visible, "Robot battery telemetry stale".into());
            }
            if self.status(now).is_none_or(|s| s.stale) {
                raise("status_stale", AlarmLevel::Visible, "Robot status stale".into());
            }
        }
        alarms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(lq: u8) -> Frame {
        Frame::LinkStatistics(LinkStats {
            uplink_rssi_ant1: 64,
            uplink_link_quality: lq,
            uplink_snr: 9,
            uplink_tx_power: 3,
            downlink_rssi: 66,
            downlink_link_quality: 97,
            ..Default::default()
        })
    }

    fn ok_inputs() -> AlarmInputs {
        AlarmInputs { serial_ok: true, device_present: true }
    }

    #[test]
    fn values_and_staleness() {
        let mut t = Telemetry::new(TelemetryConfig::default());
        let t0 = Instant::now();
        t.on_frame(&link(98), t0);
        t.on_frame(&Frame::Battery(Battery { voltage: 15.6, current: 1.2, used_mah: 450, percent: 72 }), t0);
        t.on_frame(&Frame::FlightMode("RDY".into()), t0);

        let l = t.link(t0 + Duration::from_millis(500)).unwrap();
        assert_eq!((l.lq, l.rssi_dbm, l.tx_power_mw, l.stale), (98, -64, 100, false));
        assert!(t.link(t0 + Duration::from_millis(1001)).unwrap().stale);
        assert!(!t.battery(t0 + Duration::from_millis(3000)).unwrap().stale);
        assert!(t.battery(t0 + Duration::from_millis(3001)).unwrap().stale);
        assert_eq!(t.status(t0).unwrap().text, "RDY");
        assert!(t.status(t0 + Duration::from_millis(3001)).unwrap().stale);
    }

    #[test]
    fn alarms() {
        let mut t = Telemetry::new(TelemetryConfig::default());
        let t0 = Instant::now();
        let a = t.alarms(t0, ok_inputs());
        assert_eq!(a.keys().copied().collect::<Vec<_>>(), vec!["elrs_lost"]);

        t.on_frame(&link(98), t0);
        let a = t.alarms(t0, ok_inputs());
        assert_eq!(a.keys().copied().collect::<Vec<_>>(), vec!["battery_stale", "status_stale"]);

        t.on_frame(&Frame::Battery(Battery::default()), t0);
        t.on_frame(&Frame::FlightMode("RDY".into()), t0);
        assert!(t.alarms(t0, ok_inputs()).is_empty());

        t.on_frame(&link(30), t0);
        let a = t.alarms(t0, ok_inputs());
        assert_eq!(a["elrs_degraded"].level, AlarmLevel::Loud);

        t.on_frame(&link(0), t0);
        assert!(t.alarms(t0, ok_inputs()).contains_key("elrs_lost"));

        t.on_frame(&link(99), t0);
        let a = t.alarms(t0 + Duration::from_millis(1500), AlarmInputs::default());
        assert!(a.contains_key("elrs_lost") && a.contains_key("serial_error") && a.contains_key("input_lost"));
        assert!(!a.contains_key("battery_stale"), "suppressed while the link is lost");
    }
}
