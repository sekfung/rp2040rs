#![forbid(unsafe_code)]

//! Safe Rust port of the rp2040js Raspberry Pi Pico emulator.

pub mod clock;
pub mod cortex_m0_core;
pub mod gpio_pin;
pub mod irq;
pub mod peripherals;
pub mod rp2040;
pub mod sio;
pub mod usb;
pub mod utils;

#[cfg(test)]
pub mod test_support;

pub use clock::{Clock, MockClock, SimulationClock};
pub use cortex_m0_core::Core;
pub use gpio_pin::{GpioPin, GpioPinState};
pub use peripherals::peripheral::{BasePeripheral, Peripheral};
pub use rp2040::{Memory, Rp2040};
pub use sio::Sio;
pub use utils::{ConsoleLogger, Fifo, LogLevel, Logger};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
