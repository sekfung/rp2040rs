use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::clock::Alarm;
use crate::irq::Irq;
use crate::peripherals::peripheral::{atomic_update, Peripheral};
use crate::rp2040::BoardShared;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum DreqChannel {
    Pio0Tx0 = 0,
    Pio0Tx1 = 1,
    Pio0Tx2 = 2,
    Pio0Tx3 = 3,
    Pio0Rx0 = 4,
    Pio0Rx1 = 5,
    Pio0Rx2 = 6,
    Pio0Rx3 = 7,
    Pio1Tx0 = 8,
    Pio1Tx1 = 9,
    Pio1Tx2 = 10,
    Pio1Tx3 = 11,
    Pio1Rx0 = 12,
    Pio1Rx1 = 13,
    Pio1Rx2 = 14,
    Pio1Rx3 = 15,
    Spi0Tx = 16,
    Spi0Rx = 17,
    Spi1Tx = 18,
    Spi1Rx = 19,
    Uart0Tx = 20,
    Uart0Rx = 21,
    Uart1Tx = 22,
    Uart1Rx = 23,
    PwmWrap0 = 24,
    PwmWrap1 = 25,
    PwmWrap2 = 26,
    PwmWrap3 = 27,
    PwmWrap4 = 28,
    PwmWrap5 = 29,
    PwmWrap6 = 30,
    PwmWrap7 = 31,
    I2c0Tx = 32,
    I2c0Rx = 33,
    I2c1Tx = 34,
    I2c1Rx = 35,
    Adc = 36,
    XipStream = 37,
    XipSsiTx = 38,
    XipSsiRx = 39,
}

pub const DREQ_MAX: usize = 40;
const TREQ_PERMANENT: u32 = 0x3f;
const TREQ_TIMER0: u32 = 0x3b;
const TREQ_TIMER1: u32 = 0x3c;
const TREQ_TIMER2: u32 = 0x3d;
const TREQ_TIMER3: u32 = 0x3e;

const CH_READ_ADDR: u32 = 0x000;
const CH_WRITE_ADDR: u32 = 0x004;
const CH_TRANS_COUNT: u32 = 0x008;
const CH_CTRL_TRIG: u32 = 0x00c;
const CH_AL1_CTRL: u32 = 0x010;
const CH_AL1_READ_ADDR: u32 = 0x014;
const CH_AL1_WRITE_ADDR: u32 = 0x018;
const CH_AL1_TRANS_COUNT_TRIG: u32 = 0x01c;
const CH_AL2_CTRL: u32 = 0x020;
const CH_AL2_TRANS_COUNT: u32 = 0x024;
const CH_AL2_READ_ADDR: u32 = 0x028;
const CH_AL2_WRITE_ADDR_TRIG: u32 = 0x02c;
const CH_AL3_CTRL: u32 = 0x030;
const CH_AL3_WRITE_ADDR: u32 = 0x034;
const CH_AL3_TRANS_COUNT: u32 = 0x038;
const CH_AL3_READ_ADDR_TRIG: u32 = 0x03c;
const CH_DBG_CTDREQ: u32 = 0x800;
const CH_DBG_TCR: u32 = 0x804;
const CHANNEL_REGISTERS_SIZE: u32 = 12 * 0x40;
const CHANNEL_REGISTERS_MASK: u32 = 0x83f;

const INTR: u32 = 0x400;
const INTE0: u32 = 0x404;
const INTF0: u32 = 0x408;
const INTS0: u32 = 0x40c;
const INTE1: u32 = 0x414;
const INTF1: u32 = 0x418;
const INTS1: u32 = 0x41c;
const TIMER0: u32 = 0x420;
const TIMER1: u32 = 0x424;
const TIMER2: u32 = 0x428;
const TIMER3: u32 = 0x42c;
const MULTI_CHAN_TRIGGER: u32 = 0x430;
const CHAN_ABORT: u32 = 0x444;
const N_CHANNELS: u32 = 0x448;

