use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::irq::Irq;
use crate::peripherals::peripheral::{atomic_update, Peripheral};
use crate::rp2040::BoardShared;
use crate::utils::Fifo;

const IC_CON: u32 = 0x00;
const IC_TAR: u32 = 0x04;
const IC_SAR: u32 = 0x08;
const IC_DATA_CMD: u32 = 0x10;
const IC_SS_SCL_HCNT: u32 = 0x14;
const IC_SS_SCL_LCNT: u32 = 0x18;
const IC_FS_SCL_HCNT: u32 = 0x1c;
const IC_FS_SCL_LCNT: u32 = 0x20;
const IC_INTR_STAT: u32 = 0x2c;
const IC_INTR_MASK: u32 = 0x30;
const IC_RAW_INTR_STAT: u32 = 0x34;
const IC_RX_TL: u32 = 0x38;
const IC_TX_TL: u32 = 0x3c;
const IC_CLR_INTR: u32 = 0x40;
const IC_ENABLE: u32 = 0x6c;
const IC_STATUS: u32 = 0x70;
const IC_TXFLR: u32 = 0x74;
const IC_RXFLR: u32 = 0x78;
const IC_TX_ABRT_SOURCE: u32 = 0x80;
const IC_ENABLE_STATUS: u32 = 0x9c;
const IC_FS_SPKLEN: u32 = 0xa0;
const IC_COMP_VERSION: u32 = 0xf8;
const IC_COMP_TYPE: u32 = 0xfc;

const IC_RESTART_EN: u32 = 1 << 5;
const IC_SLAVE_DISABLE: u32 = 1 << 6;
const SPEED_MASK: u32 = 0x3;
const SPEED_SHIFT: u32 = 1;
const MASTER_MODE: u32 = 1;
const ENABLE: u32 = 1;
const ABORT: u32 = 1 << 1;
const TX_CMD_BLOCK: u32 = 1 << 2;
const MST_ACTIVITY: u32 = 1 << 5;
const RFF: u32 = 1 << 4;
const RFNE: u32 = 1 << 3;
const TFE: u32 = 1 << 2;
const TFNF: u32 = 1 << 1;
const ACTIVITY: u32 = 1;
const CMD: u32 = 1 << 8;
const STOP: u32 = 1 << 9;
const RESTART: u32 = 1 << 10;
const FIRST_DATA_BYTE: u32 = 1 << 10;

const R_TX_EMPTY: u32 = 1 << 4;
const R_TX_OVER: u32 = 1 << 3;
const R_RX_FULL: u32 = 1 << 2;
const R_RX_OVER: u32 = 1 << 1;
const R_RX_UNDER: u32 = 1;
const R_STOP_DET: u32 = 1 << 9;
const R_START_DET: u32 = 1 << 10;
const R_TX_ABRT: u32 = 1 << 6;

#[derive(Clone, Copy, PartialEq, Eq)]
enum I2cState {
    Idle,
    Start,
    Connect,
    Connected,
}

/// I2C transfer direction, matching upstream `I2CMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum I2cMode {
    Write = 0,
    Read = 1,
}

/// I2C bus speed class, matching upstream `I2CSpeed`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum I2cSpeed {
    Invalid = 0,
    Standard = 1,
    FastMode = 2,
    HighSpeedMode = 3,
}

#[derive(Clone)]
pub struct I2c {
    name: String,
    state: Rc<RefCell<I2cStateMachine>>,
    board: Weak<RefCell<BoardShared>>,
    irq: u32,
}

struct I2cStateMachine {
    state: I2cState,
    busy: bool,
    stop: bool,
    pending_restart: bool,
    first_byte: bool,
    rx_fifo: Fifo,
    tx_fifo: Fifo,
    enable: u32,
    rx_threshold: u32,
    tx_threshold: u32,
    control: u32,
    ss_clock_high: u32,
    ss_clock_low: u32,
    fs_clock_high: u32,
    fs_clock_low: u32,
    target_address: u32,
    slave_address: u32,
    abort_source: u32,
    int_raw: u32,
    int_enable: u32,
    spike_len: u32,
    on_start: Option<Box<dyn FnMut()>>,
    on_connect: Option<Box<dyn FnMut(u32, bool) -> bool>>,
    on_write: Option<Box<dyn FnMut(u32) -> bool>>,
    on_read: Option<Box<dyn FnMut() -> u32>>,
    on_stop: Option<Box<dyn FnMut()>>,
}

