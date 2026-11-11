pub mod clocks;
pub mod dma;
pub mod peripheral;
pub mod pll;
pub mod ppb;
pub mod pwm;
pub mod timer;
pub mod uart;
pub mod xosc;

pub use clocks::Clocks;
pub use dma::{Dma, DreqChannel};
pub use peripheral::{atomic_update, BasePeripheral, Peripheral};
pub use pll::Pll;
pub use ppb::Ppb;
pub use pwm::Pwm;
pub use timer::Timer;
pub use uart::Uart;
pub use xosc::Xosc;
