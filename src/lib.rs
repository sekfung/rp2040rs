#![forbid(unsafe_code)]

//! Safe Rust port of the rp2040js Raspberry Pi Pico emulator.

pub mod clock;
pub mod utils;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
