//! Handheld control daemon (design §3.1). The only process that owns the gamepad and the
//! ELRS TX module.

pub mod config;
pub mod engine;
pub mod histogram;
pub mod input;
pub mod ipc;
pub mod keycodes;
pub mod mapping;
pub mod params;
pub mod protocol;
pub mod runtime;
pub mod safety;
pub mod telemetry;