const READ_ERROR: u32 = 1 << 30;
const WRITE_ERROR: u32 = 1 << 29;
const BUSY: u32 = 1 << 24;
const BSWAP: u32 = 1 << 22;
const IRQ_QUIET: u32 = 1 << 21;
const TREQ_SEL_MASK: u32 = 0x3f;
const TREQ_SEL_SHIFT: u32 = 15;
const CHAIN_TO_MASK: u32 = 0xf;
const CHAIN_TO_SHIFT: u32 = 11;
const RING_SEL: u32 = 1 << 10;
const RING_SIZE_MASK: u32 = 0xf;
const RING_SIZE_SHIFT: u32 = 6;
const INCR_WRITE: u32 = 1 << 5;
const INCR_READ: u32 = 1 << 4;
const DATA_SIZE_MASK: u32 = 0x3;
const DATA_SIZE_SHIFT: u32 = 2;
const EN: u32 = 1 << 0;
const CTRL_WRITE_MASK: u32 = 0x00ff_ffff;
const CTRL_WRITE_CLEAR_MASK: u32 = READ_ERROR | WRITE_ERROR;

struct Channel {
    ctrl: u32,
    read_addr: u32,
    write_addr: u32,
    trans_count: u32,
    dreq_counter: u32,
    trans_count_reload: u32,
    treq_value: u32,
    data_size: u32,
    chain_to: u32,
    ring_mask: u32,
    transfer_alarm: Alarm,
}

struct DmaState {
    channels: Vec<Channel>,
    int_raw: u32,
    int_enable0: u32,
    int_force0: u32,
    int_enable1: u32,
    int_force1: u32,
    timers: [u32; 4],
    dreq: [bool; DREQ_MAX],
}

#[derive(Clone)]
pub struct Dma {
    state: Rc<RefCell<DmaState>>,
    board: Weak<RefCell<BoardShared>>,
}

impl Dma {
    pub fn new(board: Weak<RefCell<BoardShared>>) -> Self {
        let state = Rc::new(RefCell::new(DmaState {
            channels: Vec::new(),
            int_raw: 0,
            int_enable0: 0,
            int_force0: 0,
            int_enable1: 0,
            int_force1: 0,
            timers: [0; 4],
            dreq: [false; DREQ_MAX],
        }));
        let clock = board
            .upgrade()
            .expect("DMA requires a live board")
            .borrow()
            .clock
            .clone();
        let mut channels = Vec::with_capacity(12);
        for index in 0..12u32 {
            let weak_state = Rc::downgrade(&state);
            let weak_board = board.clone();
            let alarm = clock.create_alarm(Box::new(move || {
                if let (Some(state), Some(board)) = (weak_state.upgrade(), weak_board.upgrade()) {
                    Dma {
                        state,
                        board: Rc::downgrade(&board),
                    }
                    .transfer(index);
                }
            }));
            channels.push(Channel {
                ctrl: 0,
                read_addr: 0,
                write_addr: 0,
                trans_count: 0,
                dreq_counter: 0,
                trans_count_reload: 0,
                treq_value: 0,
                data_size: 1,
                chain_to: 0,
                ring_mask: 0,
                transfer_alarm: alarm,
            });
        }
        state.borrow_mut().channels = channels;
        let dma = Self { state, board };
        dma.reset();
        dma
    }

    pub fn set_dreq(&self, channel: DreqChannel) {
        let mut state = self.state.borrow_mut();
        let index = channel as usize;
        let treq = channel as u32;
        if !state.dreq[index] {
            state.dreq[index] = true;
            let active = state
                .channels
                .iter()
                .enumerate()
                .filter(|(_, channel)| channel.treq_value == treq && channel.ctrl & BUSY != 0)
                .map(|(index, _)| index as u32)
                .collect::<Vec<_>>();
            drop(state);
            for index in active {
                self.schedule(index);
            }
        }
    }

    pub fn clear_dreq(&self, channel: DreqChannel) {
        self.state.borrow_mut().dreq[channel as usize] = false;
    }

    fn start(&self, index: u32) {
        let count = {
            let mut state = self.state.borrow_mut();
            let Some(channel) = state.channels.get_mut(index as usize) else {
                return;
            };
            if channel.ctrl & EN == 0 || channel.ctrl & BUSY != 0 {
                return;
            }
            channel.ctrl |= BUSY;
            channel.trans_count = channel.trans_count_reload;
            channel.trans_count
        };
        if count > 0 {
            self.schedule(index);
        }
    }

