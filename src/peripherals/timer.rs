use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::clock::Alarm;
use crate::irq::Irq;
use crate::peripherals::peripheral::{atomic_update, Peripheral};
use crate::rp2040::BoardShared;

const TIMEHR: u32 = 0x08;
const TIMELR: u32 = 0x0c;
const TIMERAWH: u32 = 0x24;
const TIMERAWL: u32 = 0x28;
const ALARM0: u32 = 0x10;
const ARMED: u32 = 0x20;
const PAUSE: u32 = 0x30;
const INTR: u32 = 0x34;
const INTE: u32 = 0x38;
const INTF: u32 = 0x3c;
const INTS: u32 = 0x40;

#[derive(Clone)]
pub struct Timer {
    name: String,
    state: Rc<RefCell<TimerState>>,
    board: Weak<RefCell<BoardShared>>,
}

struct TimerState {
    alarms: Vec<AlarmState>,
    latched_high: u32,
    int_raw: u32,
    int_enable: u32,
    int_force: u32,
    paused: bool,
}

struct AlarmState {
    armed: bool,
    target_micros: u32,
    alarm: Alarm,
}

impl Timer {
    pub fn new(name: impl Into<String>, board: Weak<RefCell<BoardShared>>) -> Self {
        let clock = board
            .upgrade()
            .map(|b| b.borrow().clock.clone())
            .expect("timer requires a board with a clock");
        let state = Rc::new(RefCell::new(TimerState {
            alarms: Vec::with_capacity(4),
            latched_high: 0,
            int_raw: 0,
            int_enable: 0,
            int_force: 0,
            paused: false,
        }));
        let mut alarms = Vec::with_capacity(4);
        for index in 0..4 {
            let state_for_alarm = state.clone();
            let board_for_alarm = board.clone();
            let alarm = clock.create_alarm(Box::new(move || {
                fire_alarm(&state_for_alarm, &board_for_alarm, index);
            }));
            alarms.push(AlarmState {
                armed: false,
                target_micros: 0,
                alarm,
            });
        }
        state.borrow_mut().alarms = alarms;
        Self {
            name: name.into(),
            state,
            board,
        }
    }

    fn now_micros(&self) -> f64 {
        self.board
            .upgrade()
            .map(|b| b.borrow().clock.nanos() / 1000.0)
            .unwrap_or(0.0)
    }

    fn now_micros_u32(&self) -> u32 {
        self.now_micros() as u64 as u32
    }

    fn int_status(&self) -> u32 {
        let state = self.state.borrow();
        (state.int_raw & state.int_enable) | state.int_force
    }

    fn check_interrupts(&self) {
        let status = self.int_status();
        if let Some(board) = self.board.upgrade() {
            let core = board.borrow().core.clone();
            for index in 0..4 {
                core.set_interrupt(Irq::Timer0 as u32 + index, status & (1 << index) != 0);
            }
        }
    }

    fn disarm(&self, index: usize) {
        let mut state = self.state.borrow_mut();
        if let Some(alarm) = state.alarms.get_mut(index) {
            alarm.alarm.cancel();
            alarm.armed = false;
        }
    }
}

fn fire_alarm(state: &Rc<RefCell<TimerState>>, board: &Weak<RefCell<BoardShared>>, index: usize) {
    {
        let mut state = state.borrow_mut();
        if let Some(alarm) = state.alarms.get_mut(index) {
            alarm.armed = false;
        }
        state.int_raw |= 1 << index;
    }
    if let Some(board) = board.upgrade() {
        let core = board.borrow().core.clone();
        let status = {
            let state = state.borrow();
            (state.int_raw & state.int_enable) | state.int_force
        };
        for i in 0..4 {
            core.set_interrupt(Irq::Timer0 as u32 + i, status & (1 << i) != 0);
        }
    }
}

impl Peripheral for Timer {
    fn name(&self) -> &str {
        &self.name
    }

    fn read_u32(&self, offset: u32) -> u32 {
        let time = self.now_micros();
        match offset {
            TIMEHR => self.state.borrow().latched_high,
            TIMELR => {
                let mut state = self.state.borrow_mut();
                state.latched_high = (time / 2f64.powi(32)).floor() as u32;
                self.now_micros_u32()
            }
            TIMERAWH => (time / 2f64.powi(32)).floor() as u32,
            TIMERAWL => self.now_micros_u32(),
            ALARM0..=0x1c => {
                let index = ((offset - ALARM0) / 4) as usize;
                self.state
                    .borrow()
                    .alarms
                    .get(index)
                    .map(|a| a.target_micros)
                    .unwrap_or(0)
            }
            PAUSE => u32::from(self.state.borrow().paused),
            INTR => self.state.borrow().int_raw,
            INTE => self.state.borrow().int_enable,
            INTF => self.state.borrow().int_force,
            INTS => self.int_status(),
            ARMED => {
                let state = self.state.borrow();
                state
                    .alarms
                    .iter()
                    .enumerate()
                    .fold(0, |acc, (i, a)| if a.armed { acc | (1 << i) } else { acc })
            }
            _ => 0xffff_ffff,
        }
    }