impl I2c {
    pub fn new(name: impl Into<String>, irq: Irq, _board: Weak<RefCell<BoardShared>>) -> Self {
        Self {
            name: name.into(),
            state: Rc::new(RefCell::new(I2cStateMachine {
                state: I2cState::Idle,
                busy: false,
                stop: false,
                pending_restart: false,
                first_byte: false,
                rx_fifo: Fifo::new(16),
                tx_fifo: Fifo::new(16),
                enable: 0,
                rx_threshold: 0,
                tx_threshold: 0,
                control: IC_SLAVE_DISABLE | IC_RESTART_EN | (2 << SPEED_SHIFT) | MASTER_MODE,
                ss_clock_high: 0x28,
                ss_clock_low: 0x2f,
                fs_clock_high: 0x06,
                fs_clock_low: 0x0d,
                target_address: 0x55,
                slave_address: 0x55,
                abort_source: 0,
                int_raw: 0,
                int_enable: 0,
                spike_len: 7,
                on_start: None,
                on_connect: None,
                on_write: None,
                on_read: None,
                on_stop: None,
            })),
            board: _board,
            irq: irq as u32,
        }
    }

    pub fn set_on_start(&self, callback: Box<dyn FnMut()>) {
        self.state.borrow_mut().on_start = Some(callback);
    }

    pub fn set_on_connect(&self, callback: Box<dyn FnMut(u32, bool) -> bool>) {
        self.state.borrow_mut().on_connect = Some(callback);
    }

    pub fn set_on_write(&self, callback: Box<dyn FnMut(u32) -> bool>) {
        self.state.borrow_mut().on_write = Some(callback);
    }

    pub fn set_on_read(&self, callback: Box<dyn FnMut() -> u32>) {
        self.state.borrow_mut().on_read = Some(callback);
    }

    pub fn set_on_stop(&self, callback: Box<dyn FnMut()>) {
        self.state.borrow_mut().on_stop = Some(callback);
    }

    fn int_status(&self) -> u32 {
        let state = self.state.borrow();
        state.int_raw & state.int_enable
    }

    fn check_interrupts(&self) {
        if let Some(board) = self.board.upgrade() {
            board
                .borrow()
                .core
                .clone()
                .set_interrupt(self.irq, self.int_status() != 0);
        }
    }

    fn set_interrupts(&self, mask: u32) {
        let changed = {
            let mut state = self.state.borrow_mut();
            if state.int_raw & mask == 0 {
                state.int_raw |= mask;
                true
            } else {
                false
            }
        };
        if changed {
            self.check_interrupts();
        }
    }

    fn clear_interrupts(&self, mask: u32) -> u32 {
        let changed = {
            let mut state = self.state.borrow_mut();
            if state.int_raw & mask != 0 {
                state.int_raw &= !mask;
                true
            } else {
                false
            }
        };
        if changed {
            self.check_interrupts();
        }
        u32::from(changed)
    }

    fn next_command(&self) {
        let (enabled, blocked, empty, busy, restart, idle) = {
            let state = self.state.borrow();
            (
                state.enable & ENABLE != 0,
                state.enable & TX_CMD_BLOCK != 0,
                state.tx_fifo.empty(),
                state.busy,
                !state.stop && (state.tx_fifo.peek() & RESTART) != 0,
                state.state == I2cState::Idle,
            )
        };
        if empty || busy || blocked || !enabled {
            return;
        }
        self.state.borrow_mut().busy = true;
        if (idle || restart) && !self.state.borrow().pending_restart {
            let mut state = self.state.borrow_mut();
            state.pending_restart = true;
            state.stop = false;
            state.state = I2cState::Start;
            drop(state);
            self.complete_start();
            return;
        }
        self.state.borrow_mut().pending_restart = false;
        let cmd = self.state.borrow_mut().tx_fifo.pull();
        let read_mode = cmd & CMD != 0;
        self.state.borrow_mut().stop = cmd & STOP != 0;
        if read_mode {
            self.complete_read(0xff);
        } else {
            self.complete_write(true, cmd & 0xff);
        }
        let threshold = self.state.borrow().tx_threshold;
        if self.state.borrow().tx_fifo.item_count() as u32 <= threshold {
            self.set_interrupts(R_TX_EMPTY);
        }
    }

