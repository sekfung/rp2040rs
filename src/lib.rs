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
pub use peripherals::i2c::{I2cMode, I2cSpeed};
pub use peripherals::pio::StateMachine;
pub use peripherals::{BasePeripheral, I2c, Peripheral, Pio, UsbController};
pub use rp2040::{ClockListener, ClockListenerId, Memory, Rp2040};
pub use simulator::Simulator;
pub use sio::Sio;
pub use usb::{
    create_setup_packet, get_descriptor_packet, is_standard_request, parse_setup_packet,
    set_device_address_packet, set_device_configuration_packet, DataDirection, DescriptorType,
    EndpointNumbers, ISetupPacketParams, ParsedSetupPacket, SetupPacketParams, SetupRecipient,
    SetupRequest, SetupType, StandardRequest, StandardRequestCode, UsbCdc, UsbDescriptorType,
    UsbDevice, UsbTransferResult, UsbTransferStatus,
};
pub use utils::{ConsoleLogger, Fifo, LogLevel, Logger};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
