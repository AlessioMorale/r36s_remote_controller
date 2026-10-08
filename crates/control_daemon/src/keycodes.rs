//! Linux input event codes by name (linux/input-event-codes.h), so the mapping file can say
//! `BTN_TR` instead of `311`. Kept here, not taken from the `evdev` crate, so the mapping
//! and safety logic build and test on any platform.

const KEYS: &[(&str, u16)] = &[
    ("BTN_0", 0x100), ("BTN_1", 0x101), ("BTN_2", 0x102), ("BTN_3", 0x103),
    ("BTN_4", 0x104), ("BTN_5", 0x105), ("BTN_6", 0x106), ("BTN_7", 0x107),
    ("BTN_8", 0x108), ("BTN_9", 0x109),
    ("BTN_TRIGGER", 0x120), ("BTN_THUMB", 0x121), ("BTN_THUMB2", 0x122), ("BTN_TOP", 0x123),
    ("BTN_TOP2", 0x124), ("BTN_PINKIE", 0x125), ("BTN_BASE", 0x126), ("BTN_BASE2", 0x127),
    ("BTN_SOUTH", 0x130), ("BTN_A", 0x130), ("BTN_EAST", 0x131), ("BTN_B", 0x131),
    ("BTN_C", 0x132), ("BTN_NORTH", 0x133), ("BTN_X", 0x133), ("BTN_WEST", 0x134),
    ("BTN_Y", 0x134), ("BTN_Z", 0x135), ("BTN_TL", 0x136), ("BTN_TR", 0x137),
    ("BTN_TL2", 0x138), ("BTN_TR2", 0x139), ("BTN_SELECT", 0x13a), ("BTN_START", 0x13b),
    ("BTN_MODE", 0x13c), ("BTN_THUMBL", 0x13d), ("BTN_THUMBR", 0x13e),
    ("BTN_DPAD_UP", 0x220), ("BTN_DPAD_DOWN", 0x221), ("BTN_DPAD_LEFT", 0x222),
    ("BTN_DPAD_RIGHT", 0x223),
    ("BTN_TRIGGER_HAPPY1", 0x2c0), ("BTN_TRIGGER_HAPPY2", 0x2c1),
    ("BTN_TRIGGER_HAPPY3", 0x2c2), ("BTN_TRIGGER_HAPPY4", 0x2c3),
    ("BTN_TRIGGER_HAPPY5", 0x2c4), ("BTN_TRIGGER_HAPPY6", 0x2c5),
    ("KEY_VOLUMEDOWN", 114), ("KEY_VOLUMEUP", 115), ("KEY_POWER", 116),
];

const AXES: &[(&str, u16)] = &[
    ("ABS_X", 0x00), ("ABS_Y", 0x01), ("ABS_Z", 0x02), ("ABS_RX", 0x03), ("ABS_RY", 0x04),
    ("ABS_RZ", 0x05), ("ABS_THROTTLE", 0x06), ("ABS_RUDDER", 0x07), ("ABS_WHEEL", 0x08),
    ("ABS_GAS", 0x09), ("ABS_BRAKE", 0x0a), ("ABS_HAT0X", 0x10), ("ABS_HAT0Y", 0x11),
    ("ABS_HAT1X", 0x12), ("ABS_HAT1Y", 0x13),
];

/// Highest key code tracked (KEY_MAX)
pub const KEY_MAX: u16 = 0x2ff;
/// Highest absolute axis code tracked (ABS_MAX)
pub const ABS_MAX: u16 = 0x3f;

fn lookup(table: &[(&str, u16)], name: &str) -> Option<u16> {
    table
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, code)| *code)
        .or_else(|| parse_number(name))
}

fn parse_number(text: &str) -> Option<u16> {
    match text.strip_prefix("0x") {
        Some(hex) => u16::from_str_radix(hex, 16).ok(),
        None => text.parse().ok(),
    }
}

/// Key code for a name such as `BTN_TR`, or a decimal/`0x` number
pub fn key(name: &str) -> Option<u16> {
    lookup(KEYS, name).filter(|code| *code <= KEY_MAX)
}

/// Absolute axis code for a name such as `ABS_RX`, or a decimal/`0x` number
pub fn axis(name: &str) -> Option<u16> {
    lookup(AXES, name).filter(|code| *code <= ABS_MAX)
}

/// Name for a key code, for logs
pub fn key_name(code: u16) -> String {
    KEYS.iter()
        .find(|(_, c)| *c == code)
        .map_or_else(|| format!("KEY_{code}"), |(n, _)| (*n).to_owned())
}

/// Name for an axis code, for logs
pub fn axis_name(code: u16) -> String {
    AXES.iter()
        .find(|(_, c)| *c == code)
        .map_or_else(|| format!("ABS_{code}"), |(n, _)| (*n).to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_numbers() {
        assert_eq!(key("BTN_TR"), Some(0x137));
        assert_eq!(key("btn_select"), Some(0x13a));
        assert_eq!(key("0x13b"), Some(0x13b));
        assert_eq!(key("311"), Some(311));
        assert_eq!(key("0x400"), None);
        assert_eq!(axis("ABS_RX"), Some(3));
        assert_eq!(axis("ABS_NOPE"), None);
        assert_eq!(axis_name(0x11), "ABS_HAT0Y");
    }
}
