//! A `uinput` gamepad with the R36S button and axis layout (Linux only).

use anyhow::{Context, Result};
use evdev::uinput::VirtualDevice;
use evdev::{AbsInfo, AbsoluteAxisCode, AttributeSet, EventType, InputEvent, KeyCode, UinputAbsSetup};

use crate::script::Step;

/// Keys of the R36S gamepad (as the joypad driver reports them)
const KEYS: &[u16] = &[
    0x130, 0x131, 0x133, 0x134, // A(south) B(east) X(north) Y(west)
    0x136, 0x137, 0x138, 0x139, // L1 R1 L2 R2
    0x13a, 0x13b, 0x13c, 0x13d, 0x13e, // select start mode thumbl thumbr
    0x220, 0x221, 0x222, 0x223, // D-pad as buttons
];
/// Sticks: ABS_X/Y (left), ABS_RX/RY (right)
const STICK_AXES: &[u16] = &[0x00, 0x01, 0x03, 0x04];
pub const STICK_RANGE: i32 = 1800;

pub struct VirtualPad {
    device: VirtualDevice,
}

impl VirtualPad {
    pub fn create(name: &str) -> Result<Self> {
        let mut keys = AttributeSet::<KeyCode>::new();
        for code in KEYS {
            keys.insert(KeyCode::new(*code));
        }
        let mut builder = VirtualDevice::builder()
            .context("open /dev/uinput (need root or the uinput group)")?
            .name(name)
            .with_keys(&keys)?;
        for code in STICK_AXES {
            let info = AbsInfo::new(0, -STICK_RANGE, STICK_RANGE, 16, 64, 0);
            builder = builder.with_absolute_axis(&UinputAbsSetup::new(AbsoluteAxisCode(*code), info))?;
        }
        Ok(Self { device: builder.build()? })
    }

    pub fn key(&mut self, code: u16, value: i32) -> Result<()> {
        self.device.emit(&[InputEvent::new(EventType::KEY.0, code, value)])?;
        Ok(())
    }

    pub fn abs(&mut self, code: u16, value: i32) -> Result<()> {
        self.device.emit(&[InputEvent::new(EventType::ABSOLUTE.0, code, value)])?;
        Ok(())
    }

    /// Plays a parsed script (blocking)
    pub fn play(&mut self, steps: &[Step]) -> Result<()> {
        for step in steps {
            match *step {
                Step::Wait(ms) => std::thread::sleep(std::time::Duration::from_millis(ms)),
                Step::Key { code, value } => self.key(code, value)?,
                Step::Abs { code, value } => self.abs(code, value)?,
            }
        }
        Ok(())
    }

    pub fn device_node(&mut self) -> Option<String> {
        self.device
            .enumerate_dev_nodes_blocking()
            .ok()?
            .filter_map(Result::ok)
            .next()
            .map(|path| path.display().to_string())
    }
}
