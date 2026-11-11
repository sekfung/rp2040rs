use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::clock::Alarm;
use crate::peripherals::peripheral::Peripheral;
use crate::rp2040::BoardShared;

const CTRL: u32 = 0x00;
const LOAD: u32 = 0x04;
const REASON: u32 = 0x08;
const SCRATCH0: u32 = 0x0c;
const TICK: u32 = 0x2c;
const TRIGGER: u32 = 1 << 31;
const ENABLE: u32 = 1 << 30;
const PAUSE_DBG1: u32 = 1 << 26;
const PAUSE_DBG0: u32 = 1 << 25;
const PAUSE_JTAG: u32 = 1 << 24;
const TIME_MASK: u32 = 0x00ff_ffff;
const REASON_FORCE: u32 = 1 << 1;
const REASON_TIMER: u32 = 1 << 0;
const RUNNING: u32 = 1 << 10;
const TICK_ENABLE: u32 = 1 << 9;
const TICK_FREQUENCY: f64 = 2_000_000.0;

#[derive(Clone)]
pub struct Watchdog {
    name: String,
    state: Rc<RefCell<WatchdogState>>,
}

struct WatchdogState {
    enable: bool,
    tick_enable: bool,
    reason: u32,
    pause_dbg0: bool,
    pause_dbg1: bool,
    pause_jtag: bool,
    counter: u32,
    load: u32,
    scratch: [u32; 8],
    alarm: Alarm,
    on_trigger: Option<Box<dyn FnMut()>>,
}

impl Watchdog {
    pub fn new(name: impl Into<String>, board: Weak<RefCell<BoardShared>>) -> Self {
        let clock = board
            .upgrade()
            .map(|b| b.borrow().clock.clone())
            .expect("watchdog requires a clock");
        let state = Rc::new(RefCell::new(WatchdogState {
            enable: false,
            tick_enable: true,
            reason: 0,
            pause_dbg0: true,
            pause_dbg1: true,
            pause_jtag: true,
            counter: 0,
            load: 0,
            scratch: [0; 8],
            alarm: clock.create_alarm(Box::new(|| {})),
            on_trigger: None,
        }));
        let state_for_alarm = state.clone();
        let alarm = clock.create_alarm(Box::new(move || {
            let mut state = state_for_alarm.borrow_mut();
            state.reason = REASON_TIMER;
            state.enable = false;
            if let Some(callback) = state.on_trigger.as_mut() {
                callback();
            }
        }));
        state.borrow_mut().alarm = alarm;
        Self {
            name: name.into(),
            state,
        }
    }

    pub fn set_on_watchdog_trigger(&self, callback: Box<dyn FnMut()>) {
        self.state.borrow_mut().on_trigger = Some(callback);
    }

    fn arm(&self) {
        let (enable, tick_enable, load) = {
            let state = self.state.borrow();
            (state.enable, state.tick_enable, state.load)
        };
        let mut state = self.state.borrow_mut();
        state.alarm.cancel();
        if enable && tick_enable {
            let nanos = f64::from(load & TIME_MASK) * 1_000_000_000.0 / TICK_FREQUENCY;
            state.alarm.schedule(nanos.max(1.0));
        }
    }
}

impl Peripheral for Watchdog {
    fn name(&self) -> &str {
        &self.name
    }

    fn read_u32(&self, offset: u32) -> u32 {
        let state = self.state.borrow();
        match offset {
            CTRL => {
                (u32::from(state.enable) * ENABLE)
                    | (u32::from(state.pause_dbg0) * PAUSE_DBG0)
                    | (u32::from(state.pause_dbg1) * PAUSE_DBG1)
                    | (u32::from(state.pause_jtag) * PAUSE_JTAG)
                    | (state.counter & TIME_MASK)
            }
            LOAD => state.load,
            REASON => state.reason,
            SCRATCH0..=0x28 => state.scratch[((offset - SCRATCH0) >> 2) as usize],
            TICK => {
                if state.tick_enable {
                    RUNNING | TICK_ENABLE
                } else {
                    0
                }
            }
            _ => 0xffff_ffff,
        }
    }

    fn write_u32_atomic(&self, offset: u32, value: u32, _atomic_type: u32) {
        match offset {
            CTRL => {
                let mut state = self.state.borrow_mut();
                if value & TRIGGER != 0 {
                    state.reason = REASON_FORCE;
                    if let Some(callback) = state.on_trigger.as_mut() {
                        callback();
                    }
                }
                state.enable = value & ENABLE != 0;
                state.pause_dbg0 = value & PAUSE_DBG0 != 0;
                state.pause_dbg1 = value & PAUSE_DBG1 != 0;
                state.pause_jtag = value & PAUSE_JTAG != 0;
                drop(state);
                self.arm();
            }
            LOAD => {
                let mut state = self.state.borrow_mut();
                state.load = value & TIME_MASK;
                state.counter = state.load;
                drop(state);
                self.arm();
            }
            SCRATCH0..=0x28 => {
                self.state.borrow_mut().scratch[((offset - SCRATCH0) >> 2) as usize] = value;
            }
            TICK => {
                let mut state = self.state.borrow_mut();
                state.tick_enable = value & TICK_ENABLE != 0;
                drop(state);
                self.arm();
            }
            _ => {}
        }
    }
}
