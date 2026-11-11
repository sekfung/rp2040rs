use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::clock::Alarm;
use crate::irq::Irq;
use crate::peripherals::peripheral::{atomic_update, Peripheral};
use crate::rp2040::BoardShared;
use crate::utils::Fifo;

const CS: u32 = 0x00;
const RESULT: u32 = 0x04;
const FCS: u32 = 0x08;
const FIFO_REG: u32 = 0x0c;
const DIV: u32 = 0x10;
const INTR: u32 = 0x14;
const INTE: u32 = 0x18;
const INTF: u32 = 0x1c;
const INTS: u32 = 0x20;

const CS_RROBIN_MASK: u32 = 0x1f;
const CS_RROBIN_SHIFT: u32 = 16;
const CS_AINSEL_MASK: u32 = 0x7;
const CS_AINSEL_SHIFT: u32 = 12;
const CS_ERR_STICKY: u32 = 1 << 10;
const CS_ERR: u32 = 1 << 9;
const CS_READY: u32 = 1 << 8;
const CS_START_MANY: u32 = 1 << 3;
const CS_START_ONE: u32 = 1 << 2;
const CS_EN: u32 = 1 << 0;
const CS_WRITE_MASK: u32 = 0x001f_700f;

const FCS_THRES_MASK: u32 = 0xf;
const FCS_THRESH_SHIFT: u32 = 24;
const FCS_LEVEL_MASK: u32 = 0xf;
const FCS_LEVEL_SHIFT: u32 = 16;
const FCS_OVER: u32 = 1 << 11;
const FCS_UNDER: u32 = 1 << 10;
const FCS_FULL: u32 = 1 << 9;
const FCS_EMPTY: u32 = 1 << 8;
const FCS_ERR: u32 = 1 << 2;
const FCS_SHIFT: u32 = 1 << 1;
const FCS_EN: u32 = 1 << 0;
const FCS_WRITE_MASK: u32 = 0x0f00_000f;
const FIFO_ERR: u32 = 1 << 15;
const FIFO_INT: u32 = 1 << 0;

#[derive(Clone)]
pub struct Adc {
    name: String,
    state: Rc<RefCell<AdcState>>,
    board: Weak<RefCell<BoardShared>>,
}

struct AdcState {
    cs: u32,
    fcs: u32,
    clock_div: u32,
    int_enable: u32,
    int_force: u32,
    result: u32,
    busy: bool,
    err: bool,
    current_channel: usize,
    channel_values: [u16; 5],
    fifo: Fifo,
    sample_alarm: Alarm,
    multi_alarm: Alarm,
}

impl Adc {
    pub fn new(name: impl Into<String>, board: Weak<RefCell<BoardShared>>) -> Self {
        let clock = board
            .upgrade()
            .map(|b| b.borrow().clock.clone())
            .expect("ADC requires a clock");
        let state = Rc::new(RefCell::new(AdcState {
            cs: 0,
            fcs: 0,
            clock_div: 0,
            int_enable: 0,
            int_force: 0,
            result: 0,
            busy: false,
            err: false,
            current_channel: 0,
            channel_values: [0; 5],
            fifo: Fifo::new(4),
            sample_alarm: clock.create_alarm(Box::new(|| {})),
            multi_alarm: clock.create_alarm(Box::new(|| {})),
        }));
        let sample_state = state.clone();
        let sample_board = board.clone();
        let sample_alarm = clock.create_alarm(Box::new(move || {
            let channel = sample_state.borrow().current_channel;
            let value = sample_state.borrow().channel_values[channel.min(4)] as u32;
            complete_adc_read(&sample_state, &sample_board, value, false);
        }));
        let multi_state = state.clone();
        let multi_board = board.clone();
        let multi_alarm = clock.create_alarm(Box::new(move || {
            if multi_state.borrow().cs & CS_START_MANY != 0 {
                start_adc_read(&multi_state, &multi_board);
            }
        }));
        {
            let mut state = state.borrow_mut();
            state.sample_alarm = sample_alarm;
            state.multi_alarm = multi_alarm;
        }
        Self {
            name: name.into(),
            state,
            board,
        }
    }

    pub fn set_channel_value(&self, channel: usize, value: u16) {
        if let Some(slot) = self.state.borrow_mut().channel_values.get_mut(channel) {
            *slot = value;
        }
    }

    fn int_raw(&self) -> u32 {
        let state = self.state.borrow();
        let threshold = (state.fcs >> FCS_THRESH_SHIFT) & FCS_THRES_MASK;
        u32::from(state.fifo.item_count() >= threshold as usize) * FIFO_INT
    }

    fn int_status(&self) -> u32 {
        let state = self.state.borrow();
        (self.int_raw() & state.int_enable) | state.int_force
    }

    fn check_interrupts(&self) {
        if let Some(board) = self.board.upgrade() {
            let core = board.borrow().core.clone();
            core.set_interrupt(Irq::AdcFifo as u32, self.int_status() != 0);
        }
    }
}

fn start_adc_read(state: &Rc<RefCell<AdcState>>, board: &Weak<RefCell<BoardShared>>) {
    let channel = ((state.borrow().cs >> CS_AINSEL_SHIFT) & CS_AINSEL_MASK) as usize;
    {
        let mut state = state.borrow_mut();
        state.current_channel = channel.min(4);
        state.busy = true;
    }
    let mut alarm = state.borrow().sample_alarm.clone();
    alarm.schedule(2_000.0);
    state.borrow_mut().sample_alarm = alarm;
    let _ = board;
}