    fn abort(&self, index: u32) {
        if let Some(channel) = self.state.borrow_mut().channels.get_mut(index as usize) {
            channel.ctrl &= !BUSY;
            channel.transfer_alarm.cancel();
        }
    }

    fn schedule(&self, index: u32) {
        let (treq, dreq, delay) = {
            let state = self.state.borrow();
            let Some(channel) = state.channels.get(index as usize) else {
                return;
            };
            let treq = channel.treq_value;
            (
                treq,
                state.dreq.get(treq as usize).copied().unwrap_or(false),
                self.timer(treq),
            )
        };
        if dreq || treq == TREQ_PERMANENT {
            if let Some(channel) = self.state.borrow_mut().channels.get_mut(index as usize) {
                channel.transfer_alarm.schedule(0.0);
            }
        } else if delay > 0.0 {
            if let Some(channel) = self.state.borrow_mut().channels.get_mut(index as usize) {
                channel.transfer_alarm.schedule(delay * 1000.0);
            }
        }
    }

    fn timer(&self, treq: u32) -> f64 {
        let (timer_value, divisor) = {
            let state = self.state.borrow();
            match treq {
                TREQ_PERMANENT => (1, 1),
                TREQ_TIMER0 | TREQ_TIMER1 | TREQ_TIMER2 | TREQ_TIMER3 => {
                    let timer = state.timers[(treq - TREQ_TIMER0) as usize];
                    (timer >> 16, timer & 0xffff)
                }
                _ => (0, 1),
            }
        };
        if divisor == 0 {
            return 0.0;
        }
        let clk_sys = self
            .board
            .upgrade()
            .map(|board| board.borrow().clk_sys)
            .unwrap_or(0.0);
        if clk_sys == 0.0 {
            return 0.0;
        }
        ((timer_value as f64 / divisor as f64) * 1e6) / clk_sys
    }

    fn transfer(&self, index: u32) {
        let (ctrl, data_size, read_addr, write_addr, ring_mask) = {
            let state = self.state.borrow();
            let Some(channel) = state.channels.get(index as usize) else {
                return;
            };
            (
                channel.ctrl,
                channel.data_size,
                channel.read_addr,
                channel.write_addr,
                channel.ring_mask,
            )
        };
        if ctrl & EN == 0 {
            return;
        }
        let board = self.board.upgrade();
        let Some(board) = board else {
            return;
        };
        let bus = crate::rp2040::Bus(board.clone());
        match data_size {
            2 => {
                let value = bus.read_u16(read_addr);
                let value = if ctrl & BSWAP != 0 {
                    value.swap_bytes()
                } else {
                    value
                };
                bus.write_u16(write_addr, value);
            }
            4 => {
                let value = bus.read_u32(read_addr);
                let value = if ctrl & BSWAP != 0 {
                    value.swap_bytes()
                } else {
                    value
                };
                bus.write_u32(write_addr, value);
            }
            _ => bus.write_u8(write_addr, bus.read_u8(read_addr)),
        }
        let (done, chain_to) = {
            let mut state = self.state.borrow_mut();
            let (done, chain_to) = {
                let channel = &mut state.channels[index as usize];
                if ctrl & INCR_READ != 0 {
                    if ring_mask != 0 && ctrl & RING_SEL == 0 {
                        channel.read_addr = (channel.read_addr & !ring_mask)
                            | ((channel.read_addr + data_size) & ring_mask);
                    } else {
                        channel.read_addr = channel.read_addr.wrapping_add(data_size);
                    }
                }
                if ctrl & INCR_WRITE != 0 {
                    if ring_mask != 0 && ctrl & RING_SEL != 0 {
                        channel.write_addr = (channel.write_addr & !ring_mask)
                            | ((channel.write_addr + data_size) & ring_mask);
                    } else {
                        channel.write_addr = channel.write_addr.wrapping_add(data_size);
                    }
                }
                channel.trans_count = channel.trans_count.saturating_sub(1);
                let done = channel.trans_count == 0;
                if done {
                    channel.ctrl &= !BUSY;
                }
                (done, channel.chain_to)
            };
            if done && ctrl & IRQ_QUIET == 0 {
                state.int_raw |= 1 << index;
            }
            (done, chain_to)
        };
        if done {
            self.check_interrupts();
            if chain_to != index {
                self.start(chain_to);
            }
        } else {
            self.schedule(index);
        }
    }

