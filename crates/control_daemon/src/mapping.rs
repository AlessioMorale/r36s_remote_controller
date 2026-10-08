//! Gamepad state and its mapping to CRSF channels (design §5.1).
//!
//! Pure functions only: the evdev reader fills an [`InputState`], the TX loop maps it.

use crate::keycodes;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

pub const CENTER_US: u16 = 1500;

/// Latest state of every input device, merged
#[derive(Debug, Clone)]
pub struct InputState {
    pub axes: [i32; keycodes::ABS_MAX as usize + 1],
    keys: [u64; (keycodes::KEY_MAX as usize + 1).div_ceil(64)],
    /// Rising edges per key, so a tap shorter than one TX period is never lost
    press_counts: Vec<u32>,
    /// All configured devices are open
    pub device_present: bool,
    /// Incremented on every event
    pub seq: u64,
    /// Kernel timestamp of the latest event (input-to-UART latency trace)
    pub last_event: Option<SystemTime>,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            axes: [0; keycodes::ABS_MAX as usize + 1],
            keys: [0; (keycodes::KEY_MAX as usize + 1).div_ceil(64)],
            press_counts: vec![0; keycodes::KEY_MAX as usize + 1],
            device_present: false,
            seq: 0,
            last_event: None,
        }
    }
}

impl InputState {
    pub fn key(&self, code: u16) -> bool {
        let code = code as usize;
        code <= keycodes::KEY_MAX as usize && self.keys[code / 64] & (1 << (code % 64)) != 0
    }

    pub fn presses(&self, code: u16) -> u32 {
        self.press_counts.get(code as usize).copied().unwrap_or(0)
    }

    pub fn set_key(&mut self, code: u16, pressed: bool) {
        let index = code as usize;
        if index > keycodes::KEY_MAX as usize {
            return;
        }
        if pressed && !self.key(code) {
            self.press_counts[index] = self.press_counts[index].wrapping_add(1);
        }
        let bit = 1u64 << (index % 64);
        if pressed {
            self.keys[index / 64] |= bit;
        } else {
            self.keys[index / 64] &= !bit;
        }
    }

    pub fn set_axis(&mut self, code: u16, value: i32) {
        if let Some(axis) = self.axes.get_mut(code as usize) {
            *axis = value;
        }
    }

    /// Releases every key (device lost); press counts are kept
    pub fn release_all(&mut self) {
        self.keys.iter_mut().for_each(|word| *word = 0);
    }
}

/// One stick axis feeding one CRSF channel
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AxisMap {
    pub channel: usize,
    /// evdev axis name, e.g. `ABS_RX`
    pub source: String,
    pub min: i32,
    pub center: i32,
    pub max: i32,
    /// Fraction of travel around the center that maps to exactly 1500 µs
    #[serde(default = "default_deadzone")]
    pub deadzone: f32,
    /// Flip direction (evdev Y axes usually grow downwards; CRSF up is high)
    #[serde(default)]
    pub invert: bool,
}

fn default_deadzone() -> f32 {
    0.05
}

/// A button: a key name (`BTN_TR`), or one direction of an axis (`ABS_HAT0Y-`, `ABS_HAT0Y+`)
/// for D-pads that report as a hat
#[derive(Debug, Clone, PartialEq)]
pub enum ButtonSource {
    Key(u16),
    AxisNegative(u16),
    AxisPositive(u16),
}

impl ButtonSource {
    pub fn parse(spec: &str) -> Option<Self> {
        if let Some(axis) = spec.strip_suffix('-') {
            return keycodes::axis(axis).map(Self::AxisNegative);
        }
        if let Some(axis) = spec.strip_suffix('+') {
            return keycodes::axis(axis).map(Self::AxisPositive);
        }
        keycodes::key(spec).map(Self::Key)
    }

    pub fn pressed(&self, state: &InputState) -> bool {
        match *self {
            Self::Key(code) => state.key(code),
            Self::AxisNegative(code) => state.axes[code as usize] < 0,
            Self::AxisPositive(code) => state.axes[code as usize] > 0,
        }
    }

