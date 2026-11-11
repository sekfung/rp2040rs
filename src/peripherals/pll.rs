use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::peripherals::peripheral::{atomic_update, Peripheral};
use crate::rp2040::BoardShared;

const PLL_CS: u32 = 0x00;
const PLL_PWR: u32 = 0x04;
const PLL_FBDIV_INT: u32 = 0x08;
const PLL_PRIM: u32 = 0x0c;

const PLL_CS_LOCK: u32 = 1 << 31;
const PLL_CS_BYPASS: u32 = 1 << 8;
const PLL_CS_REFDIV_MASK: u32 = 0x3f;
const PLL_FBDIV_INT_MASK: u32 = 0xfff;
const PLL_PRIM_POSTDIV1_SHIFT: u32 = 16;
const PLL_PRIM_POSTDIV2_SHIFT: u32 = 12;
const PLL_PRIM_POSTDIV_MASK: u32 = 0x7;

struct PllState {
    cs: u32,
    pwr: u32,
    fbdiv_int: u32,
    prim: u32,
}

#[derive(Clone)]
pub struct Pll {
    name: String,
    state: Rc<RefCell<PllState>>,
    board: Weak<RefCell<BoardShared>>,
}

impl Pll {
    pub fn new(name: impl Into<String>, board: Weak<RefCell<BoardShared>>) -> Self {
        Self {
            name: name.into(),
            state: Rc::new(RefCell::new(PllState {
                cs: 0x1,
                pwr: 0x2d,
                fbdiv_int: 0,
                prim: 0x77000,
            })),
            board,
        }
    }

    pub fn refdiv(&self) -> u32 {
        self.state.borrow().cs & PLL_CS_REFDIV_MASK
    }

    pub fn fbdiv(&self) -> u32 {
        self.state.borrow().fbdiv_int & PLL_FBDIV_INT_MASK
    }

    pub fn postdiv1(&self) -> u32 {
        (self.state.borrow().prim >> PLL_PRIM_POSTDIV1_SHIFT) & PLL_PRIM_POSTDIV_MASK
    }

    pub fn postdiv2(&self) -> u32 {
        (self.state.borrow().prim >> PLL_PRIM_POSTDIV2_SHIFT) & PLL_PRIM_POSTDIV_MASK
    }

    pub fn frequency_from(&self, xosc_freq: f64) -> f64 {
        let ref_freq = xosc_freq / f64::from(self.refdiv().max(1));
        if self.state.borrow().cs & PLL_CS_BYPASS != 0 {
            return ref_freq;
        }
        let postdiv = f64::from(self.postdiv1().max(1)) * f64::from(self.postdiv2().max(1));
        (ref_freq * f64::from(self.fbdiv())) / postdiv
    }

    fn read_register(&self, offset: u32) -> u32 {
        let state = self.state.borrow();
        match offset {
            PLL_CS => state.cs | PLL_CS_LOCK,
            PLL_PWR => state.pwr,
            PLL_FBDIV_INT => state.fbdiv_int,
            PLL_PRIM => state.prim,
            _ => 0xffff_ffff,
        }
    }

    fn write_register(&self, offset: u32, value: u32) {
        match offset {
            PLL_CS => self.state.borrow_mut().cs = value,
            PLL_PWR => self.state.borrow_mut().pwr = value,
            PLL_FBDIV_INT => self.state.borrow_mut().fbdiv_int = value,
            PLL_PRIM => self.state.borrow_mut().prim = value,
            _ => return,
        }
        if let Some(board) = self.board.upgrade() {
            crate::rp2040::update_clocks(&board);
        }
    }
}

impl Peripheral for Pll {
    fn name(&self) -> &str {
        &self.name
    }

    fn read_u32(&self, offset: u32) -> u32 {
        self.read_register(offset)
    }

    fn write_u32_atomic(&self, offset: u32, value: u32, atomic_type: u32) {
        let value = if atomic_type == 0 {
            value
        } else {
            atomic_update(self.read_register(offset), atomic_type, value)
        };
        self.write_register(offset, value);
    }
}
