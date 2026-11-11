use std::cell::{Cell, RefCell};
use std::rc::Weak;

use crate::peripherals::peripheral::{BasePeripheral, Peripheral};
use crate::rp2040::BoardShared;

#[derive(Clone)]
pub struct Resets {
    base: BasePeripheral,
    reset: Cell<u32>,
    wdsel: Cell<u32>,
    reset_done: Cell<u32>,
}

impl Resets {
    pub fn new(name: impl Into<String>, board: Weak<RefCell<BoardShared>>) -> Self {
        Self {
            base: BasePeripheral::with_board(name, board),
            reset: Cell::new(0),
            wdsel: Cell::new(0),
            reset_done: Cell::new(0x1ff_ffff),
        }
    }
}

impl Peripheral for Resets {
    fn name(&self) -> &str {
        self.base.name()
    }

    fn read_u32(&self, offset: u32) -> u32 {
        match offset {
            0x0 => self.reset.get(),
            0x4 => self.wdsel.get(),
            0x8 => self.reset_done.get(),
            _ => self.base.read_u32(offset),
        }
    }

    fn write_u32_atomic(&self, offset: u32, value: u32, atomic_type: u32) {
        match offset {
            0x0 => self.reset.set(value & 0x1ff_ffff),
            0x4 => self.wdsel.set(value & 0x1ff_ffff),
            _ => self.base.write_u32_atomic(offset, value, atomic_type),
        }
    }
}