    /// Key presses so far; hat directions are sampled by level instead
    pub fn presses(&self, state: &InputState) -> Option<u32> {
        match *self {
            Self::Key(code) => Some(state.presses(code)),
            _ => None,
        }
    }
}

/// Button roles, as names in the mapping file
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ButtonsConfig {
    /// L1: held with the deadman for the arm/disarm gesture
    pub arm: String,
    /// R1: must be held for AUX1 (teleop enable) to go high
    pub deadman: String,
    /// R2: toggles turbo (AUX2)
    pub turbo: String,
    /// Select: opens and closes the menu
    pub menu: String,
    pub up: String,
    pub down: String,
    pub left: String,
    pub right: String,
    pub a: String,
    pub b: String,
    pub x: String,
    pub y: String,
    pub start: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MappingConfig {
    pub axes: Vec<AxisMap>,
    pub buttons: ButtonsConfig,
    #[serde(default = "default_aux_low")]
    pub aux_low_us: u16,
    #[serde(default = "default_aux_high")]
    pub aux_high_us: u16,
    /// Normalized stick deflection still counted as neutral for arming and startup
    #[serde(default = "default_neutral_tolerance")]
    pub neutral_tolerance: f32,
}

fn default_aux_low() -> u16 {
    1000
}
fn default_aux_high() -> u16 {
    2000
}
fn default_neutral_tolerance() -> f32 {
    0.1
}

/// Menu buttons forwarded to the UI while the menu is open (docs/ipc.md `menu_input`)
pub const MENU_BUTTONS: [&str; 9] = ["up", "down", "left", "right", "a", "b", "x", "y", "start"];

/// A mapping with every name resolved to a code
#[derive(Debug, Clone)]
pub struct Mapping {
    pub config: MappingConfig,
    axes: Vec<(AxisMap, u16)>,
    pub arm: ButtonSource,
    pub deadman: ButtonSource,
    pub turbo: ButtonSource,
    pub menu: ButtonSource,
    /// In [`MENU_BUTTONS`] order
    pub menu_buttons: Vec<(&'static str, ButtonSource)>,
}

impl Mapping {
    pub fn new(config: MappingConfig) -> anyhow::Result<Self> {
        let button = |spec: &str| {
            ButtonSource::parse(spec).ok_or_else(|| anyhow::anyhow!("unknown button '{spec}'"))
        };
        let mut axes = Vec::new();
        for axis in &config.axes {
            let code = keycodes::axis(&axis.source)
                .ok_or_else(|| anyhow::anyhow!("unknown axis '{}'", axis.source))?;
            anyhow::ensure!(axis.channel < 4, "axis channel {} is not 0-3", axis.channel);
            anyhow::ensure!(
                axis.min < axis.center && axis.center < axis.max,
                "axis {}: need min < center < max",
                axis.source
            );
            anyhow::ensure!((0.0..0.5).contains(&axis.deadzone), "axis {}: deadzone", axis.source);
            axes.push((axis.clone(), code));
        }
        let b = &config.buttons;
        let menu_buttons = MENU_BUTTONS
            .iter()
            .zip([&b.up, &b.down, &b.left, &b.right, &b.a, &b.b, &b.x, &b.y, &b.start])
            .map(|(name, spec)| Ok((*name, button(spec)?)))
            .collect::<anyhow::Result<Vec<_>>>()?;
        anyhow::ensure!(config.aux_low_us < config.aux_high_us, "aux_low_us >= aux_high_us");
        Ok(Self {
            arm: button(&b.arm)?,
            deadman: button(&b.deadman)?,
            turbo: button(&b.turbo)?,
            menu: button(&b.menu)?,
            menu_buttons,
            axes,
            config,
        })
    }

    /// Axis codes in use, for calibration
    pub fn axis_codes(&self) -> impl Iterator<Item = (&AxisMap, u16)> {
        self.axes.iter().map(|(map, code)| (map, *code))
    }

    /// Stick channels 0-3 in µs; unmapped channels stay at center
    pub fn axis_channels(&self, state: &InputState) -> [u16; 4] {
        let mut out = [CENTER_US; 4];
        for (map, code) in &self.axes {
            out[map.channel] = axis_us(state.axes[*code as usize], map);
        }
        out
    }

    /// True when every mapped stick is within `neutral_tolerance` of center
    pub fn sticks_neutral(&self, state: &InputState) -> bool {
        self.axes.iter().all(|(map, code)| {
            normalize(state.axes[*code as usize], map).abs() <= self.config.neutral_tolerance
        })
    }

    pub fn aux_us(&self, high: bool) -> u16 {
        if high {
            self.config.aux_high_us
        } else {
            self.config.aux_low_us
        }
    }

    /// Applies recorded stick ranges (calibration file)
    pub fn apply_calibration(&mut self, calibration: &Calibration) {
        for (map, _) in &mut self.axes {
            if let Some(range) = calibration.axes.iter().find(|r| r.source == map.source) {
                if range.min < range.center && range.center < range.max {
                    map.min = range.min;
                    map.center = range.center;
                    map.max = range.max;
                }
            }
        }
        self.config.axes = self.axes.iter().map(|(map, _)| map.clone()).collect();
    }
}

/// Raw value to [-1, 1] with the calibrated center at 0
pub fn normalize(raw: i32, map: &AxisMap) -> f32 {
    let value = if raw >= map.center {
        (raw - map.center) as f32 / (map.max - map.center) as f32
    } else {
        (raw - map.center) as f32 / (map.center - map.min) as f32
    };
    let value = value.clamp(-1.0, 1.0);
    if map.invert {
        -value
    } else {
        value
    }
}

/// Raw value to µs: deadzone around the center, rescaled so full travel is still 1000/2000
pub fn axis_us(raw: i32, map: &AxisMap) -> u16 {
    let n = normalize(raw, map);
    let magnitude = n.abs();
    let shaped = if magnitude <= map.deadzone {
        0.0
    } else {
        n.signum() * (magnitude - map.deadzone) / (1.0 - map.deadzone)
    };
    (f32::from(CENTER_US) + shaped * 500.0).round() as u16
}

/// Recorded stick ranges, stored in the calibration file
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Calibration {
    pub axes: Vec<AxisRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AxisRange {
    pub source: String,
    pub min: i32,
    pub center: i32,
    pub max: i32,
}

/// Records stick ranges during a calibration run: the center is the value seen when the
/// run starts (sticks released), min/max the extremes reached while moving them
#[derive(Debug, Clone, Default)]
pub struct CalibrationRecorder {
    ranges: Vec<(u16, AxisRange)>,
}

impl CalibrationRecorder {
    pub fn start(mapping: &Mapping, state: &InputState) -> Self {
        let ranges = mapping
            .axis_codes()
            .map(|(map, code)| {
                let value = state.axes[code as usize];
                (code, AxisRange { source: map.source.clone(), min: value, center: value, max: value })
            })
            .collect();
        Self { ranges }
    }

    pub fn sample(&mut self, state: &InputState) {
        for (code, range) in &mut self.ranges {
            let value = state.axes[*code as usize];
            range.min = range.min.min(value);
            range.max = range.max.max(value);
        }
    }

    pub fn finish(self) -> Calibration {
        Calibration { axes: self.ranges.into_iter().map(|(_, range)| range).collect() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn r36s_mapping() -> Mapping {
        let config: MappingConfig =
            toml::from_str(include_str!("../config/mapping.toml")).expect("mapping.toml");
        Mapping::new(config).unwrap()
    }

    fn axis(map: &Mapping, channel: usize) -> (AxisMap, u16) {
        map.axis_codes()
            .find(|(m, _)| m.channel == channel)
            .map(|(m, c)| (m.clone(), c))
            .unwrap()
    }

    #[test]
    fn neutral_maps_to_center() {
        let map = r36s_mapping();
        let mut state = InputState::default();
        for channel in 0..4 {
            let (m, code) = axis(&map, channel);
            state.set_axis(code, m.center);
        }
        assert_eq!(map.axis_channels(&state), [1500; 4]);
        assert!(map.sticks_neutral(&state));
    }

    #[test]
    fn full_deflection_each_axis() {
        let map = r36s_mapping();
        for channel in 0..4 {
            let (m, code) = axis(&map, channel);
            for (raw, expect_high) in [(m.max, !m.invert), (m.min, m.invert)] {
                let mut state = InputState::default();
                for other in 0..4 {
                    let (om, oc) = axis(&map, other);
                    state.set_axis(oc, om.center);
                }
                state.set_axis(code, raw);
                let channels = map.axis_channels(&state);
                let expected = if expect_high { 2000 } else { 1000 };
                assert_eq!(channels[channel], expected, "channel {channel} raw {raw}");
                for other in (0..4).filter(|c| *c != channel) {
                    assert_eq!(channels[other], 1500);
                }
                assert!(!map.sticks_neutral(&state));
            }
        }
    }

    #[test]
    fn stick_up_and_right_are_high() {
        // Design §5.1: ch0 = right stick X (right = high), ch1 = right stick Y (up = high)
        let map = r36s_mapping();
        let (x, x_code) = axis(&map, 0);
        let (y, y_code) = axis(&map, 1);
        assert_eq!(x.source, "ABS_RX");
        assert_eq!(y.source, "ABS_RY");
        let mut state = InputState::default();
        state.set_axis(x_code, x.max);
        state.set_axis(y_code, y.min); // evdev: up is the low end
        let channels = map.axis_channels(&state);
        assert_eq!(channels[0], 2000);
        assert_eq!(channels[1], 2000);
    }

    #[test]
    fn deadzone_and_rescale() {
        let m = AxisMap {
            channel: 0,
            source: "ABS_X".into(),
            min: -1000,
            center: 0,
            max: 1000,
            deadzone: 0.1,
            invert: false,
        };
        assert_eq!(axis_us(99, &m), 1500);
        assert_eq!(axis_us(-100, &m), 1500);
        assert_eq!(axis_us(550, &m), 1750);
        assert_eq!(axis_us(5000, &m), 2000);
        // Asymmetric ranges are normalized per side
        let a = AxisMap { min: -500, max: 2000, deadzone: 0.0, ..m };
        assert_eq!(axis_us(-250, &a), 1250);
        assert_eq!(axis_us(1000, &a), 1750);
    }

    #[test]
    fn hat_buttons_and_press_counts() {
        let mut state = InputState::default();
        let up = ButtonSource::parse("ABS_HAT0Y-").unwrap();
        state.set_axis(0x11, -1);
        assert!(up.pressed(&state));
        let r1 = ButtonSource::parse("BTN_TR").unwrap();
        state.set_key(0x137, true);
        state.set_key(0x137, true);
        state.set_key(0x137, false);
        state.set_key(0x137, true);
        assert_eq!(r1.presses(&state), Some(2));
        state.release_all();
        assert!(!r1.pressed(&state));
    }

    #[test]
    fn calibration_records_ranges() {
        let mut map = r36s_mapping();
        let (m, code) = axis(&map, 0);
        let mut state = InputState::default();
        state.set_axis(code, 10);
        let mut recorder = CalibrationRecorder::start(&map, &state);
        for value in [-900, 300, 1200, 10] {
            state.set_axis(code, value);
            recorder.sample(&state);
        }
        let calibration = recorder.finish();
        let range = calibration.axes.iter().find(|r| r.source == m.source).unwrap();
        assert_eq!((range.min, range.center, range.max), (-900, 10, 1200));
        map.apply_calibration(&calibration);
        state.set_axis(code, 1200);
        assert_eq!(map.axis_channels(&state)[0], if m.invert { 1000 } else { 2000 });
    }

    #[test]
    fn rejects_bad_config() {
        let mut config = r36s_mapping().config;
        config.axes[0].source = "ABS_NOPE".into();
        assert!(Mapping::new(config.clone()).is_err());
        let mut config = r36s_mapping().config;
        config.axes[0].channel = 4; // AUX1 can never come from a stick
        assert!(Mapping::new(config).is_err());
    }
}
