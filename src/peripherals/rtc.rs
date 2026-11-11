use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::peripherals::peripheral::{atomic_update, Peripheral};
use crate::rp2040::BoardShared;

const SETUP0: u32 = 0x04;
const SETUP1: u32 = 0x08;
const CTRL: u32 = 0x0c;
const IRQ_SETUP_0: u32 = 0x10;
const RTC1: u32 = 0x18;
const RTC0: u32 = 0x1c;
const ENABLE: u32 = 0x01;
const ACTIVE: u32 = 0x02;
const LOAD: u32 = 0x10;

#[derive(Clone)]
pub struct Rtc {
    name: String,
    state: Rc<RefCell<RtcState>>,
    board: Weak<RefCell<BoardShared>>,
}

struct RtcState {
    setup0: u32,
    setup1: u32,
    ctrl: u32,
    baseline_seconds: i64,
    baseline_nanos: f64,
}

impl Rtc {
    pub fn new(name: impl Into<String>, board: Weak<RefCell<BoardShared>>) -> Self {
        Self {
            name: name.into(),
            state: Rc::new(RefCell::new(RtcState {
                setup0: 0,
                setup1: 0,
                ctrl: 0,
                baseline_seconds: days_from_civil(2021, 1, 1) * 86_400,
                baseline_nanos: 0.0,
            })),
            board,
        }
    }

    fn now(&self) -> (i64, u32) {
        let nanos = self
            .board
            .upgrade()
            .map(|b| b.borrow().clock.nanos())
            .unwrap_or(0.0);
        let state = self.state.borrow();
        let total = state.baseline_seconds as f64 + (nanos - state.baseline_nanos) / 1e9;
        let seconds = total.floor() as i64;
        (seconds, seconds.div_euclid(86_400) as u32)
    }
}

impl Peripheral for Rtc {
    fn name(&self) -> &str {
        &self.name
    }

    fn read_u32(&self, offset: u32) -> u32 {
        match offset {
            SETUP0 => self.state.borrow().setup0,
            SETUP1 => self.state.borrow().setup1,
            CTRL => self.state.borrow().ctrl,
            IRQ_SETUP_0 => 0,
            RTC1 => {
                let (seconds, _) = self.now();
                let days = seconds.div_euclid(86_400);
                let (year, month, day) = civil_from_days(days);
                ((year as u32 & 0xfff) << 12) | ((month & 0xf) << 8) | (day & 0x1f)
            }
            RTC0 => {
                let (seconds, _) = self.now();
                let days = seconds.div_euclid(86_400);
                let second_of_day = seconds.rem_euclid(86_400);
                let weekday = (days + 4).rem_euclid(7) as u32;
                let hour = (second_of_day / 3600) as u32;
                let minute = ((second_of_day / 60) % 60) as u32;
                let second = (second_of_day % 60) as u32;
                (weekday << 24) | (hour << 16) | (minute << 8) | second
            }
            _ => 0xffff_ffff,
        }
    }

    fn write_u32_atomic(&self, offset: u32, value: u32, atomic_type: u32) {
        let current = self.read_u32(offset);
        let value = if atomic_type == 0 {
            value
        } else {
            atomic_update(current, atomic_type, value)
        };
        match offset {
            SETUP0 => self.state.borrow_mut().setup0 = value,
            SETUP1 => self.state.borrow_mut().setup1 = value,
            CTRL => {
                let mut state = self.state.borrow_mut();
                if value & LOAD != 0 {
                    state.ctrl |= LOAD;
                }
                if value & ENABLE != 0 {
                    state.ctrl |= ENABLE | ACTIVE;
                    if state.ctrl & LOAD != 0 {
                        let year = (state.setup0 >> 12) & 0xfff;
                        let month = (state.setup0 >> 8) & 0xf;
                        let day = state.setup0 & 0x1f;
                        let hour = (state.setup1 >> 16) & 0x1f;
                        let minute = (state.setup1 >> 8) & 0x3f;
                        let second = state.setup1 & 0x3f;
                        state.baseline_seconds = days_from_civil(year as i32, month, day) * 86_400
                            + i64::from(hour * 3600 + minute * 60 + second);
                        state.baseline_nanos = self
                            .board
                            .upgrade()
                            .map(|b| b.borrow().clock.nanos())
                            .unwrap_or(0.0);
                        state.ctrl &= !LOAD;
                    }
                } else {
                    state.ctrl &= !(ENABLE | ACTIVE);
                }
            }
            _ => {}
        }
    }
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - i32::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = month as i32;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    i64::from(era * 146_097 + doe - 719_468)
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = year + i64::from(month <= 2);
    (year as i32, month as u32, day as u32)
}
