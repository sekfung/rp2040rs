use std::cell::RefCell;
use std::rc::Weak;

use crate::peripherals::peripheral::{BasePeripheral, Peripheral};
use crate::rp2040::BoardShared;

#[derive(Clone)]
pub struct SysInfo {
    base: BasePeripheral,
}

impl SysInfo {
    pub fn new(name: impl Into<String>, board: Weak<RefCell<BoardShared>>) -> Self {
        Self {
            base: BasePeripheral::with_board(name, board),
        }
    }
}

impl Peripheral for SysInfo {
    fn name(&self) -> &str {
        self.base.name()
    }

    fn read_u32(&self, offset: u32) -> u32 {
        match offset {
            0x00 => 0x1000_2927,
            0x04 => 0x0000_0002,
            0x40 => 0xe0c9_12e8,
            _ => self.base.read_u32(offset),
        }
    }

    fn write_u32_atomic(&self, offset: u32, value: u32, atomic_type: u32) {
        self.base.write_u32_atomic(offset, value, atomic_type);
    }
}
