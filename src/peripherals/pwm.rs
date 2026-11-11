use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::peripherals::peripheral::{atomic_update, Peripheral};
use crate::rp2040::BoardShared;
use crate::utils::Timer32;

#[derive(Clone)]
pub struct PwmChannel {
    pub timer: Timer32,
}

#[derive(Clone)]
pub struct Pwm {
    name: String,
    pub channels: [PwmChannel; 8],
    state: Rc<RefCell<PwmState>>,
    board: Weak<RefCell<BoardShared>>,
}

struct PwmState {
    raw: Vec<u32>,
}

impl Pwm {
    pub fn new(name: impl Into<String>, board: Weak<RefCell<BoardShared>>) -> Self {
        Self {
            name: name.into(),
            channels: std::array::from_fn(|_| PwmChannel {
                timer: Timer32::new(125_000_000.0),
            }),
            state: Rc::new(RefCell::new(PwmState {
                raw: vec![0; 0x100],
            })),
            board,
        }
    }

    pub fn set_clock_freq(&self, frequency: f64) {
        for channel in &self.channels {
            channel.timer.set_frequency(frequency);
        }
    }
}

impl Peripheral for Pwm {
    fn name(&self) -> &str {
        &self.name
    }

    fn read_u32(&self, offset: u32) -> u32 {
        self.state
            .borrow()
            .raw
            .get(offset as usize / 4)
            .copied()
            .unwrap_or(0)
    }

    fn write_u32_atomic(&self, offset: u32, value: u32, atomic_type: u32) {
        let current = self.read_u32(offset);
        let value = if atomic_type == 0 {
            value
        } else {
            atomic_update(current, atomic_type, value)
        };
        if let Some(slot) = self.state.borrow_mut().raw.get_mut(offset as usize / 4) {
            *slot = value;
        }
        if let Some(board) = self.board.upgrade() {
            let logger = board.borrow().logger.clone();
            logger.borrow().debug(&self.name, "PWM register written");
        }
    }
}
