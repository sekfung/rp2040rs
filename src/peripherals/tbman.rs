use std::cell::RefCell;
use std::rc::Weak;

use crate::peripherals::peripheral::{BasePeripheral, Peripheral};
use crate::rp2040::BoardShared;

#[derive(Clone)]
pub struct Tbman {
    base: BasePeripheral,
}

impl Tbman {
    pub fn new(name: impl Into<String>, board: Weak<RefCell<BoardShared>>) -> Self {
        Self {
            base: BasePeripheral::with_board(name, board),
        }
    }
}

impl Peripheral for Tbman {
    fn name(&self) -> &str {
        self.base.name()
    }

    fn read_u32(&self, offset: u32) -> u32 {
        match offset {
            0 => 1,
            _ => self.base.read_u32(offset),
        }
    }

    fn write_u32_atomic(&self, offset: u32, value: u32, atomic_type: u32) {
        self.base.write_u32_atomic(offset, value, atomic_type);
    }
}
