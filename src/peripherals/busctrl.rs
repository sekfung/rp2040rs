use std::cell::{Cell, RefCell};
use std::rc::Weak;

use crate::peripherals::peripheral::{atomic_update, BasePeripheral, Peripheral};
use crate::rp2040::BoardShared;

#[derive(Clone)]
pub struct BusCtrl {
    base: BasePeripheral,
    perf_ctr: [Cell<u32>; 4],
    perf_sel: [Cell<u32>; 4],
}

impl BusCtrl {
    pub fn new(name: impl Into<String>, board: Weak<RefCell<BoardShared>>) -> Self {
        Self {
            base: BasePeripheral::with_board(name, board),
            perf_ctr: std::array::from_fn(|_| Cell::new(0)),
            perf_sel: std::array::from_fn(|_| Cell::new(0x1f)),
        }
    }
}

impl Peripheral for BusCtrl {
    fn name(&self) -> &str {
        self.base.name()
    }

    fn read_u32(&self, offset: u32) -> u32 {
        match offset {
            0x004 => 1,
            0x008 | 0x010 | 0x018 | 0x020 => self.perf_ctr[((offset - 0x008) >> 3) as usize].get(),
            0x00c | 0x014 | 0x01c | 0x024 => self.perf_sel[((offset - 0x00c) >> 3) as usize].get(),
            _ => self.base.read_u32(offset),
        }
    }

    fn write_u32_atomic(&self, offset: u32, value: u32, atomic_type: u32) {
        match offset {
            0x008 | 0x010 | 0x018 | 0x020 => {
                self.perf_ctr[((offset - 0x008) >> 3) as usize].set(0);
            }
            0x00c | 0x014 | 0x01c | 0x024 => {
                let index = ((offset - 0x00c) >> 3) as usize;
                let value = atomic_update(self.perf_sel[index].get(), atomic_type, value);
                self.perf_sel[index].set(value & 0x1f);
            }
            _ => self.base.write_u32_atomic(offset, value, atomic_type),
        }
    }
}