    fn check_interrupts(&self) {
        let (status0, status1) = {
            let state = self.state.borrow();
            (
                (state.int_raw & state.int_enable0) | state.int_force0,
                (state.int_raw & state.int_enable1) | state.int_force1,
            )
        };
        if let Some(board) = self.board.upgrade() {
            let core = board.borrow().core.clone();
            core.set_interrupt(Irq::DmaIrq0 as u32, status0 != 0);
            core.set_interrupt(Irq::DmaIrq1 as u32, status1 != 0);
        }
    }

    fn channel_read(&self, index: u32, offset: u32) -> u32 {
        let state = self.state.borrow();
        let Some(channel) = state.channels.get(index as usize) else {
            return 0;
        };
        match offset {
            CH_READ_ADDR | CH_AL1_READ_ADDR | CH_AL2_READ_ADDR | CH_AL3_READ_ADDR_TRIG => {
                channel.read_addr
            }
            CH_WRITE_ADDR | CH_AL1_WRITE_ADDR | CH_AL2_WRITE_ADDR_TRIG | CH_AL3_WRITE_ADDR => {
                channel.write_addr
            }
            CH_TRANS_COUNT | CH_AL1_TRANS_COUNT_TRIG | CH_AL2_TRANS_COUNT | CH_AL3_TRANS_COUNT => {
                channel.trans_count
            }
            CH_CTRL_TRIG | CH_AL1_CTRL | CH_AL2_CTRL | CH_AL3_CTRL => channel.ctrl,
            CH_DBG_CTDREQ => channel.dreq_counter,
            CH_DBG_TCR => channel.trans_count_reload,
            _ => 0,
        }
    }

    fn channel_write(&self, index: u32, offset: u32, value: u32) {
        let trigger = matches!(
            offset,
            CH_AL3_READ_ADDR_TRIG | CH_AL2_WRITE_ADDR_TRIG | CH_AL1_TRANS_COUNT_TRIG | CH_CTRL_TRIG
        );
        let ctrl_write = matches!(
            offset,
            CH_CTRL_TRIG | CH_AL1_CTRL | CH_AL2_CTRL | CH_AL3_CTRL
        );
        {
            let mut state = self.state.borrow_mut();
            let Some(channel) = state.channels.get_mut(index as usize) else {
                return;
            };
            match offset {
                CH_READ_ADDR | CH_AL1_READ_ADDR | CH_AL2_READ_ADDR | CH_AL3_READ_ADDR_TRIG => {
                    channel.read_addr = value;
                }
                CH_WRITE_ADDR | CH_AL1_WRITE_ADDR | CH_AL2_WRITE_ADDR_TRIG | CH_AL3_WRITE_ADDR => {
                    channel.write_addr = value;
                }
                CH_TRANS_COUNT
                | CH_AL1_TRANS_COUNT_TRIG
                | CH_AL2_TRANS_COUNT
                | CH_AL3_TRANS_COUNT => {
                    channel.trans_count_reload = value;
                }
                CH_CTRL_TRIG | CH_AL1_CTRL | CH_AL2_CTRL | CH_AL3_CTRL => {
                    channel.ctrl = (channel.ctrl & !CTRL_WRITE_MASK) | (value & CTRL_WRITE_MASK);
                    channel.ctrl &= !(value & CTRL_WRITE_CLEAR_MASK);
                    channel.treq_value = (channel.ctrl >> TREQ_SEL_SHIFT) & TREQ_SEL_MASK;
                    channel.chain_to = (channel.ctrl >> CHAIN_TO_SHIFT) & CHAIN_TO_MASK;
                    let ring_size = (channel.ctrl >> RING_SIZE_SHIFT) & RING_SIZE_MASK;
                    channel.ring_mask = if ring_size != 0 {
                        (1u32 << ring_size) - 1
                    } else {
                        0
                    };
                    channel.data_size = match (channel.ctrl >> DATA_SIZE_SHIFT) & DATA_SIZE_MASK {
                        1 => 2,
                        2 => 4,
                        _ => 1,
                    };
                }
                CH_DBG_CTDREQ => channel.dreq_counter = 0,
                _ => {}
            }
        }
        if trigger {
            if value != 0 {
                self.start(index);
            } else {
                let irq_quiet = self
                    .state
                    .borrow()
                    .channels
                    .get(index as usize)
                    .map(|channel| channel.ctrl & IRQ_QUIET != 0)
                    .unwrap_or(false);
                if irq_quiet {
                    self.state.borrow_mut().int_raw |= 1 << index;
                    self.check_interrupts();
                }
            }
        }
        if ctrl_write {
            let (enabled, busy) = {
                let state = self.state.borrow();
                let channel = &state.channels[index as usize];
                (channel.ctrl & EN != 0, channel.ctrl & BUSY != 0)
            };
            if enabled && busy {
                self.schedule(index);
            } else if !enabled {
                self.abort(index);
            }
        }
    }
}

