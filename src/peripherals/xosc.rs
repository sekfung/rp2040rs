use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::peripherals::peripheral::{atomic_update, Peripheral};
use crate::rp2040::BoardShared;

const XOSC_CTRL: u32 = 0x00;
const XOSC_STATUS: u32 = 0x04;
const XOSC_DORMANT: u32 = 0x08;
const XOSC_STARTUP: u32 = 0x0c;
const XOSC_COUNT: u32 = 0x1c;

const CTRL_ENABLE_LSB: u32 = 12;
const CTRL_ENABLE_BITS: u32 = 0x00ff_f000;
const CTRL_ENABLE_DISABLE: u32 = 0xd1e;
const CTRL_ENABLE_ENABLE: u32 = 0xfab;
const STATUS_STABLE: u32 = 0x8000_0000;
const STATUS_BADWRITE: u32 = 0x0100_0000;
const STATUS_ENABLED: u32 = 0x0000_1000;
const DORMANT_VALUE: u32 = 0x636f_6d61;
const WAKE_VALUE: u32 = 0x7761_6b65;
const STARTUP_X4: u32 = 0x0010_0000;
const STARTUP_DELAY_BITS: u32 = 0x0000_3fff;

struct XoscState {
    ctrl: u32,
    status: u32,
    dormant: u32,
    startup: u32,
    count: u32,
    enabled: bool,
    stable: bool,
    is_dormant: bool,
}

#[derive(Clone)]
pub struct Xosc {
    name: String,
    state: Rc<RefCell<XoscState>>,
    board: Weak<RefCell<BoardShared>>,
}

impl Xosc {
    pub fn new(name: impl Into<String>, board: Weak<RefCell<BoardShared>>) -> Self {
        Self {
            name: name.into(),
            state: Rc::new(RefCell::new(XoscState {
                ctrl: 0,
                status: 0,
                dormant: 0,
                startup: 0,
                count: 0,
                enabled: false,
                stable: false,
                is_dormant: false,
            })),
            board,
        }
    }

    fn read_register(&self, offset: u32) -> u32 {
        let state = self.state.borrow();
        match offset {
            XOSC_CTRL => state.ctrl,
            XOSC_STATUS => {
                state.status
                    | if state.stable { STATUS_STABLE } else { 0 }
                    | if state.enabled { STATUS_ENABLED } else { 0 }
            }
            XOSC_DORMANT => state.dormant,
            XOSC_STARTUP => state.startup,
            XOSC_COUNT => state.count,
            _ => 0xffff_ffff,
        }
    }

    fn write_register(&self, offset: u32, value: u32) {
        match offset {
            XOSC_CTRL => {
                let mut state = self.state.borrow_mut();
                state.ctrl = value;
                let enable = (value & CTRL_ENABLE_BITS) >> CTRL_ENABLE_LSB;
                if enable == CTRL_ENABLE_ENABLE {
                    if !state.is_dormant {
                        state.enabled = true;
                        state.stable = true;
                    }
                } else if enable == CTRL_ENABLE_DISABLE {
                    state.enabled = false;
                    state.stable = false;
                } else if enable != 0 {
                    state.status |= STATUS_BADWRITE;
                }
            }
            XOSC_STATUS => {
                if value & STATUS_BADWRITE != 0 {
                    self.state.borrow_mut().status &= !STATUS_BADWRITE;
                }
            }
            XOSC_DORMANT => {
                let mut state = self.state.borrow_mut();
                if value == DORMANT_VALUE {
                    state.is_dormant = true;
                    state.stable = false;
                } else if value == WAKE_VALUE {
                    state.is_dormant = false;
                    if state.enabled {
                        state.stable = true;
                    }
                }
                state.dormant = value;
            }
            XOSC_STARTUP => {
                self.state.borrow_mut().startup = value & (STARTUP_X4 | STARTUP_DELAY_BITS);
            }
            XOSC_COUNT => self.state.borrow_mut().count = value & 0xff,
            _ => {
                if let Some(board) = self.board.upgrade() {
                    let logger = board.borrow().logger.clone();
                    logger.borrow().warn(&self.name, "Unimplemented XOSC write");
                }
            }
        }
    }
}

impl Peripheral for Xosc {
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
