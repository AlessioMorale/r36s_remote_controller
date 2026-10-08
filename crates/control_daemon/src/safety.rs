//! Arm, deadman, menu and override rules (design §6). Pure: no I/O, time is passed in.
//!
//! * Startup: no RC frame is sent until the sticks have been seen neutral; always disarmed.
//! * Arming: hold arm (L1) + deadman (R1) for `arm_hold_ms` with the sticks neutral.
//! * Disarming: pressing arm + deadman together again (rising edge), or losing the gamepad.
//! * AUX1 is high only while armed, with the deadman held, and the menu closed. After
//!   arming, opening or closing the menu, the deadman must be released and pressed again.
//! * Menu open: sticks forced neutral, AUX1 low, overrides cleared.
//! * Overrides (from the UI): axis channels only, bounded and time-limited while armed,
//!   cleared on disarm.

use serde::Serialize;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq)]
pub struct SafetyConfig {
    pub arm_hold: Duration,
    pub override_max_delta_us: u16,
    pub override_max_ttl: Duration,
    /// TTL limit while disarmed (overrides must still expire)
    pub override_disarmed_max_ttl: Duration,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            arm_hold: Duration::from_millis(1000),
            override_max_delta_us: 100,
            override_max_ttl: Duration::from_millis(1000),
            override_disarmed_max_ttl: Duration::from_secs(60),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArmState {
    Disarmed,
    Arming,
    Armed,
}

/// What the TX loop sampled from the gamepad this tick
#[derive(Debug, Clone, Copy, Default)]
pub struct SafetyInputs {
    pub device_present: bool,
    pub arm_held: bool,
    pub deadman_held: bool,
    pub sticks_neutral: bool,
    /// Rising edges of the turbo button since the last tick
    pub turbo_presses: u32,
    /// Rising edges of the menu button since the last tick
    pub menu_presses: u32,
}

/// Decisions for this tick's RC frame
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SafetyOutput {
    /// False until the sticks have been neutral once after startup
    pub send_frames: bool,
    /// Axis channels held at 1500 µs
    pub force_neutral: bool,
    pub aux1: bool,
    pub turbo: bool,
    pub arm_state: ArmState,
    pub menu_open: bool,
    pub deadman: bool,
    /// Active overrides for channels 0-3, already filtered by the rules
    pub overrides: [Option<u16>; 4],
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Override {
    value_us: u16,
    expires: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverrideError {
    Channel,
    Range,
    MenuOpen,
    /// Armed: too far from center or TTL too long
    Bounds,
}

impl std::fmt::Display for OverrideError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Channel => "overrides are limited to channels 0-3",
            Self::Range => "value_us must be 1000-2000 and ttl_ms > 0",
            Self::MenuOpen => "overrides are not allowed while the menu is open",
            Self::Bounds => "overrides need disarmed or bounded value and ttl",
        })
    }
}

#[derive(Debug, Clone)]
pub struct Safety {
    config: SafetyConfig,
    state: ArmState,
    arming_since: Option<Instant>,
    startup_confirmed: bool,
    menu_open: bool,
    deadman_ready: bool,
    turbo: bool,
    prev_combo: bool,
    overrides: [Option<Override>; 4],
}

impl Safety {
    pub fn new(config: SafetyConfig) -> Self {
        Self {
            config,
            state: ArmState::Disarmed,
            arming_since: None,
            startup_confirmed: false,
            menu_open: false,
            deadman_ready: false,
            turbo: false,
            // A combo held at startup must be released before it can arm
            prev_combo: true,
            overrides: [None; 4],
        }
    }

    pub fn arm_state(&self) -> ArmState {
        self.state
    }

    pub fn menu_open(&self) -> bool {
        self.menu_open
    }

    fn disarm(&mut self) {
        self.state = ArmState::Disarmed;
        self.arming_since = None;
        self.turbo = false;
        self.overrides = [None; 4];
    }

    fn set_menu(&mut self, open: bool) {
        if open != self.menu_open {
            self.menu_open = open;
            self.deadman_ready = false;
            self.overrides = [None; 4];
            if open && self.state == ArmState::Arming {
                self.disarm();
            }
        }
    }

    /// Closes the menu (UI request; the same as pressing the menu button)
    pub fn close_menu(&mut self) {
        self.set_menu(false);
    }