    fn write_u32_atomic(&self, offset: u32, value: u32, atomic_type: u32) {
        let current = self.read_u32(offset);
        let atomic_value = if atomic_type == 0 {
            value
        } else {
            atomic_update(current, atomic_type, value)
        };
        match offset {
            ALARM0..=0x1c => {
                let index = ((offset - ALARM0) / 4) as usize;
                let now = self.now_micros_u32();
                let delta_micros = value.wrapping_sub(now);
                let mut state = self.state.borrow_mut();
                if let Some(alarm) = state.alarms.get_mut(index) {
                    alarm.armed = true;
                    alarm.target_micros = value;
                    alarm.alarm.schedule(f64::from(delta_micros) * 1000.0);
                }
            }
            ARMED => {
                for index in 0..4 {
                    if value & (1 << index) != 0 {
                        self.disarm(index);
                    }
                }
            }
            PAUSE => self.state.borrow_mut().paused = (atomic_value & 1) != 0,
            INTR => {
                self.state.borrow_mut().int_raw &= !value;
                self.check_interrupts();
            }
            INTE => {
                self.state.borrow_mut().int_enable = atomic_value & 0xf;
                self.check_interrupts();
            }
            INTF => {
                self.state.borrow_mut().int_force = atomic_value & 0xf;
                self.check_interrupts();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::MockClock;
    use crate::rp2040::Rp2040;

    const ALARM1: u32 = 0x4005_4014;
    const ALARM2: u32 = 0x4005_4018;
    const ALARM3: u32 = 0x4005_401c;
    const ARMED: u32 = 0x4005_4020;
    const INTR: u32 = 0x4005_4034;
    const INTE: u32 = 0x4005_4038;
    const INTF: u32 = 0x4005_403c;
    const INTS: u32 = 0x4005_4040;

    #[test]
    fn should_set_alarm1_to_armed_when_writing_alarm1() {
        let clock = Rc::new(MockClock::new());
        let rp = Rp2040::with_clock(clock);
        rp.write_u32(ALARM1, 0x1000);
        assert_eq!(rp.read_u32(ARMED), 0x2);
    }

    #[test]
    fn should_disarm_alarm2_when_writing_armed() {
        let rp = Rp2040::new();
        rp.write_u32(ALARM2, 0x1000);
        assert_eq!(rp.read_u32(ARMED), 0x4);
        rp.write_u32(ARMED, 0xff);
        assert_eq!(rp.read_u32(ARMED), 0);
    }

    #[test]
    fn should_generate_irq3_when_alarm3_fires() {
        let clock = Rc::new(MockClock::new());
        let rp = Rp2040::with_clock(clock.clone());
        rp.write_u32(ALARM3, 1000);
        assert_eq!(rp.read_u32(ARMED), 0x8);
        assert_eq!(rp.read_u32(INTR), 0);
        clock.advance(2000);
        assert_eq!(rp.read_u32(ARMED), 0);
        assert_eq!(rp.read_u32(INTR), 0x8);
        assert_eq!(rp.read_u32(INTS), 0);
        assert_eq!(rp.core.pending_interrupts(), 0);
        rp.write_u32(INTE, 0xff);
        assert_eq!(rp.read_u32(INTS), 0x8);
        assert_eq!(rp.core.pending_interrupts(), 0x8);
        assert!(rp.core.interrupts_updated());
        rp.write_u32(INTR | 0x3000, 0x8);
        assert_eq!(rp.read_u32(INTS), 0);
        assert_eq!(rp.core.pending_interrupts(), 0);
    }

    #[test]
    fn should_generate_interrupt_if_intf_is_one_even_when_inte_is_zero() {
        let clock = Rc::new(MockClock::new());
        let rp = Rp2040::with_clock(clock);
        assert_eq!(rp.read_u32(INTS), 0);
        assert_eq!(rp.read_u32(INTE), 0);
        rp.write_u32(INTF, 0x4);
        assert_eq!(rp.read_u32(INTS), 0x4);
    }
}
