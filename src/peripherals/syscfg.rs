use std::cell::RefCell;
use std::rc::Weak;

use crate::cortex_m0_core::Core;
use crate::peripherals::peripheral::{BasePeripheral, Peripheral};
use crate::rp2040::BoardShared;

#[derive(Clone)]
pub struct SysCfg {
    base: BasePeripheral,
    core: Core,
}

impl SysCfg {
    pub fn new(name: impl Into<String>, core: Core, board: Weak<RefCell<BoardShared>>) -> Self {
        Self {
            base: BasePeripheral::with_board(name, board),
            core,
        }
    }
}

impl Peripheral for SysCfg {
    fn name(&self) -> &str {
        self.base.name()
    }

    fn read_u32(&self, offset: u32) -> u32 {
        match offset {
            0x00 => self.core.interrupt_nmi_mask(),
            _ => self.base.read_u32(offset),
        }
    }

    fn write_u32_atomic(&self, offset: u32, value: u32, atomic_type: u32) {
        match offset {
            0x00 => self.core.set_interrupt_nmi_mask(value),
            _ => self.base.write_u32_atomic(offset, value, atomic_type),
        }
    }
}