impl Peripheral for Dma {
    fn name(&self) -> &str {
        "DMA"
    }

    fn read_u32(&self, offset: u32) -> u32 {
        if (offset & 0x7ff) < CHANNEL_REGISTERS_SIZE {
            let index = (offset & 0x7ff) >> 6;
            if index >= 12 {
                return 0;
            }
            return self.channel_read(index, offset & CHANNEL_REGISTERS_MASK);
        }
        let state = self.state.borrow();
        match offset {
            TIMER0 => state.timers[0],
            TIMER1 => state.timers[1],
            TIMER2 => state.timers[2],
            TIMER3 => state.timers[3],
            INTR => state.int_raw,
            INTE0 => state.int_enable0,
            INTF0 => state.int_force0,
            INTS0 => (state.int_raw & state.int_enable0) | state.int_force0,
            INTE1 => state.int_enable1,
            INTF1 => state.int_force1,
            INTS1 => (state.int_raw & state.int_enable1) | state.int_force1,
            N_CHANNELS => 12,
            _ => 0xffff_ffff,
        }
    }

    fn write_u32_atomic(&self, offset: u32, value: u32, atomic_type: u32) {
        if (offset & 0x7ff) < CHANNEL_REGISTERS_SIZE {
            let index = (offset & 0x7ff) >> 6;
            if index >= 12 {
                return;
            }
            let old = self.channel_read(index, offset & CHANNEL_REGISTERS_MASK);
            let value = if atomic_type == 0 {
                value
            } else {
                atomic_update(old, atomic_type, value)
            };
            self.channel_write(index, offset & CHANNEL_REGISTERS_MASK, value);
            return;
        }
        let old = self.read_u32(offset);
        let atomic_value = if atomic_type == 0 {
            value
        } else {
            atomic_update(old, atomic_type, value)
        };
        match offset {
            TIMER0 | TIMER1 | TIMER2 | TIMER3 => {
                self.state.borrow_mut().timers[(offset - TIMER0) as usize] = atomic_value;
            }
            INTR | INTS0 | INTS1 => {
                self.state.borrow_mut().int_raw &= !value;
                self.check_interrupts();
            }
            INTE0 => {
                self.state.borrow_mut().int_enable0 = atomic_value & 0xffff;
                self.check_interrupts();
            }
            INTF0 => {
                self.state.borrow_mut().int_force0 = atomic_value & 0xffff;
                self.check_interrupts();
            }
            INTE1 => {
                self.state.borrow_mut().int_enable1 = atomic_value & 0xffff;
                self.check_interrupts();
            }
            INTF1 => {
                self.state.borrow_mut().int_force1 = atomic_value & 0xffff;
                self.check_interrupts();
            }
            MULTI_CHAN_TRIGGER => {
                for index in 0..12 {
                    if value & (1 << index) != 0 {
                        self.start(index);
                    }
                }
            }
            CHAN_ABORT => {
                for index in 0..12 {
                    if value & (1 << index) != 0 {
                        self.abort(index);
                    }
                }
            }
            _ => {}
        }
    }