    pub fn update(&mut self, now: Instant, input: SafetyInputs) -> SafetyOutput {
        if !input.device_present {
            self.disarm();
            self.deadman_ready = false;
            self.prev_combo = true;
        } else {
            if !self.startup_confirmed && input.sticks_neutral {
                self.startup_confirmed = true;
            }
            if input.menu_presses % 2 == 1 {
                self.set_menu(!self.menu_open);
            }
            if !self.menu_open && input.turbo_presses % 2 == 1 && self.state == ArmState::Armed {
                self.turbo = !self.turbo;
            }
            if !input.deadman_held {
                self.deadman_ready = true;
            }
            self.update_arming(now, &input);
        }

        for slot in &mut self.overrides {
            if slot.is_some_and(|o| o.expires <= now) {
                *slot = None;
            }
        }

        let armed = self.state == ArmState::Armed;
        SafetyOutput {
            send_frames: self.startup_confirmed,
            force_neutral: self.menu_open || !input.device_present,
            aux1: armed
                && input.device_present
                && !self.menu_open
                && input.deadman_held
                && self.deadman_ready,
            turbo: self.turbo,
            arm_state: self.state,
            menu_open: self.menu_open,
            deadman: input.deadman_held,
            overrides: self.overrides.map(|o| o.map(|o| o.value_us)),
        }
    }

    fn update_arming(&mut self, now: Instant, input: &SafetyInputs) {
        let combo = input.arm_held && input.deadman_held;
        let rising = combo && !self.prev_combo;
        self.prev_combo = combo;

        match self.state {
            ArmState::Armed => {
                if rising {
                    self.disarm();
                }
            }
            ArmState::Disarmed => {
                if rising && !self.menu_open && input.sticks_neutral && self.startup_confirmed {
                    self.state = ArmState::Arming;
                    self.arming_since = Some(now);
                }
            }
            ArmState::Arming => {
                if !combo || !input.sticks_neutral || self.menu_open {
                    self.disarm();
                } else if self
                    .arming_since
                    .is_some_and(|since| now.duration_since(since) >= self.config.arm_hold)
                {
                    self.state = ArmState::Armed;
                    self.arming_since = None;
                    // R1 is still held from the gesture: it must be released first
                    self.deadman_ready = false;
                    // Overrides set while disarmed must also satisfy the armed bounds
                    let (max_delta, max_ttl) =
                        (self.config.override_max_delta_us, self.config.override_max_ttl);
                    for slot in &mut self.overrides {
                        if slot.is_some_and(|o| {
                            o.value_us.abs_diff(1500) > max_delta
                                || o.expires.saturating_duration_since(now) > max_ttl
                        }) {
                            *slot = None;
                        }
                    }
                }
            }
        }
    }

    pub fn set_override(
        &mut self,
        now: Instant,
        channel: usize,
        value_us: u16,
        ttl: Duration,
    ) -> Result<(), OverrideError> {
        if channel >= 4 {
            return Err(OverrideError::Channel);
        }
        if !(1000..=2000).contains(&value_us) || ttl.is_zero() {
            return Err(OverrideError::Range);
        }
        if self.menu_open {
            return Err(OverrideError::MenuOpen);
        }
        let disarmed = self.state == ArmState::Disarmed;
        let allowed = if disarmed {
            ttl <= self.config.override_disarmed_max_ttl
        } else {
            value_us.abs_diff(1500) <= self.config.override_max_delta_us
                && ttl <= self.config.override_max_ttl
        };
        if !allowed {
            return Err(OverrideError::Bounds);
        }
        self.overrides[channel] = Some(Override { value_us, expires: now + ttl });
        Ok(())
    }

    pub fn clear_override(&mut self, channel: Option<usize>) {
        match channel {
            Some(channel) if channel < 4 => self.overrides[channel] = None,
            Some(_) => {}
            None => self.overrides = [None; 4],
        }
    }

