use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::rp2040::BoardShared;

pub const ATOMIC_NORMAL: u32 = 0;
pub const ATOMIC_XOR: u32 = 1;
pub const ATOMIC_SET: u32 = 2;
pub const ATOMIC_CLEAR: u32 = 3;

pub fn atomic_update(current_value: u32, atomic_type: u32, new_value: u32) -> u32 {
    match atomic_type {
        ATOMIC_XOR => current_value ^ new_value,
        ATOMIC_SET => current_value | new_value,
        ATOMIC_CLEAR => current_value & !new_value,
        _ => new_value,
    }
}

pub trait Peripheral {
    fn name(&self) -> &str;
    fn read_u32(&self, offset: u32) -> u32;
    fn write_u32_atomic(&self, offset: u32, value: u32, atomic_type: u32);
    fn reset(&self) {}
}

pub struct BasePeripheral {
    name: String,
    board: Weak<RefCell<BoardShared>>,
    raw_write_value: Cell<u32>,
}

impl BasePeripheral {
    pub fn new(name: impl Into<String>) -> Self {
        Self::with_board(name, Weak::new())
    }

    pub fn with_board(name: impl Into<String>, board: Weak<RefCell<BoardShared>>) -> Self {
        Self {
            name: name.into(),
            board,
            raw_write_value: Cell::new(0),
        }
    }

    fn warn(&self, message: &str) {
        if let Some(board) = self.board.upgrade() {
            let logger = board.borrow().logger.clone();
            logger.borrow().warn(&self.name, message);
        }
    }

    pub fn raw_write_value(&self) -> u32 {
        self.raw_write_value.get()
    }
}

impl Peripheral for BasePeripheral {
    fn name(&self) -> &str {
        &self.name
    }

    fn read_u32(&self, offset: u32) -> u32 {
        self.warn(&format!("Unimplemented peripheral read from 0x{offset:x}"));
        if offset > 0x1000 {
            self.warn("Unimplemented read from peripheral in the atomic operation region");
        }
        0xffff_ffff
    }

    fn write_u32_atomic(&self, offset: u32, value: u32, atomic_type: u32) {
        self.raw_write_value.set(value);
        let new_value = if atomic_type != ATOMIC_NORMAL {
            atomic_update(self.read_u32(offset), atomic_type, value)
        } else {
            value
        };
        self.warn(&format!(
            "Unimplemented peripheral write to 0x{offset:x}: 0x{new_value:x}"
        ));
    }
}

pub type SharedPeripheral = Rc<dyn Peripheral>;
