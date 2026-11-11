use std::cell::{Cell, RefCell};
use std::rc::Weak;

use crate::gpio_pin::GpioPin;
use crate::peripherals::peripheral::{BasePeripheral, Peripheral};
use crate::rp2040::BoardShared;

#[derive(Clone)]
pub struct Pads {
    base: BasePeripheral,
    pins: Vec<GpioPin>,
    first: u32,
    last: u32,
    voltage_select: Cell<u32>,
}

impl Pads {
    pub fn new(
        name: impl Into<String>,
        pins: Vec<GpioPin>,
        qspi: bool,
        board: Weak<RefCell<BoardShared>>,
    ) -> Self {
        Self {
            base: BasePeripheral::with_board(name, board),
            pins,
            first: 0x4,
            last: if qspi { 0x18 } else { 0x78 },
            voltage_select: Cell::new(0),
        }
    }
}

impl Peripheral for Pads {
    fn name(&self) -> &str {
        self.base.name()
    }

    fn read_u32(&self, offset: u32) -> u32 {
        if (self.first..=self.last).contains(&offset) {
            let index = ((offset - self.first) >> 2) as usize;
            return self.pins.get(index).map(GpioPin::pad_value).unwrap_or(0);
        }
        match offset {
            0 => self.voltage_select.get(),
            _ => self.base.read_u32(offset),
        }
    }

    fn write_u32_atomic(&self, offset: u32, value: u32, atomic_type: u32) {
        if (self.first..=self.last).contains(&offset) {
            let index = ((offset - self.first) >> 2) as usize;
            if let Some(pin) = self.pins.get(index) {
                pin.set_pad_value(value);
            }
            return;
        }
        match offset {
            0 => self.voltage_select.set(value & 1),
            _ => self.base.write_u32_atomic(offset, value, atomic_type),
        }
    }
}
