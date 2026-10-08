//! Test doubles for the handheld: a fake ELRS TX module and a virtual gamepad.

pub mod fake_tx;
pub mod pty;
pub mod script;
#[cfg(target_os = "linux")]
pub mod virtual_pad;
