use std::cell::{Cell, RefCell};
use std::rc::Weak;

use crate::peripherals::peripheral::{atomic_update, BasePeripheral, Peripheral};
use crate::rp2040::BoardShared;

const PSM_BITS_MASK: u32 = 0x0001_ffff;

#[derive(Clone)]
pub struct Psm {
    base: BasePeripheral,
    frce_on: Cell<u32>,
    frce_off: Cell<u32>,
    wdsel: Cell<u32>,
}

impl Psm {
    pub fn new(name: impl Into<String>, board: Weak<RefCell<BoardShared>>) -> Self {
        Self {
            base: BasePeripheral::with_board(name, board),
            frce_on: Cell::new(0),
            frce_off: Cell::new(0),
            wdsel: Cell::new(0),
        }
    }
}

impl Peripheral for Psm {
    fn name(&self) -> &str {
        self.base.name()
    }

    fn read_u32(&self, offset: u32) -> u32 {
        match offset {
            0x00 => self.frce_on.get(),
            0x04 => self.frce_off.get(),
            0x08 => self.wdsel.get(),
            0x0c => {
                (PSM_BITS_MASK & !self.frce_off.get()) | (self.frce_on.get() & self.frce_off.get())
            }
            _ => self.base.read_u32(offset),
        }
    }

    fn write_u32_atomic(&self, offset: u32, value: u32, atomic_type: u32) {
        match offset {
            0x00 => {
                let value = atomic_update(self.frce_on.get(), atomic_type, value);
                self.frce_on.set(value & PSM_BITS_MASK);
            }
            0x04 => {
                let value = atomic_update(self.frce_off.get(), atomic_type, value);
                self.frce_off.set(value & PSM_BITS_MASK);
            }
            0x08 => {
                let value = atomic_update(self.wdsel.get(), atomic_type, value);
                self.wdsel.set(value & PSM_BITS_MASK);
            }
            _ => self.base.write_u32_atomic(offset, value, atomic_type),
        }
    }
}
