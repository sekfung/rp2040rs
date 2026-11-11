#![forbid(unsafe_code)]

//! Safe Rust port of the rp2040js Raspberry Pi Pico emulator.

pub mod bootrom;
pub mod clock;
pub mod cortex_m0_core;
pub mod gdb;
pub mod gpio_pin;
pub mod interpolator;
pub mod irq;
pub mod loaders;
pub mod peripherals;
pub mod rp2040;
pub mod simulator;
pub mod sio;
pub mod usb;
pub mod utils;

#[cfg(test)]
pub mod test_support;

pub use bootrom::BOOTROM_B1;
pub use clock::{Clock, MockClock, SimulationClock};
pub use cortex_m0_core::Core;
pub use gdb::{GdbConnection, GdbServer, GdbTarget, GdbTcpServer};
pub use gpio_pin::{GpioPin, GpioPinState};
pub use loaders::{load_hex, load_uf2, LoadError};
pub use peripherals::peripheral::{BasePeripheral, Peripheral};
pub use rp2040::{Memory, Rp2040};
pub use simulator::Simulator;
pub use sio::Sio;
pub use utils::{ConsoleLogger, Fifo, LogLevel, Logger};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