fn complete_adc_read(
    state: &Rc<RefCell<AdcState>>,
    board: &Weak<RefCell<BoardShared>>,
    value: u32,
    error: bool,
) {
    let (int_status, enable_fifo, shift, fcs_err, round, start_many) = {
        let mut state = state.borrow_mut();
        state.busy = false;
        state.result = value;
        if error {
            state.cs |= CS_ERR_STICKY | CS_ERR;
        } else {
            state.cs &= !CS_ERR;
        }
        if state.fcs & FCS_EN != 0 {
            if state.fifo.full() {
                state.fcs |= FCS_OVER;
            } else {
                let mut value = value & 0xfff;
                if state.fcs & FCS_SHIFT != 0 {
                    value >>= 4;
                }
                if error && state.fcs & FCS_ERR != 0 {
                    value |= FIFO_ERR;
                }
                state.fifo.push(value);
            }
        }
        let round = (state.cs >> CS_RROBIN_SHIFT) & CS_RROBIN_MASK;
        if round != 0 {
            let mut channel = ((state.cs >> CS_AINSEL_SHIFT) & CS_AINSEL_MASK) + 1;
            while round & (1 << channel) == 0 {
                channel = (channel + 1) % 5;
            }
            state.cs = (state.cs & !(CS_AINSEL_MASK << CS_AINSEL_SHIFT))
                | ((channel & CS_AINSEL_MASK) << CS_AINSEL_SHIFT);
        }
        let threshold = (state.fcs >> FCS_THRESH_SHIFT) & FCS_THRES_MASK;
        let int_status = if state.fifo.item_count() >= threshold as usize {
            FIFO_INT
        } else {
            0
        };
        (
            int_status,
            state.fcs & FCS_EN != 0,
            state.fcs & FCS_SHIFT != 0,
            state.fcs & FCS_ERR != 0,
            round,
            state.cs & CS_START_MANY != 0,
        )
    };
    let _ = (enable_fifo, shift, fcs_err, value);
    if let Some(board) = board.upgrade() {
        let core = board.borrow().core.clone();
        let status = {
            let state = state.borrow();
            (int_status & state.int_enable) | state.int_force
        };
        core.set_interrupt(Irq::AdcFifo as u32, status != 0);
    }
    if start_many {
        let (divider, mut alarm) = {
            let state = state.borrow();
            (
                1.0 + ((state.clock_div >> 8) & 0xffff) as f64
                    + (state.clock_div & 0xff) as f64 / 256.0,
                state.multi_alarm.clone(),
            )
        };
        let sample_ticks = 48.0 * 2.0;
        let micros = if divider > sample_ticks {
            (divider - sample_ticks) / 48.0
        } else {
            0.0
        };
        if micros > 0.0 {
            alarm.schedule(micros * 1000.0);
            state.borrow_mut().multi_alarm = alarm;
        } else {
            start_adc_read(state, board);
        }
    }
    let _ = round;
}

impl Peripheral for Adc {
    fn name(&self) -> &str {
        &self.name
    }

    fn read_u32(&self, offset: u32) -> u32 {
        if offset == FIFO_REG {
            let mut state = self.state.borrow_mut();
            if state.fifo.empty() {
                state.fcs |= FCS_UNDER;
                return 0;
            }
            return state.fifo.pull();
        }
        let state = self.state.borrow();
        match offset {
            CS => {
                state.cs
                    | if state.err { CS_ERR } else { 0 }
                    | if state.busy { 0 } else { CS_READY }
            }
            RESULT => state.result,
            FCS => {
                state.fcs
                    | ((state.fifo.item_count() as u32 & FCS_LEVEL_MASK) << FCS_LEVEL_SHIFT)
                    | if state.fifo.full() { FCS_FULL } else { 0 }
                    | if state.fifo.empty() { FCS_EMPTY } else { 0 }
            }
            DIV => state.clock_div,
            INTR => self.int_raw(),
            INTE => state.int_enable,
            INTF => state.int_force,
            INTS => (self.int_raw() & state.int_enable) | state.int_force,
            _ => 0xffff_ffff,
        }
    }

    fn write_u32_atomic(&self, offset: u32, value: u32, atomic_type: u32) {
        let value = if atomic_type == 0 {
            value
        } else {
            atomic_update(self.read_u32(offset), atomic_type, value)
        };
        match offset {
            CS => {
                let mut state = self.state.borrow_mut();
                state.fcs &= !(value & CS_ERR_STICKY);
                state.cs = (state.cs & !CS_WRITE_MASK) | (value & CS_WRITE_MASK);
                let start = state.cs & CS_EN != 0
                    && !state.busy
                    && (state.cs & (CS_START_ONE | CS_START_MANY)) != 0;
                if start {
                    drop(state);
                    start_adc_read(&self.state, &self.board);
                }
            }
            FCS => {
                let mut state = self.state.borrow_mut();
                state.fcs &= !(value & (FCS_OVER | FCS_UNDER));
                state.fcs = (state.fcs & !FCS_WRITE_MASK) | (value & FCS_WRITE_MASK);
            }
            DIV => self.state.borrow_mut().clock_div = value,
            INTE => {
                self.state.borrow_mut().int_enable = value & FIFO_INT;
                self.check_interrupts();
            }
            INTF => {
                self.state.borrow_mut().int_force = value & FIFO_INT;
                self.check_interrupts();
            }
            _ => {}
        }
    }
}