    fn reset(&self) {
        let mut state = self.state.borrow_mut();
        state.int_raw = 0;
        state.int_enable0 = 0;
        state.int_force0 = 0;
        state.int_enable1 = 0;
        state.int_force1 = 0;
        state.timers = [0; 4];
        state.dreq = [false; DREQ_MAX];
        for (index, channel) in state.channels.iter_mut().enumerate() {
            channel.ctrl = (index as u32) << CHAIN_TO_SHIFT;
            channel.read_addr = 0;
            channel.write_addr = 0;
            channel.trans_count = 0;
            channel.dreq_counter = 0;
            channel.trans_count_reload = 0;
            channel.treq_value = 0;
            channel.data_size = 1;
            channel.chain_to = index as u32;
            channel.ring_mask = 0;
            channel.transfer_alarm.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use crate::clock::MockClock;
    use crate::rp2040::Rp2040;

    const CH2_WRITE_ADDR: u32 = 0x5000_0084;
    const CH2_TRANS_COUNT: u32 = 0x5000_0088;
    const CH2_AL1_CTRL: u32 = 0x5000_0090;
    const CH2_AL3_READ_ADDR_TRIG: u32 = 0x5000_00bc;
    const CH6_READ_ADDR: u32 = 0x5000_0180;
    const CH6_WRITE_ADDR: u32 = 0x5000_0184;
    const CH6_TRANS_COUNT: u32 = 0x5000_0188;
    const CH6_CTRL_TRIG: u32 = 0x5000_018c;
    const INTR: u32 = 0x5000_0400;
    const PAST_CHANNELS: u32 = 0x5000_0300;

    const EN: u32 = 1;
    const DATA_SIZE_SHIFT: u32 = 2;
    const INCR_WRITE: u32 = 1 << 5;
    const INCR_READ: u32 = 1 << 4;
    const CHAIN_TO_SHIFT: u32 = 11;
    const TREQ_SEL_SHIFT: u32 = 15;
    const BUSY: u32 = 1 << 24;
    const TREQ_PERMANENT: u32 = 0x3f;

    #[test]
    fn should_support_dma_channel_chaining() {
        let clock = Rc::new(MockClock::new());
        let rp = Rp2040::with_clock(clock.clone());

        let chunks = [0x2000_1000, 0x2000_1100, 0x2000_2200, 0x2000_2300];
        let dest = 0x2000_8000;
        let control_block = 0x2000_a000;

        for (i, chunk) in chunks.iter().enumerate() {
            rp.write_u32(*chunk, 0x10 + i as u32 * 0x20);
            rp.write_u32(*chunk + 4, 0x20 + i as u32 * 0x20);
            rp.write_u32(control_block + i as u32 * 4, *chunk);
        }
        rp.write_u32(control_block + 16, 0);

        rp.write_u32(CH2_WRITE_ADDR, dest);
        rp.write_u32(CH2_TRANS_COUNT, 2);
        rp.write_u32(
            CH2_AL1_CTRL,
            EN | (6 << CHAIN_TO_SHIFT)
                | INCR_WRITE
                | INCR_READ
                | (TREQ_PERMANENT << TREQ_SEL_SHIFT)
                | (2 << DATA_SIZE_SHIFT),
        );

        rp.write_u32(CH6_WRITE_ADDR, CH2_AL3_READ_ADDR_TRIG);
        rp.write_u32(CH6_READ_ADDR, control_block);
        rp.write_u32(CH6_TRANS_COUNT, 1);
        rp.write_u32(
            CH6_CTRL_TRIG,
            EN | INCR_READ | (TREQ_PERMANENT << TREQ_SEL_SHIFT) | (2 << DATA_SIZE_SHIFT),
        );

        assert_eq!(rp.read_u32(CH6_CTRL_TRIG) & BUSY, BUSY);
        clock.advance(32);

        assert_eq!(rp.read_u32(CH2_AL3_READ_ADDR_TRIG), 0);
        assert_eq!(rp.read_u32(CH2_AL1_CTRL) & BUSY, 0);
        assert_eq!(rp.read_u32(CH6_CTRL_TRIG) & BUSY, 0);
        assert_eq!(rp.read_u32(INTR), (1 << 2) | (1 << 6));
        for (i, expected) in [0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80]
            .iter()
            .enumerate()
        {
            assert_eq!(rp.read_u16(dest + i as u32 * 4), *expected);
        }
        assert_eq!(rp.read_u16(dest + 32), 0);
    }

    #[test]
    fn should_not_dispatch_past_channel_registers() {
        let clock = Rc::new(MockClock::new());
        let rp = Rp2040::with_clock(clock);
        let _ = rp.read_u32(PAST_CHANNELS);
        rp.write_u32(PAST_CHANNELS, 0x1234);
    }
}