    fn complete_start(&self) {
        let mode_read = self.state.borrow().tx_fifo.peek() & CMD != 0;
        let target = self.state.borrow().target_address;
        let empty = self.state.borrow().tx_fifo.empty();
        if empty || self.state.borrow().state != I2cState::Start || self.state.borrow().stop {
            self.complete_stop();
            return;
        }
        self.state.borrow_mut().state = I2cState::Connect;
        self.set_interrupts(R_START_DET);
        let mut callback = self.state.borrow_mut().on_start.take();
        if let Some(callback) = callback.as_mut() {
            callback();
        }
        self.state.borrow_mut().on_start = callback;
        let mut connect = self.state.borrow_mut().on_connect.take();
        let ack = match connect.as_mut() {
            Some(callback) => callback(target, mode_read),
            None => true,
        };
        self.state.borrow_mut().on_connect = connect;
        self.complete_connect(ack);
    }

    fn complete_connect(&self, ack: bool) {
        if !ack {
            self.abort(1);
            self.complete_stop();
            return;
        }
        {
            let mut state = self.state.borrow_mut();
            state.state = I2cState::Connected;
            state.busy = false;
            state.first_byte = true;
        }
        self.next_command();
    }

    fn complete_write(&self, ack: bool, value: u32) {
        let mut callback = self.state.borrow_mut().on_write.take();
        let ack = match callback.as_mut() {
            Some(callback) => callback(value),
            None => ack,
        };
        self.state.borrow_mut().on_write = callback;
        if !ack {
            self.abort(1 << 3);
            self.complete_stop();
            return;
        }
        self.state.borrow_mut().busy = false;
        self.next_command();
    }

    fn complete_read(&self, value: u32) {
        let mut callback = self.state.borrow_mut().on_read.take();
        let value = match callback.as_mut() {
            Some(callback) => callback(),
            None => value,
        };
        self.state.borrow_mut().on_read = callback;
        {
            let mut state = self.state.borrow_mut();
            let first_byte = state.first_byte;
            if state.rx_fifo.full() {
                state.int_raw |= R_RX_OVER;
            } else {
                state
                    .rx_fifo
                    .push(value | if first_byte { FIRST_DATA_BYTE } else { 0 });
                if state.rx_fifo.item_count() as u32 > state.rx_threshold {
                    state.int_raw |= R_RX_FULL;
                }
            }
        }
        self.check_interrupts();
        if self.state.borrow().stop {
            self.complete_stop();
            return;
        }
        self.state.borrow_mut().first_byte = false;
        self.state.borrow_mut().busy = false;
        self.next_command();
    }

    fn complete_stop(&self) {
        let was_abort = {
            let mut state = self.state.borrow_mut();
            state.state = I2cState::Idle;
            state.busy = false;
            state.pending_restart = false;
            let was_abort = state.enable & ABORT != 0;
            if state.enable & ABORT != 0 {
                state.enable &= !ABORT;
            }
            was_abort
        };
        let mut callback = self.state.borrow_mut().on_stop.take();
        if let Some(callback) = callback.as_mut() {
            callback();
        }
        self.state.borrow_mut().on_stop = callback;
        self.set_interrupts(R_STOP_DET);
        if !was_abort {
            self.next_command();
        }
    }

    fn abort(&self, reason: u32) {
        {
            let mut state = self.state.borrow_mut();
            state.abort_source &= 1 << 9;
            state.abort_source |= reason | ((state.tx_fifo.item_count() as u32) << 23);
            state.tx_fifo.reset();
        }
        self.set_interrupts(R_TX_ABRT);
    }
}

impl Peripheral for I2c {
    fn name(&self) -> &str {
        &self.name
    }