    /// Remaining TTL of each active override, for the telemetry snapshot
    pub fn override_status(&self, now: Instant) -> Vec<(usize, u16, Duration)> {
        self.overrides
            .iter()
            .enumerate()
            .filter_map(|(channel, o)| {
                o.map(|o| (channel, o.value_us, o.expires.saturating_duration_since(now)))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Harness {
        safety: Safety,
        now: Instant,
        input: SafetyInputs,
    }

    impl Harness {
        fn new() -> Self {
            Self {
                safety: Safety::new(SafetyConfig::default()),
                now: Instant::now(),
                input: SafetyInputs { device_present: true, sticks_neutral: true, ..Default::default() },
            }
        }

        fn step(&mut self, ms: u64) -> SafetyOutput {
            self.now += Duration::from_millis(ms);
            let out = self.safety.update(self.now, self.input);
            self.input.menu_presses = 0;
            self.input.turbo_presses = 0;
            out
        }

        fn hold_combo(&mut self, ms: u64) -> SafetyOutput {
            self.input.arm_held = true;
            self.input.deadman_held = true;
            let mut out = self.step(0);
            let mut elapsed = 0;
            while elapsed < ms {
                out = self.step(10);
                elapsed += 10;
            }
            out
        }

        fn release(&mut self) -> SafetyOutput {
            self.input.arm_held = false;
            self.input.deadman_held = false;
            self.step(10)
        }

        fn arm(&mut self) {
            self.step(10);
            assert_eq!(self.hold_combo(1000).arm_state, ArmState::Armed);
            self.release();
        }
    }

    #[test]
    fn starts_disarmed_and_silent_until_neutral() {
        let mut h = Harness::new();
        h.input.sticks_neutral = false;
        let out = h.step(10);
        assert!(!out.send_frames);
        assert_eq!(out.arm_state, ArmState::Disarmed);
        assert!(!out.aux1);
        h.input.sticks_neutral = true;
        assert!(h.step(10).send_frames);
        // Once confirmed, frames keep flowing even if the sticks move
        h.input.sticks_neutral = false;
        assert!(h.step(10).send_frames);
    }

    #[test]
    fn refuses_to_arm_with_sticks_off_centre() {
        let mut h = Harness::new();
        h.step(10);
        h.input.sticks_neutral = false;
        assert_eq!(h.hold_combo(1500).arm_state, ArmState::Disarmed);
        // Moving a stick during the hold aborts arming
        h.release();
        h.input.sticks_neutral = true;
        assert_eq!(h.hold_combo(500).arm_state, ArmState::Arming);
        h.input.sticks_neutral = false;
        assert_eq!(h.step(10).arm_state, ArmState::Disarmed);
        h.input.sticks_neutral = true;
        assert_eq!(h.hold_combo(1500).arm_state, ArmState::Disarmed, "needs a new press");
    }

    #[test]
    fn arming_needs_the_full_hold() {
        let mut h = Harness::new();
        h.step(10);
        assert_eq!(h.hold_combo(900).arm_state, ArmState::Arming);
        assert_eq!(h.release().arm_state, ArmState::Disarmed);
        assert_eq!(h.hold_combo(1000).arm_state, ArmState::Armed);
    }

    #[test]
    fn combo_held_at_startup_does_not_arm() {
        let mut h = Harness::new();
        assert_eq!(h.hold_combo(2000).arm_state, ArmState::Disarmed);
    }

    #[test]
    fn aux1_only_while_armed_and_deadman_held() {
        let mut h = Harness::new();
        h.step(10);
        // Disarmed: deadman alone does nothing
        h.input.deadman_held = true;
        assert!(!h.step(10).aux1);
        h.input.deadman_held = false;
        h.step(10);

        // Arming gesture: R1 is held when arming completes, but AUX1 stays low
        let out = h.hold_combo(1000);
        assert_eq!(out.arm_state, ArmState::Armed);
        assert!(!out.aux1);
        h.input.arm_held = false;
        assert!(!h.step(10).aux1, "R1 still held from the gesture");
        h.input.deadman_held = false;
        assert!(!h.step(10).aux1);
        h.input.deadman_held = true;
        assert!(h.step(10).aux1);
        h.input.deadman_held = false;
        assert!(!h.step(10).aux1);
        h.input.deadman_held = true;
        assert!(h.step(10).aux1);
    }

    #[test]
    fn short_combo_press_disarms() {
        let mut h = Harness::new();
        h.arm();
        h.input.deadman_held = true;
        assert!(h.step(10).aux1);
        h.input.arm_held = true; // L1 while driving with R1
        let out = h.step(10);
        assert_eq!(out.arm_state, ArmState::Disarmed);
        assert!(!out.aux1);
        // Keeping the combo held does not re-arm
        assert_eq!(h.hold_combo(2000).arm_state, ArmState::Disarmed);
    }

    #[test]
    fn menu_forces_neutral_and_aux1_low() {
        let mut h = Harness::new();
        h.arm();
        h.input.deadman_held = true;
        assert!(h.step(10).aux1);
        h.input.menu_presses = 1;
        let out = h.step(10);
        assert!(out.menu_open && out.force_neutral && !out.aux1);
        assert_eq!(out.arm_state, ArmState::Armed);
        // Arming is impossible from the menu
        h.release();
        assert_eq!(h.hold_combo(1500).arm_state, ArmState::Disarmed, "combo disarms, never arms");
        h.release();
        h.input.menu_presses = 1;
        assert!(!h.step(10).menu_open);
        // Closing the menu: AUX1 needs a fresh deadman press
        h.arm();
        h.input.menu_presses = 1;
        h.step(10);
        h.input.deadman_held = true;
        h.step(10);
        h.input.menu_presses = 1;
        let out = h.step(10);
        assert!(!out.menu_open && !out.force_neutral);
        assert!(!out.aux1, "deadman held across the menu");
        h.input.deadman_held = false;
        h.step(10);
        h.input.deadman_held = true;
        assert!(h.step(10).aux1);
    }

    #[test]
    fn ui_close_menu_needs_fresh_deadman() {
        let mut h = Harness::new();
        h.arm();
        h.input.menu_presses = 1;
        h.input.deadman_held = true;
        h.step(10);
        h.safety.close_menu();
        let out = h.step(10);
        assert!(!out.menu_open);
        assert!(!out.aux1);
    }

    #[test]
    fn losing_the_input_device_disarms() {
        let mut h = Harness::new();
        h.arm();
        h.input.deadman_held = true;
        assert!(h.step(10).aux1);
        h.input.device_present = false;
        let out = h.step(10);
        assert_eq!(out.arm_state, ArmState::Disarmed);
        assert!(!out.aux1 && out.force_neutral);
        // Back again: still disarmed, and a combo held through the reconnect does not arm
        h.input.device_present = true;
        assert_eq!(h.hold_combo(2000).arm_state, ArmState::Disarmed);
    }

    #[test]
    fn turbo_toggles_only_while_armed_and_resets_on_disarm() {
        let mut h = Harness::new();
        h.step(10);
        h.input.turbo_presses = 1;
        assert!(!h.step(10).turbo);
        h.arm();
        h.input.turbo_presses = 1;
        assert!(h.step(10).turbo);
        h.input.turbo_presses = 2; // two taps within one tick
        assert!(h.step(10).turbo);
        h.input.arm_held = true;
        h.input.deadman_held = true;
        assert!(!h.step(10).turbo);
    }

    #[test]
    fn override_expires_when_not_refreshed() {
        let mut h = Harness::new();
        h.step(10);
        let now = h.now;
        h.safety.set_override(now, 2, 1700, Duration::from_millis(100)).unwrap();
        assert_eq!(h.step(50).overrides[2], Some(1700));
        h.safety.set_override(h.now, 2, 1700, Duration::from_millis(100)).unwrap();
        assert_eq!(h.step(80).overrides[2], Some(1700));
        assert_eq!(h.step(30).overrides[2], None);
    }

    #[test]
    fn override_bounds_while_armed_and_cleared_on_disarm() {
        let mut h = Harness::new();
        h.step(10);
        // Disarmed: large value allowed, but it is dropped when arming (out of armed bounds)
        h.safety.set_override(h.now, 0, 2000, Duration::from_secs(10)).unwrap();
        assert_eq!(h.hold_combo(990).arm_state, ArmState::Arming);
        // Within the armed bounds: kept when arming completes
        h.safety.set_override(h.now, 1, 1550, Duration::from_millis(900)).unwrap();
        assert_eq!(h.step(20).arm_state, ArmState::Armed);
        h.release();
        let out = h.step(10);
        assert_eq!(out.overrides[0], None);
        assert_eq!(out.overrides[1], Some(1550));

        let now = h.now;
        assert_eq!(
            h.safety.set_override(now, 0, 1700, Duration::from_millis(500)),
            Err(OverrideError::Bounds)
        );
        assert_eq!(
            h.safety.set_override(now, 0, 1550, Duration::from_secs(5)),
            Err(OverrideError::Bounds)
        );
        h.safety.set_override(now, 0, 1450, Duration::from_millis(500)).unwrap();
        assert_eq!(h.step(10).overrides[0], Some(1450));

        h.input.arm_held = true;
        h.input.deadman_held = true;
        let out = h.step(10);
        assert_eq!(out.arm_state, ArmState::Disarmed);
        assert_eq!(out.overrides, [None; 4]);
    }

    #[test]
    fn override_never_reaches_aux_channels() {
        let mut h = Harness::new();
        h.step(10);
        for channel in 4..16 {
            assert_eq!(
                h.safety.set_override(h.now, channel, 2000, Duration::from_millis(100)),
                Err(OverrideError::Channel)
            );
        }
        assert_eq!(
            h.safety.set_override(h.now, 0, 2100, Duration::from_millis(100)),
            Err(OverrideError::Range)
        );
    }

    #[test]
    fn menu_clears_and_rejects_overrides() {
        let mut h = Harness::new();
        h.step(10);
        h.safety.set_override(h.now, 0, 1600, Duration::from_secs(1)).unwrap();
        h.input.menu_presses = 1;
        assert_eq!(h.step(10).overrides, [None; 4]);
        assert_eq!(
            h.safety.set_override(h.now, 0, 1600, Duration::from_secs(1)),
            Err(OverrideError::MenuOpen)
        );
    }
}
