#![forbid(unsafe_code)]

//! Safe Rust port of the rp2040js Raspberry Pi Pico emulator.

pub mod clock;
pub mod cortex_m0_core;
pub mod irq;
pub mod peripherals;
pub mod rp2040;
pub mod utils;

#[cfg(test)]
pub mod test_support;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