    fn read_u32(&self, offset: u32) -> u32 {
        match offset {
            IC_CON => self.state.borrow().control,
            IC_TAR => self.state.borrow().target_address,
            IC_SAR => self.state.borrow().slave_address,
            IC_DATA_CMD => {
                let value = {
                    let mut state = self.state.borrow_mut();
                    if state.rx_fifo.empty() {
                        state.int_raw |= R_RX_UNDER;
                        0
                    } else {
                        let value = state.rx_fifo.pull();
                        state.int_raw &= !R_RX_FULL;
                        value
                    }
                };
                self.check_interrupts();
                value
            }
            IC_SS_SCL_HCNT => self.state.borrow().ss_clock_high,
            IC_SS_SCL_LCNT => self.state.borrow().ss_clock_low,
            IC_FS_SCL_HCNT => self.state.borrow().fs_clock_high,
            IC_FS_SCL_LCNT => self.state.borrow().fs_clock_low,
            IC_INTR_STAT => self.int_status(),
            IC_INTR_MASK => self.state.borrow().int_enable,
            IC_RAW_INTR_STAT => self.state.borrow().int_raw,
            IC_RX_TL => self.state.borrow().rx_threshold,
            IC_TX_TL => self.state.borrow().tx_threshold,
            IC_CLR_INTR => {
                self.state.borrow_mut().abort_source &= 1 << 9;
                self.clear_interrupts(0xfff)
            }
            IC_ENABLE => self.state.borrow().enable,
            IC_STATUS => {
                let state = self.state.borrow();
                (if state.state != I2cState::Idle {
                    MST_ACTIVITY | ACTIVITY
                } else {
                    0
                }) | if state.rx_fifo.full() { RFF } else { 0 }
                    | if !state.rx_fifo.empty() { RFNE } else { 0 }
                    | if state.tx_fifo.empty() { TFE } else { 0 }
                    | if !state.tx_fifo.full() { TFNF } else { 0 }
            }
            IC_TXFLR => self.state.borrow().tx_fifo.item_count() as u32,
            IC_RXFLR => self.state.borrow().rx_fifo.item_count() as u32,
            IC_TX_ABRT_SOURCE => {
                let value = self.state.borrow().abort_source;
                self.state.borrow_mut().abort_source &= 1 << 9;
                value
            }
            IC_ENABLE_STATUS => self.state.borrow().enable & 1,
            IC_FS_SPKLEN => self.state.borrow().spike_len & 0xff,
            IC_COMP_VERSION => 0x3230_312a,
            IC_COMP_TYPE => 0x4457_0140,
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
            IC_CON => {
                let mut value = value;
                if ((value >> SPEED_SHIFT) & SPEED_MASK) == 0 {
                    value = (value & !(SPEED_MASK << SPEED_SHIFT)) | (3 << SPEED_SHIFT);
                }
                self.state.borrow_mut().control = value;
            }
            IC_TAR => self.state.borrow_mut().target_address = value & 0x3ff,
            IC_SAR => self.state.borrow_mut().slave_address = value & 0x3ff,
            IC_DATA_CMD => {
                let mut state = self.state.borrow_mut();
                if state.tx_fifo.full() {
                    state.int_raw |= R_TX_OVER;
                } else {
                    state.tx_fifo.push(value);
                    state.int_raw &= !R_TX_EMPTY;
                }
                drop(state);
                self.next_command();
            }
            IC_SS_SCL_HCNT => self.state.borrow_mut().ss_clock_high = value & 0xffff,
            IC_SS_SCL_LCNT => self.state.borrow_mut().ss_clock_low = value & 0xffff,
            IC_FS_SCL_HCNT => self.state.borrow_mut().fs_clock_high = value & 0xffff,
            IC_FS_SCL_LCNT => self.state.borrow_mut().fs_clock_low = value & 0xffff,
            IC_RX_TL => self.state.borrow_mut().rx_threshold = (value & 0xff).min(16),
            IC_TX_TL => self.state.borrow_mut().tx_threshold = (value & 0xff).min(16),
            IC_ENABLE => {
                let (mut value, idle) = {
                    let state = self.state.borrow();
                    (
                        value | (state.enable & ABORT),
                        state.state == I2cState::Idle,
                    )
                };
                if value & ABORT != 0 {
                    if idle {
                        value &= !ABORT;
                    } else {
                        self.abort(1 << 16);
                    }
                }
                {
                    let mut state = self.state.borrow_mut();
                    if value & ABORT != 0 && !idle {
                        state.stop = true;
                    }
                    if value & ENABLE == 0 {
                        state.tx_fifo.reset();
                        state.rx_fifo.reset();
                    }
                    state.enable = value;
                }
                self.next_command();
            }
            IC_FS_SPKLEN if value > 0 => self.state.borrow_mut().spike_len = value,
            _ => {}
        }
    }
}
