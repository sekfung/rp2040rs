use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::clock::Alarm;
use crate::gpio_pin::{SOURCE_PIO0, SOURCE_PIO1};
use crate::peripherals::dma::DreqChannel;
use crate::peripherals::{atomic_update, Peripheral};
use crate::rp2040::BoardShared;
use crate::utils::Fifo;

const SM0_CLKDIV: u32 = 0x0c8;
const CTRL: u32 = 0x000;
const FSTAT: u32 = 0x004;
const FDEBUG: u32 = 0x008;
const FLEVEL: u32 = 0x00c;
const TXF0: u32 = 0x010;
const RXF0: u32 = 0x020;
const IRQ: u32 = 0x030;
const IRQ_FORCE: u32 = 0x034;
const INPUT_SYNC_BYPASS: u32 = 0x038;
const DBG_PADOUT: u32 = 0x03c;
const DBG_PADOE: u32 = 0x040;
const DBG_CFGINFO: u32 = 0x044;
const INSTR_MEM0: u32 = 0x048;
const INSTR_MEM31: u32 = 0x0c4;

const INTR: u32 = 0x128;
const IRQ0_INTE: u32 = 0x12c;
const IRQ0_INTF: u32 = 0x130;
const IRQ0_INTS: u32 = 0x134;
const IRQ1_INTE: u32 = 0x138;
const IRQ1_INTF: u32 = 0x13c;
const IRQ1_INTS: u32 = 0x140;

const FSTAT_TXEMPTY: u32 = 1 << 24;
const FSTAT_TXFULL: u32 = 1 << 16;
const FSTAT_RXEMPTY: u32 = 1 << 8;
const FSTAT_RXFULL: u32 = 1 << 0;

const FDEBUG_TXSTALL: u32 = 1 << 24;
const FDEBUG_RXSTALL: u32 = 1 << 0;
const FDEBUG_TXOVER: u32 = 1 << 16;
const FDEBUG_RXUNDER: u32 = 1 << 8;

const SHIFTCTRL_AUTOPUSH: u32 = 1 << 16;
const SHIFTCTRL_AUTOPULL: u32 = 1 << 17;
const SHIFTCTRL_IN_SHIFTDIR: u32 = 1 << 18;
const SHIFTCTRL_OUT_SHIFTDIR: u32 = 1 << 19;

const EXECCTRL_STATUS_SEL: u32 = 1 << 4;
const EXECCTRL_SIDE_PINDIR: u32 = 1 << 29;
const EXECCTRL_SIDE_EN: u32 = 1 << 30;
const EXECCTRL_EXEC_STALLED: u32 = 1 << 31;

const VALID_PINS_MASK: u32 = 0x3fff_ffff;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WaitType {
    None,
    Pin,
    RxFifo,
    TxFifo,
    Irq,
    Out,
}

#[derive(Clone, Debug)]
/// One PIO state machine. Exported for API parity with upstream `StateMachine`.
pub struct StateMachine {
    enabled: bool,
    x: u32,
    y: u32,
    pc: u32,
    input_shift_reg: u32,
    input_shift_count: u32,
    output_shift_reg: u32,
    output_shift_count: u32,
    cycles: u32,
    exec_opcode: u16,
    exec_valid: bool,
    update_pc: bool,
    clock_div_int: u32,
    clock_div_frac: u32,
    exec_ctrl: u32,
    shift_ctrl: u32,
    pin_ctrl: u32,
    rx_fifo: Fifo,
    tx_fifo: Fifo,
    out_pin_values: u32,
    out_pin_direction: u32,
    waiting: bool,
    wait_type: WaitType,
    wait_index: u32,
    wait_polarity: bool,
    wait_delay: i32,
}

impl StateMachine {
    fn new() -> Self {
        Self {
            enabled: false,
            x: 0,
            y: 0,
            pc: 0,
            input_shift_reg: 0,
            input_shift_count: 0,
            output_shift_reg: 0,
            output_shift_count: 0,
            cycles: 0,
            exec_opcode: 0,
            exec_valid: false,
            update_pc: true,
            clock_div_int: 1,
            clock_div_frac: 0,
            exec_ctrl: 0x1f << 12,
            shift_ctrl: 0b11 << 18,
            pin_ctrl: 0x5 << 26,
            rx_fifo: Fifo::new(4),
            tx_fifo: Fifo::new(4),
            out_pin_values: 0,
            out_pin_direction: 0,
            waiting: false,
            wait_type: WaitType::None,
            wait_index: 0,
            wait_polarity: false,
            wait_delay: -1,
        }
    }

    fn restart(&mut self) {
        self.cycles = 0;
        self.input_shift_count = 0;
        self.output_shift_count = 32;
        self.input_shift_reg = 0;
        self.waiting = false;
        self.exec_ctrl &= !EXECCTRL_EXEC_STALLED;
    }
}

struct PioState {
    instructions: [u16; 32],
    machines: [StateMachine; 4],
    fdebug: u32,
    tx_stall: u32,
    rx_stall: u32,
    input_sync_bypass: u32,
    irq: u32,
    pin_values: u32,
    pin_directions: u32,
    old_pin_values: u32,
    old_pin_directions: u32,
    irq0_int_enable: u32,
    irq0_int_force: u32,
    irq1_int_enable: u32,
    irq1_int_force: u32,
    running: bool,
}

impl PioState {
    fn new() -> Self {
        Self {
            instructions: [0; 32],
            machines: [
                StateMachine::new(),
                StateMachine::new(),
                StateMachine::new(),
                StateMachine::new(),
            ],
            fdebug: 0,
            tx_stall: 0,
            rx_stall: 0,
            input_sync_bypass: 0,
            irq: 0,
            pin_values: 0,
            pin_directions: 0,
            old_pin_values: 0,
            old_pin_directions: 0,
            irq0_int_enable: 0,
            irq0_int_force: 0,
            irq1_int_enable: 0,
            irq1_int_force: 0,
            running: false,
        }
    }

    fn int_raw(&self) -> u32 {
        let mut value = (self.irq & 0xf) << 8;
        for (index, machine) in self.machines.iter().enumerate() {
            let shift = index as u32;
            if !machine.tx_fifo.full() {
                value |= 0x10 << shift;
            }
            if !machine.rx_fifo.empty() {
                value |= 1 << shift;
            }
        }
        value
    }
}

#[derive(Clone)]
pub struct Pio {
    state: Rc<RefCell<PioState>>,
    board: Weak<RefCell<BoardShared>>,
    name: &'static str,
    first_irq: u32,
    index: usize,
    alarm: Rc<RefCell<Option<Alarm>>>,
}

impl Pio {
    pub fn new(
        name: &'static str,
        first_irq: u32,
        index: usize,
        board: Weak<RefCell<BoardShared>>,
    ) -> Self {
        let state = Rc::new(RefCell::new(PioState::new()));
        let alarm = Rc::new(RefCell::new(None));
        if let Some(board_rc) = board.upgrade() {
            let clock = board_rc.borrow().clock.clone();
            let weak_state = Rc::downgrade(&state);
            let weak_board = board.clone();
            let alarm_slot = alarm.clone();
            let alarm_handle = clock.create_alarm(Box::new(move || {
                if let (Some(state), Some(board_rc)) = (weak_state.upgrade(), weak_board.upgrade())
                {
                    let pio = Pio {
                        state,
                        board: Rc::downgrade(&board_rc),
                        name,
                        first_irq,
                        index,
                        alarm: alarm_slot.clone(),
                    };
                    pio.run_burst();
                }
            }));
            *alarm.borrow_mut() = Some(alarm_handle);
        }
        Self {
            state,
            board,
            name,
            first_irq,
            index,
            alarm,
        }
    }

    fn board(&self) -> Option<Rc<RefCell<BoardShared>>> {
        self.board.upgrade()
    }

    fn gpio_values(&self) -> u32 {
        let Some(board) = self.board() else {
            return 0;
        };
        let board = board.borrow();
        let mut value = 0;
        for (index, pin) in board.gpio.iter().enumerate() {
            let pin_value = if pin.raw_output_enable() {
                pin.raw_output_value()
            } else {
                pin.input_value()
            };
            if pin_value {
                value |= 1 << index;
            }
        }
        value
    }

    fn gpio_input(&self, pin: u32) -> bool {
        let Some(board) = self.board() else {
            return false;
        };
        let borrowed = board.borrow();
        borrowed.gpio.get(pin as usize).is_some_and(|pin| {
            if pin.raw_output_enable() {
                pin.raw_output_value()
            } else {
                pin.input_value()
            }
        })
    }

    fn update_interrupts(&self, state: &PioState) {
        let status0 = (state.int_raw() & state.irq0_int_enable) | state.irq0_int_force;
        let status1 = (state.int_raw() & state.irq1_int_enable) | state.irq1_int_force;
        if let Some(board) = self.board() {
            let board = board.borrow();
            board.core.set_interrupt(self.first_irq, status0 != 0);
            board.core.set_interrupt(self.first_irq + 1, status1 != 0);
        }
        self.update_dreqs(state);
    }

    fn update_dreqs(&self, state: &PioState) {
        let Some(board) = self.board() else {
            return;
        };
        let dma = board.borrow().dma.clone();
        let Some(dma) = dma else {
            return;
        };
        for index in 0..4 {
            let machine = &state.machines[index];
            let tx_channel = match (self.index, index) {
                (0, 0) => DreqChannel::Pio0Tx0,
                (0, 1) => DreqChannel::Pio0Tx1,
                (0, 2) => DreqChannel::Pio0Tx2,
                (0, 3) => DreqChannel::Pio0Tx3,
                (1, 0) => DreqChannel::Pio1Tx0,
                (1, 1) => DreqChannel::Pio1Tx1,
                (1, 2) => DreqChannel::Pio1Tx2,
                _ => DreqChannel::Pio1Tx3,
            };
            let rx_channel = match (self.index, index) {
                (0, 0) => DreqChannel::Pio0Rx0,
                (0, 1) => DreqChannel::Pio0Rx1,
                (0, 2) => DreqChannel::Pio0Rx2,
                (0, 3) => DreqChannel::Pio0Rx3,
                (1, 0) => DreqChannel::Pio1Rx0,
                (1, 1) => DreqChannel::Pio1Rx1,
                (1, 2) => DreqChannel::Pio1Rx2,
                _ => DreqChannel::Pio1Rx3,
            };
            if machine.tx_fifo.full() {
                dma.clear_dreq(tx_channel);
            } else {
                dma.set_dreq(tx_channel);
            }
            if machine.rx_fifo.empty() {
                dma.clear_dreq(rx_channel);
            } else {
                dma.set_dreq(rx_channel);
            }
        }
    }

    fn check_changed_pins(&self) {
        let (changed, values, directions) = {
            let mut state = self.state.borrow_mut();
            let changed = (state.old_pin_directions ^ state.pin_directions)
                | (state.old_pin_values ^ state.pin_values);
            if changed != 0 {
                state.old_pin_directions = state.pin_directions;
                state.old_pin_values = state.pin_values;
            }
            (changed, state.pin_values, state.pin_directions)
        };
        if changed == 0 {
            return;
        }
        if let Some(board) = self.board() {
            let board = board.borrow();
            for index in 0..30 {
                let bit = 1u32 << index;
                if changed & bit != 0 && directions & bit != 0 {
                    board.gpio[index].set_source_output(
                        if self.index == 0 {
                            SOURCE_PIO0
                        } else {
                            SOURCE_PIO1
                        },
                        values & bit != 0,
                        true,
                    );
                } else if changed & bit != 0 {
                    board.gpio[index].clear_source_output(if self.index == 0 {
                        SOURCE_PIO0
                    } else {
                        SOURCE_PIO1
                    });
                }
            }
        }
    }

    fn irq_index(irq: u32, machine: usize) -> u32 {
        let rel = irq & 0x10 != 0;
        if rel {
            (irq & 0x4) | (((irq & 0x3) + machine as u32) & 0x3)
        } else {
            irq & 0x7
        }
    }

    fn bit_reverse(mut value: u32) -> u32 {
        value = ((value & 0x5555_5555) << 1) | ((value & 0xaaaa_aaaa) >> 1);
        value = ((value & 0x3333_3333) << 2) | ((value & 0xcccc_cccc) >> 2);
        value = ((value & 0x0f0f_0f0f) << 4) | ((value & 0xf0f0_f0f0) >> 4);
        value = ((value & 0x00ff_00ff) << 8) | ((value & 0xff00_ff00) >> 8);
        value.rotate_right(16)
    }

    fn shl(value: u32, count: u32) -> u32 {
        if count >= 32 {
            0
        } else {
            value << count
        }
    }

    fn mask_for(count: u32) -> u32 {
        if count >= 32 {
            u32::MAX
        } else {
            (1u32 << count) - 1
        }
    }

    fn status(state: &PioState, index: usize) -> u32 {
        let machine = &state.machines[index];
        let status_n = machine.exec_ctrl & 0xf;
        let count = if machine.exec_ctrl & EXECCTRL_STATUS_SEL != 0 {
            machine.rx_fifo.item_count() as u32
        } else {
            machine.tx_fifo.item_count() as u32
        };
        if count < status_n {
            u32::MAX
        } else {
            0
        }
    }

    fn in_pins(&self, machine: &StateMachine) -> u32 {
        let values = self.gpio_values();
        let base = (machine.pin_ctrl >> 15) & 0x1f;
        if base != 0 {
            values.rotate_right(base)
        } else {
            values
        }
    }

    fn in_source(&self, state: &PioState, index: usize, source: u32) -> u32 {
        let machine = &state.machines[index];
        match source {
            0 => self.in_pins(machine),
            1 => machine.x,
            2 => machine.y,
            3 | 4 => 0,
            5 => Self::status(state, index),
            6 => machine.input_shift_reg,
            7 => machine.output_shift_reg,
            _ => 0,
        }
    }

    fn set_out_pins(state: &mut PioState, index: usize, value: u32, base: u32, count: u32) {
        let mask = Self::mask_for(count).wrapping_shl(base) & VALID_PINS_MASK;
        state.pin_values = (state.pin_values & !mask) | (value.wrapping_shl(base) & mask);
        state.machines[index].out_pin_values = value;
    }

    fn set_out_dirs(state: &mut PioState, index: usize, value: u32, base: u32, count: u32) {
        let mask = Self::mask_for(count).wrapping_shl(base) & VALID_PINS_MASK;
        state.pin_directions = (state.pin_directions & !mask) | (value.wrapping_shl(base) & mask);
        state.machines[index].out_pin_direction = value;
    }

    fn write_out_value(
        &self,
        state: &mut PioState,
        index: usize,
        destination: u32,
        value: u32,
        bit_count: u32,
    ) {
        let machine = &mut state.machines[index];
        match destination {
            0 => {
                let base = machine.pin_ctrl & 0x1f;
                let count = (machine.pin_ctrl >> 20) & 0x3f;
                Self::set_out_pins(state, index, value, base, count);
            }
            1 => state.machines[index].x = value,
            2 => state.machines[index].y = value,
            3 => {}
            4 => {
                let base = machine.pin_ctrl & 0x1f;
                let count = (machine.pin_ctrl >> 20) & 0x3f;
                Self::set_out_dirs(state, index, value, base, count);
            }
            5 => {
                state.machines[index].pc = value & 0x1f;
                state.machines[index].update_pc = false;
            }
            6 => {
                state.machines[index].input_shift_reg = value;
                state.machines[index].input_shift_count = bit_count;
            }
            7 => {
                state.machines[index].exec_opcode = value as u16;
                state.machines[index].exec_valid = true;
            }
            _ => {}
        }
    }

    fn out_instruction(&self, state: &mut PioState, index: usize, arg: u32) {
        let bit_count = arg & 0x1f;
        let destination = arg >> 5;
        if bit_count == 0 {
            let value = state.machines[index].output_shift_reg;
            self.write_out_value(state, index, destination, value, 32);
            state.machines[index].output_shift_count = 32;
            return;
        }
        let mask = Self::mask_for(bit_count);
        let shift_out = state.machines[index].shift_ctrl & SHIFTCTRL_OUT_SHIFTDIR != 0;
        let (value, osr) = if shift_out {
            let value = state.machines[index].output_shift_reg & mask;
            (value, state.machines[index].output_shift_reg >> bit_count)
        } else {
            let value = state.machines[index].output_shift_reg >> (32 - bit_count);
            (value, state.machines[index].output_shift_reg << bit_count)
        };
        state.machines[index].output_shift_reg = osr;
        self.write_out_value(state, index, destination, value, bit_count);
        state.machines[index].output_shift_count =
            (state.machines[index].output_shift_count + bit_count).min(32);
    }

    fn transform_mov(value: u32, op: u32) -> u32 {
        match op {
            0 => value,
            1 => !value,
            2 => Self::bit_reverse(value),
            _ => value,
        }
    }

    fn set_mov_destination(state: &mut PioState, index: usize, destination: u32, value: u32) {
        match destination {
            0 => {
                let base = state.machines[index].out_base();
                let count = state.machines[index].out_count();
                Self::set_out_pins(state, index, value, base, count);
            }
            1 => state.machines[index].x = value,
            2 => state.machines[index].y = value,
            3 => {}
            4 => {
                state.machines[index].exec_opcode = value as u16;
                state.machines[index].exec_valid = true;
            }
            5 => {
                state.machines[index].pc = value & 0x1f;
                state.machines[index].update_pc = false;
            }
            6 => {
                state.machines[index].input_shift_reg = value;
                state.machines[index].input_shift_count = 0;
            }
            7 => {
                state.machines[index].output_shift_reg = value;
                state.machines[index].output_shift_count = 0;
            }
            _ => {}
        }
    }

    fn wait(
        state: &mut PioState,
        index: usize,
        wait_type: WaitType,
        polarity: bool,
        wait_index: u32,
    ) {
        let machine = &mut state.machines[index];
        machine.waiting = true;
        machine.wait_type = wait_type;
        machine.wait_polarity = polarity;
        machine.wait_index = wait_index;
        machine.wait_delay = -1;
        machine.update_pc = false;
    }

    fn check_wait(&self, state: &mut PioState, index: usize) {
        if !state.machines[index].waiting {
            return;
        }
        let wait_type = state.machines[index].wait_type;
        let wait_index = state.machines[index].wait_index;
        let polarity = state.machines[index].wait_polarity;
        match wait_type {
            WaitType::Irq => {
                let irq_value = state.irq & (1u32 << wait_index) != 0;
                if irq_value == polarity {
                    state.machines[index].waiting = false;
                    if irq_value {
                        state.irq &= !(1u32 << wait_index);
                    }
                }
            }
            WaitType::Pin => {
                if wait_index < 32 && self.gpio_input(wait_index) == polarity {
                    state.machines[index].waiting = false;
                }
            }
            WaitType::RxFifo => {
                if !state.machines[index].rx_fifo.full() {
                    state.machines[index].rx_fifo.push(wait_index);
                    state.machines[index].waiting = false;
                    self.update_interrupts(state);
                }
            }
            WaitType::TxFifo => {
                if !state.machines[index].tx_fifo.empty() {
                    state.machines[index].output_shift_reg = state.machines[index].tx_fifo.pull();
                    state.machines[index].waiting = false;
                    self.update_interrupts(state);
                }
            }
            WaitType::Out => {
                if !state.machines[index].tx_fifo.empty() {
                    let value = state.machines[index].tx_fifo.pull();
                    state.machines[index].output_shift_reg = value;
                    self.out_instruction(state, index, wait_index);
                    state.machines[index].waiting = false;
                    self.update_interrupts(state);
                }
            }
            WaitType::None => {}
        }
        if !state.machines[index].waiting {
            let wait_delay = state.machines[index].wait_delay;
            state.machines[index].next_pc();
            state.machines[index].cycles = state.machines[index]
                .cycles
                .wrapping_add(wait_delay.max(0) as u32);
            state.machines[index].exec_ctrl &= !EXECCTRL_EXEC_STALLED;
        }
    }

    fn execute_instruction(&self, state: &mut PioState, index: usize, opcode: u16) {
        let opcode = opcode as u32;
        let arg = opcode & 0xff;
        match opcode >> 13 {
            0b000 => {
                if self.jmp_condition(state, index, arg >> 5) {
                    state.machines[index].pc = arg & 0x1f;
                    state.machines[index].update_pc = false;
                }
            }
            0b001 => {
                let polarity = arg & 0x80 != 0;
                let source = (arg >> 5) & 0x3;
                let wait_index = arg & 0x1f;
                match source {
                    0 => Self::wait(state, index, WaitType::Pin, polarity, wait_index),
                    1 => Self::wait(
                        state,
                        index,
                        WaitType::Pin,
                        polarity,
                        (wait_index + state.machines[index].in_base()) % 32,
                    ),
                    2 => Self::wait(
                        state,
                        index,
                        WaitType::Irq,
                        polarity,
                        Self::irq_index(wait_index, index),
                    ),
                    _ => {}
                }
            }
            0b010 => {
                let bit_count = arg & 0x1f;
                let source_value = self.in_source(state, index, arg >> 5);
                if bit_count == 0 {
                    state.machines[index].input_shift_reg = source_value;
                    state.machines[index].input_shift_count = 32;
                } else {
                    let value = source_value & Self::mask_for(bit_count);
                    let shift_in = state.machines[index].shift_ctrl & SHIFTCTRL_IN_SHIFTDIR != 0;
                    if shift_in {
                        state.machines[index].input_shift_reg >>= bit_count;
                        state.machines[index].input_shift_reg |= Self::shl(value, 32 - bit_count);
                    } else {
                        state.machines[index].input_shift_reg =
                            Self::shl(state.machines[index].input_shift_reg, bit_count) | value;
                    }
                    state.machines[index].input_shift_count =
                        (state.machines[index].input_shift_count + bit_count).min(32);
                }
                let autopush = state.machines[index].shift_ctrl & SHIFTCTRL_AUTOPUSH != 0;
                if autopush
                    && state.machines[index].input_shift_count
                        >= state.machines[index].push_threshold()
                {
                    if !state.machines[index].rx_fifo.full() {
                        let value = state.machines[index].input_shift_reg;
                        state.machines[index].rx_fifo.push(value);
                        self.update_interrupts(state);
                    } else {
                        state.rx_stall |= FDEBUG_RXSTALL << index;
                        state.fdebug |= state.rx_stall;
                        let value = state.machines[index].input_shift_reg;
                        Self::wait(state, index, WaitType::RxFifo, false, value);
                    }
                    state.machines[index].input_shift_count = 0;
                    state.machines[index].input_shift_reg = 0;
                }
            }
            0b011 => {
                let autopull = state.machines[index].shift_ctrl & SHIFTCTRL_AUTOPULL != 0;
                if autopull
                    && state.machines[index].output_shift_count
                        >= state.machines[index].pull_threshold()
                {
                    state.machines[index].output_shift_count = 0;
                    if !state.machines[index].tx_fifo.empty() {
                        let value = state.machines[index].tx_fifo.pull();
                        state.machines[index].output_shift_reg = value;
                        self.update_interrupts(state);
                    } else {
                        state.tx_stall |= FDEBUG_TXSTALL << index;
                        state.fdebug |= state.tx_stall;
                        Self::wait(state, index, WaitType::Out, false, arg);
                    }
                }
                if !state.machines[index].waiting {
                    self.out_instruction(state, index, arg);
                }
            }
            0b100 => {
                let block = arg & (1 << 5) != 0;
                let if_full_or_empty = arg & (1 << 6) != 0;
                if arg & 0x1f != 0 {
                    return;
                }
                if arg & 0x80 != 0 {
                    let threshold = state.machines[index].pull_threshold();
                    if if_full_or_empty
                        && state.machines[index].shift_ctrl & SHIFTCTRL_AUTOPULL != 0
                        && state.machines[index].output_shift_count < threshold
                    {
                        return;
                    }
                    if !state.machines[index].tx_fifo.empty() {
                        let value = state.machines[index].tx_fifo.pull();
                        state.machines[index].output_shift_reg = value;
                        self.update_interrupts(state);
                    } else {
                        state.tx_stall |= FDEBUG_TXSTALL << index;
                        state.fdebug |= state.tx_stall;
                        if block {
                            Self::wait(state, index, WaitType::TxFifo, false, 0);
                        } else {
                            state.machines[index].output_shift_reg = state.machines[index].x;
                        }
                    }
                    state.machines[index].output_shift_count = 0;
                } else {
                    let threshold = state.machines[index].push_threshold();
                    if if_full_or_empty
                        && state.machines[index].shift_ctrl & SHIFTCTRL_AUTOPUSH != 0
                        && state.machines[index].input_shift_count < threshold
                    {
                        return;
                    }
                    if !state.machines[index].rx_fifo.full() {
                        let value = state.machines[index].input_shift_reg;
                        state.machines[index].rx_fifo.push(value);
                        self.update_interrupts(state);
                    } else {
                        state.rx_stall |= FDEBUG_RXSTALL << index;
                        state.fdebug |= state.rx_stall;
                        if block {
                            let value = state.machines[index].input_shift_reg;
                            Self::wait(state, index, WaitType::RxFifo, false, value);
                        }
                    }
                    state.machines[index].input_shift_reg = 0;
                    state.machines[index].input_shift_count = 0;
                }
            }
            0b101 => {
                let source = arg & 0x7;
                let op = (arg >> 3) & 0x3;
                let destination = (arg >> 5) & 0x7;
                let value = Self::transform_mov(self.in_source(state, index, source), op);
                Self::set_mov_destination(state, index, destination, value);
            }
            0b110 => {
                if arg & 0x80 != 0 {
                    return;
                }
                let clear = arg & 0x40 != 0;
                let wait = arg & 0x20 != 0;
                let irq = Self::irq_index(arg & 0x1f, index);
                if clear {
                    state.irq &= !(1u32 << irq);
                    self.irq_updated(state);
                } else {
                    state.irq |= 1u32 << irq;
                    self.irq_updated(state);
                    if wait {
                        Self::wait(state, index, WaitType::Irq, false, irq);
                    }
                }
            }
            0b111 => {
                let data = arg & 0x1f;
                match arg >> 5 {
                    0 => {
                        let base = state.machines[index].set_base();
                        let count = state.machines[index].set_count();
                        Self::set_out_pins(state, index, data, base, count);
                    }
                    1 => state.machines[index].x = data,
                    2 => state.machines[index].y = data,
                    4 => {
                        let base = state.machines[index].set_base();
                        let count = state.machines[index].set_count();
                        Self::set_out_dirs(state, index, data, base, count);
                    }
                    _ => {}
                }
            }
            _ => {}
        }

        state.machines[index].cycles = state.machines[index].cycles.wrapping_add(1);
        let sideset_count = state.machines[index].sideset_count();
        let delay_sideset = (opcode >> 8) & 0x1f;
        let side_en = state.machines[index].exec_ctrl & EXECCTRL_SIDE_EN != 0;
        let side_field_bits = 5u32.saturating_sub(sideset_count);
        let delay = delay_sideset & Self::mask_for(side_field_bits);
        if sideset_count > 0 && (!side_en || delay_sideset & 0x10 != 0) {
            let sideset = delay_sideset >> side_field_bits;
            let base = state.machines[index].sideset_base();
            let count = if side_en {
                sideset_count - 1
            } else {
                sideset_count
            };
            if state.machines[index].exec_ctrl & EXECCTRL_SIDE_PINDIR != 0 {
                Self::set_out_dirs(state, index, sideset, base, count);
            } else {
                Self::set_out_pins(state, index, sideset, base, count);
            }
        }
        if state.machines[index].exec_valid {
            state.machines[index].exec_valid = false;
            let opcode = state.machines[index].exec_opcode;
            self.execute_instruction(state, index, opcode);
        } else if state.machines[index].waiting {
            if state.machines[index].wait_delay < 0 {
                state.machines[index].wait_delay = delay as i32;
            }
            self.check_wait(state, index);
        } else {
            state.machines[index].cycles = state.machines[index].cycles.wrapping_add(delay);
        }
    }

    fn jmp_condition(&self, state: &mut PioState, index: usize, condition: u32) -> bool {
        let machine = &state.machines[index];
        match condition {
            0b000 => true,
            0b001 => machine.x == 0,
            0b010 => {
                let old = machine.x;
                state.machines[index].x = state.machines[index].x.wrapping_sub(1);
                old != 0
            }
            0b011 => machine.y == 0,
            0b100 => {
                let old = machine.y;
                state.machines[index].y = state.machines[index].y.wrapping_sub(1);
                old != 0
            }
            0b101 => machine.x != machine.y,
            0b110 => self.gpio_input(machine.jmp_pin()),
            0b111 => machine.output_shift_count < machine.pull_threshold(),
            _ => false,
        }
    }

    fn irq_updated(&self, state: &mut PioState) {
        for index in 0..4 {
            self.check_wait(state, index);
        }
        self.update_interrupts(state);
    }

    fn read_fifo(&self, state: &mut PioState, index: usize) -> u32 {
        if state.machines[index].shift_ctrl & (1 << 30) != 0 {
            if state.machines[index].tx_fifo.empty() {
                state.fdebug |= FDEBUG_RXUNDER << index;
                return 0;
            }
            let value = state.machines[index].tx_fifo.pull();
            self.update_interrupts(state);
            return value;
        }
        if state.machines[index].rx_fifo.empty() {
            state.fdebug |= FDEBUG_RXUNDER << index;
            return 0;
        }
        let value = state.machines[index].rx_fifo.pull();
        state.rx_stall &= !(FDEBUG_RXSTALL << index);
        self.check_wait(state, index);
        if state.machines[index].rx_fifo.empty() {
            self.update_interrupts(state);
        }
        value
    }

    fn write_fifo(&self, state: &mut PioState, index: usize, value: u32) {
        if state.machines[index].tx_fifo.full() {
            state.fdebug |= FDEBUG_TXOVER << index;
            return;
        }
        state.machines[index].tx_fifo.push(value);
        state.tx_stall &= !(FDEBUG_TXSTALL << index);
        self.check_wait(state, index);
        if state.machines[index].tx_fifo.full() {
            self.update_interrupts(state);
        }
    }

    fn fifo_stat(state: &PioState, index: usize) -> u32 {
        let machine = &state.machines[index];
        let value = (if machine.tx_fifo.empty() {
            FSTAT_TXEMPTY
        } else {
            0
        }) | (if machine.tx_fifo.full() {
            FSTAT_TXFULL
        } else {
            0
        }) | (if machine.rx_fifo.empty() {
            FSTAT_RXEMPTY
        } else {
            0
        }) | (if machine.rx_fifo.full() {
            FSTAT_RXFULL
        } else {
            0
        });
        value << index
    }

    fn read_sm_register(state: &PioState, index: usize, offset: u32) -> u32 {
        let machine = &state.machines[index];
        match offset {
            0x00 => (machine.clock_div_int << 16) | (machine.clock_div_frac << 8),
            0x04 => machine.exec_ctrl,
            0x08 => machine.shift_ctrl,
            0x0c => machine.pc,
            0x10 => state.instructions[machine.pc as usize] as u32,
            0x14 => machine.pin_ctrl,
            _ => 0xffff_ffff,
        }
    }

    fn write_sm_register(&self, state: &mut PioState, index: usize, offset: u32, value: u32) {
        match offset {
            0x00 => {
                state.machines[index].clock_div_frac = (value >> 8) & 0xff;
                state.machines[index].clock_div_int = value >> 16;
            }
            0x04 => {
                state.machines[index].exec_ctrl =
                    (value & 0x7fff_ffff) | (state.machines[index].exec_ctrl & 0x8000_0000);
            }
            0x08 => state.machines[index].shift_ctrl = value,
            0x0c => {}
            0x10 => {
                self.execute_instruction(state, index, value as u16);
                if state.machines[index].waiting {
                    state.machines[index].exec_ctrl |= EXECCTRL_EXEC_STALLED;
                }
            }
            0x14 => state.machines[index].pin_ctrl = value,
            _ => {}
        }
    }

    pub fn step(&self) {
        {
            let mut state = self.state.borrow_mut();
            for index in 0..4 {
                if state.machines[index].enabled {
                    if state.machines[index].waiting {
                        self.check_wait(&mut state, index);
                    }
                    if !state.machines[index].waiting {
                        state.machines[index].update_pc = true;
                        let opcode = state.instructions[state.machines[index].pc as usize];
                        self.execute_instruction(&mut state, index, opcode);
                        if state.machines[index].update_pc {
                            state.machines[index].next_pc();
                        }
                    }
                }
            }
        }
        self.check_changed_pins();
    }

    fn burst_delay_nanos(&self) -> f64 {
        let clk_sys = self
            .board()
            .map(|board| board.borrow().clk_sys)
            .unwrap_or(125_000_000.0);
        let divisor = {
            let state = self.state.borrow();
            state
                .machines
                .iter()
                .filter(|machine| machine.enabled)
                .map(|machine| {
                    let int = if machine.clock_div_int == 0 {
                        65536.0
                    } else {
                        f64::from(machine.clock_div_int)
                    };
                    int + f64::from(machine.clock_div_frac) / 256.0
                })
                .fold(f64::MAX, f64::min)
        };
        let divisor = if divisor.is_finite() { divisor } else { 1.0 };
        if clk_sys > 0.0 {
            1000.0 * 1e9 * divisor / clk_sys
        } else {
            8000.0
        }
    }

    pub fn reset(&self) {
        self.stop();
        {
            let mut state = self.state.borrow_mut();
            state.instructions = [0; 32];
            state.fdebug = 0;
            state.tx_stall = 0;
            state.rx_stall = 0;
            state.irq = 0;
            state.pin_values = 0;
            state.pin_directions = 0;
            state.old_pin_values = 0;
            state.old_pin_directions = 0;
            state.irq0_int_enable = 0;
            state.irq0_int_force = 0;
            state.irq1_int_enable = 0;
            state.irq1_int_force = 0;
            state.machines = [
                StateMachine::new(),
                StateMachine::new(),
                StateMachine::new(),
                StateMachine::new(),
            ];
        }
        if let Some(board) = self.board() {
            let board = board.borrow();
            board.core.set_interrupt(self.first_irq, false);
            board.core.set_interrupt(self.first_irq + 1, false);
            for pin in &board.gpio {
                pin.clear_source_output(if self.index == 0 {
                    SOURCE_PIO0
                } else {
                    SOURCE_PIO1
                });
            }
        }
    }

    fn run_burst(&self) {
        for _ in 0..1000 {
            self.step();
        }
        {
            let state = self.state.borrow();
            self.update_dreqs(&state);
        }
        let running = self
            .state
            .borrow()
            .machines
            .iter()
            .any(|machine| machine.enabled);
        self.state.borrow_mut().running = running;
        if running {
            if let Some(alarm) = self.alarm.borrow_mut().as_mut() {
                alarm.schedule(self.burst_delay_nanos());
            }
        }
    }

    pub fn stop(&self) {
        let mut state = self.state.borrow_mut();
        for machine in &mut state.machines {
            machine.enabled = false;
        }
        state.running = false;
        drop(state);
        if let Some(alarm) = self.alarm.borrow_mut().as_mut() {
            alarm.cancel();
        }
    }

    pub fn index(&self) -> usize {
        self.index
    }

    fn read_u32_impl(&self, offset: u32) -> u32 {
        let state = self.state.borrow();
        for index in 0..4 {
            let base = SM0_CLKDIV + index as u32 * 0x18;
            if offset >= base && offset <= base + 0x14 {
                return Self::read_sm_register(&state, index, offset - base);
            }
        }
        match offset {
            CTRL => (0..4).fold(0, |value, index| {
                value
                    | if state.machines[index].enabled {
                        1 << index
                    } else {
                        0
                    }
            }),
            FSTAT => (0..4).fold(0, |value, index| value | Self::fifo_stat(&state, index)),
            FDEBUG => state.fdebug,
            FLEVEL => (0..4).fold(0, |value, index| {
                value
                    | ((state.machines[index].tx_fifo.item_count() as u32 & 0xf) << (index * 8))
                    | ((state.machines[index].rx_fifo.item_count() as u32 & 0xf) << (index * 8 + 4))
            }),
            RXF0..=0x02c => {
                let index = ((offset - RXF0) >> 2) as usize;
                drop(state);
                self.read_fifo(&mut self.state.borrow_mut(), index)
            }
            IRQ => state.irq,
            IRQ_FORCE => 0,
            INPUT_SYNC_BYPASS => state.input_sync_bypass,
            DBG_PADOUT => state.pin_values,
            DBG_PADOE => state.pin_directions,
            DBG_CFGINFO => 0x20_0404,
            INTR => state.int_raw(),
            IRQ0_INTE => state.irq0_int_enable,
            IRQ0_INTF => state.irq0_int_force,
            IRQ0_INTS => (state.int_raw() & state.irq0_int_enable) | state.irq0_int_force,
            IRQ1_INTE => state.irq1_int_enable,
            IRQ1_INTF => state.irq1_int_force,
            IRQ1_INTS => (state.int_raw() & state.irq1_int_enable) | state.irq1_int_force,
            _ => 0xffff_ffff,
        }
    }

    fn write_u32_impl(&self, offset: u32, value: u32) {
        if (INSTR_MEM0..=INSTR_MEM31).contains(&offset) {
            let index = ((offset - INSTR_MEM0) >> 2) as usize;
            self.state.borrow_mut().instructions[index] = value as u16;
            return;
        }
        for index in 0..4 {
            let base = SM0_CLKDIV + index as u32 * 0x18;
            if offset >= base && offset <= base + 0x14 {
                let mut state = self.state.borrow_mut();
                self.write_sm_register(&mut state, index, offset - base, value);
                return;
            }
        }
        let mut state = self.state.borrow_mut();
        match offset {
            CTRL => {
                let was_running = state.machines.iter().any(|machine| machine.enabled);
                let should_run = value & 0xf != 0;
                for index in 0..4 {
                    state.machines[index].enabled = value & (1 << index) != 0;
                    if value & (1 << (4 + index)) != 0 {
                        state.machines[index].restart();
                    }
                }
                let now_running = state.machines.iter().any(|machine| machine.enabled);
                state.running = now_running;
                drop(state);
                if now_running && !was_running && should_run {
                    self.run_burst();
                } else if !now_running {
                    if let Some(alarm) = self.alarm.borrow_mut().as_mut() {
                        alarm.cancel();
                    }
                }
            }
            FDEBUG => {
                state.fdebug &= !value;
                state.fdebug |= state.tx_stall | state.rx_stall;
            }
            TXF0..=0x01c => {
                let index = ((offset - TXF0) >> 2) as usize;
                drop(state);
                self.write_fifo(&mut self.state.borrow_mut(), index, value);
            }
            IRQ => {
                state.irq &= !value;
                self.irq_updated(&mut state);
            }
            INPUT_SYNC_BYPASS => state.input_sync_bypass = value,
            IRQ_FORCE => {
                state.irq |= value;
                self.irq_updated(&mut state);
            }
            IRQ0_INTE => {
                state.irq0_int_enable = value & 0xfff;
                self.update_interrupts(&state);
            }
            IRQ0_INTF => {
                state.irq0_int_force = value & 0xfff;
                self.update_interrupts(&state);
            }
            IRQ1_INTE => {
                state.irq1_int_enable = value & 0xfff;
                self.update_interrupts(&state);
            }
            IRQ1_INTF => {
                state.irq1_int_force = value & 0xfff;
                self.update_interrupts(&state);
            }
            _ => {}
        }
    }
}

impl StateMachine {
    fn push_threshold(&self) -> u32 {
        let value = (self.shift_ctrl >> 20) & 0x1f;
        if value == 0 {
            32
        } else {
            value
        }
    }

    fn pull_threshold(&self) -> u32 {
        let value = (self.shift_ctrl >> 25) & 0x1f;
        if value == 0 {
            32
        } else {
            value
        }
    }

    fn sideset_count(&self) -> u32 {
        (self.pin_ctrl >> 29) & 0x7
    }
    fn set_count(&self) -> u32 {
        (self.pin_ctrl >> 26) & 0x7
    }
    fn out_count(&self) -> u32 {
        (self.pin_ctrl >> 20) & 0x3f
    }
    fn in_base(&self) -> u32 {
        (self.pin_ctrl >> 15) & 0x1f
    }
    fn sideset_base(&self) -> u32 {
        (self.pin_ctrl >> 10) & 0x1f
    }
    fn set_base(&self) -> u32 {
        (self.pin_ctrl >> 5) & 0x1f
    }
    fn out_base(&self) -> u32 {
        self.pin_ctrl & 0x1f
    }
    fn jmp_pin(&self) -> u32 {
        (self.exec_ctrl >> 24) & 0x1f
    }
    fn wrap_top(&self) -> u32 {
        (self.exec_ctrl >> 12) & 0x1f
    }
    fn wrap_bottom(&self) -> u32 {
        (self.exec_ctrl >> 7) & 0x1f
    }

    fn next_pc(&mut self) {
        if self.pc == self.wrap_top() {
            self.pc = self.wrap_bottom();
        } else {
            self.pc = (self.pc + 1) & 0x1f;
        }
    }
}

impl Peripheral for Pio {
    fn name(&self) -> &'static str {
        self.name
    }

    fn read_u32(&self, offset: u32) -> u32 {
        self.read_u32_impl(offset)
    }

    fn write_u32_atomic(&self, offset: u32, value: u32, atomic_type: u32) {
        let merged = if atomic_type != 0 {
            atomic_update(self.read_u32_impl(offset), atomic_type, value)
        } else {
            value
        };
        // FDEBUG/IRQ use raw write-one-to-clear semantics.
        let write = if matches!(offset, FDEBUG | IRQ) {
            value
        } else {
            merged
        };
        self.write_u32_impl(offset, write);
    }

    fn reset(&self) {
        *self.state.borrow_mut() = PioState::new();
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::TestDriver;
    use crate::utils::pio_assembler::*;

    const CTRL: u32 = 0x5020_0000;
    const FLEVEL: u32 = 0x5020_000c;
    const FDEBUG: u32 = 0x5020_0008;
    const TXF0: u32 = 0x5020_0010;
    const RXF0: u32 = 0x5020_0020;
    const INSTR_MEM0: u32 = 0x5020_0048;
    const INSTR_MEM1: u32 = 0x5020_004c;
    const INSTR_MEM2: u32 = 0x5020_0050;
    const INSTR_MEM3: u32 = 0x5020_0054;
    const IRQ: u32 = 0x5020_0030;
    const DBG_PADOUT: u32 = 0x5020_003c;
    const SM0_EXECCTRL: u32 = 0x5020_00cc;
    const SM0_SHIFTCTRL: u32 = 0x5020_00d0;
    const SM0_ADDR: u32 = 0x5020_00d4;
    const SM0_INSTR: u32 = 0x5020_00d8;
    const SM0_PINCTRL: u32 = 0x5020_00dc;
    const SM2_INSTR: u32 = 0x5020_0108;
    const INTR: u32 = 0x5020_0128;
    const IRQ0_INTE: u32 = 0x5020_012c;
    const NVIC_ISPR: u32 = 0xe000_e200;
    const NVIC_ICPR: u32 = 0xe000_e280;
    const PIO_IRQ0: u32 = 1 << 7;
    const INTR_SM0_TXNFULL: u32 = 1 << 4;
    const INTR_SM0_RXNEMPTY: u32 = 1 << 0;
    const TX0_SHIFT: u32 = 0;
    const RX0_SHIFT: u32 = 4;
    const FJOIN_RX: u32 = 1 << 30;
    const IN_SHIFTDIR: u32 = 1 << 18;
    const OUT_SHIFTDIR: u32 = 1 << 19;
    const EXECCTRL_STATUS_SEL: u32 = 1 << 4;
    const EXECCTRL_WRAP_BOTTOM_SHIFT: u32 = 7;
    const EXECCTRL_WRAP_TOP_SHIFT: u32 = 12;
    const EXECCTRL_EXEC_STALLED: u32 = 1 << 31;
    const SHIFTCTRL_AUTOPULL: u32 = 1 << 17;
    const SHIFTCTRL_AUTOPUSH: u32 = 1 << 16;
    const SHIFTCTRL_PULL_THRESH_SHIFT: u32 = 25;
    const SHIFTCTRL_PUSH_THRESH_SHIFT: u32 = 20;
    const FDEBUG_TXSTALL: u32 = 1 << 24;
    const SET_COUNT_SHIFT: u32 = 26;
    const SET_COUNT_BASE: u32 = 5;
    const OUT_COUNT_SHIFT: u32 = 20;
    const EXECCTRL_STATUS_N_SHIFT: u32 = 0;
    const VALID_PINS_MASK: u32 = 0x3fff_ffff;

    fn reset_state_machines(cpu: &TestDriver) {
        cpu.write_u32(CTRL, 0xf0);
        cpu.write_u32(SM0_INSTR, pio_jmp(PIO_COND_ALWAYS, 0, 0) as u32);
        cpu.write_u32(SM0_SHIFTCTRL, FJOIN_RX);
        cpu.write_u32(SM0_SHIFTCTRL, IN_SHIFTDIR | OUT_SHIFTDIR);
        cpu.write_u32(SM0_PINCTRL, 5 << SET_COUNT_SHIFT);
    }

    #[test]
    fn should_execute_a_set_pins_instruction_correctly() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(SM0_PINCTRL, (5 << SET_COUNT_SHIFT) | (0 << SET_COUNT_BASE));
        cpu.write_u32(SM0_INSTR, pio_set(PIO_DEST_PINS, 13, 0) as u32);
        assert_eq!(cpu.read_u32(DBG_PADOUT) & 0x1f, 13);
    }

    #[test]
    fn should_execute_a_mov_pins_x_instruction_correctly() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(SM0_PINCTRL, 32 << OUT_COUNT_SHIFT);
        cpu.write_u32(SM0_INSTR, pio_set(PIO_DEST_X, 8, 0) as u32);
        cpu.write_u32(
            SM0_INSTR,
            pio_mov(PIO_DEST_PINS, PIO_OP_NONE, PIO_SRC_X, 0) as u32,
        );
        assert_eq!(cpu.read_u32(DBG_PADOUT), 8);
    }

    #[test]
    fn should_execute_a_mov_pins_invert_x_instruction() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(SM0_PINCTRL, 32 << OUT_COUNT_SHIFT);
        cpu.write_u32(SM0_INSTR, pio_set(PIO_DEST_X, 29, 0) as u32);
        cpu.write_u32(
            SM0_INSTR,
            pio_mov(PIO_DEST_PINS, PIO_OP_INVERT, PIO_SRC_X, 0) as u32,
        );
        assert_eq!(cpu.read_u32(DBG_PADOUT), (!29) & VALID_PINS_MASK);
    }

    #[test]
    fn should_correctly_execute_mov_pins_bitreverse_x() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(SM0_PINCTRL, 32 << OUT_COUNT_SHIFT);
        cpu.write_u32(SM0_INSTR, pio_set(PIO_DEST_X, 0b11001, 0) as u32);
        cpu.write_u32(
            SM0_INSTR,
            pio_mov(PIO_DEST_PINS, PIO_OP_BITREV, PIO_SRC_X, 0) as u32,
        );
        assert_eq!(cpu.read_u32(DBG_PADOUT), 0x9800_0000 & VALID_PINS_MASK);
    }

    #[test]
    fn should_correctly_execute_mov_y_x() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(SM0_PINCTRL, 32 << OUT_COUNT_SHIFT);
        cpu.write_u32(SM0_INSTR, pio_set(PIO_DEST_X, 11, 0) as u32);
        cpu.write_u32(
            SM0_INSTR,
            pio_mov(PIO_MOV_DEST_Y, PIO_OP_NONE, PIO_SRC_X, 0) as u32,
        );
        cpu.write_u32(
            SM0_INSTR,
            pio_mov(PIO_DEST_PINS, PIO_OP_NONE, PIO_SRC_Y, 0) as u32,
        );
        assert_eq!(cpu.read_u32(DBG_PADOUT), 11);
    }

    #[test]
    fn should_correctly_execute_mov_pc_y() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        assert_eq!(cpu.read_u32(SM0_ADDR), 0);
        cpu.write_u32(SM0_INSTR, pio_set(PIO_DEST_Y, 23, 0) as u32);
        cpu.write_u32(
            SM0_INSTR,
            pio_mov(PIO_MOV_DEST_PC, PIO_OP_NONE, PIO_SRC_Y, 0) as u32,
        );
        assert_eq!(cpu.read_u32(SM0_ADDR), 23);
    }

    #[test]
    fn should_execute_mov_isr_status_with_status_sel_0() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(SM0_EXECCTRL, 2);
        cpu.write_u32(
            SM0_INSTR,
            pio_mov(PIO_MOV_DEST_ISR, PIO_OP_NONE, PIO_SRC_STATUS, 0) as u32,
        );
        cpu.write_u32(SM0_INSTR, pio_push(false, false, 0) as u32);
        assert_eq!(cpu.read_u32(RXF0), 0xffff_ffff);
        cpu.write_u32(TXF0, 1);
        cpu.write_u32(TXF0, 2);
        cpu.write_u32(
            SM0_INSTR,
            pio_mov(PIO_MOV_DEST_ISR, PIO_OP_NONE, PIO_SRC_STATUS, 0) as u32,
        );
        cpu.write_u32(SM0_INSTR, pio_push(false, false, 0) as u32);
        assert_eq!(cpu.read_u32(RXF0), 0);
    }

    #[test]
    fn should_execute_mov_isr_status_with_status_sel_1() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(
            SM0_EXECCTRL,
            (1 << EXECCTRL_STATUS_N_SHIFT) | EXECCTRL_STATUS_SEL,
        );
        cpu.write_u32(
            SM0_INSTR,
            pio_mov(PIO_MOV_DEST_ISR, PIO_OP_NONE, PIO_SRC_STATUS, 0) as u32,
        );
        cpu.write_u32(SM0_INSTR, pio_push(false, false, 0) as u32);
        cpu.write_u32(
            SM0_INSTR,
            pio_mov(PIO_MOV_DEST_ISR, PIO_OP_NONE, PIO_SRC_STATUS, 0) as u32,
        );
        cpu.write_u32(SM0_INSTR, pio_push(false, false, 0) as u32);
        assert_eq!(cpu.read_u32(RXF0), 0xffff_ffff);
        assert_eq!(cpu.read_u32(RXF0), 0);
    }

    #[test]
    fn should_execute_jmp_always() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(SM0_INSTR, pio_jmp(PIO_COND_ALWAYS, 10, 0) as u32);
        assert_eq!(cpu.read_u32(SM0_ADDR), 10);
    }

    #[test]
    fn should_execute_jmp_not_x() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(SM0_INSTR, pio_set(PIO_DEST_X, 5, 0) as u32);
        cpu.write_u32(SM0_INSTR, pio_jmp(PIO_COND_NOTX, 8, 0) as u32);
        assert_eq!(cpu.read_u32(SM0_ADDR), 0);
        cpu.write_u32(SM0_INSTR, pio_set(PIO_DEST_X, 0, 0) as u32);
        cpu.write_u32(SM0_INSTR, pio_jmp(PIO_COND_NOTX, 8, 0) as u32);
        assert_eq!(cpu.read_u32(SM0_ADDR), 8);
    }

    #[test]
    fn should_execute_jmp_x_decrement() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(SM0_PINCTRL, 32 << OUT_COUNT_SHIFT);
        cpu.write_u32(SM0_INSTR, pio_set(PIO_DEST_X, 5, 0) as u32);
        cpu.write_u32(SM0_INSTR, pio_jmp(PIO_COND_XDEC, 12, 0) as u32);
        assert_eq!(cpu.read_u32(SM0_ADDR), 12);
        cpu.write_u32(
            SM0_INSTR,
            pio_mov(PIO_DEST_PINS, PIO_OP_NONE, PIO_SRC_X, 0) as u32,
        );
        assert_eq!(cpu.read_u32(DBG_PADOUT), 4);
        cpu.write_u32(SM0_INSTR, pio_set(PIO_DEST_X, 0, 0) as u32);
        cpu.write_u32(SM0_INSTR, pio_jmp(PIO_COND_XDEC, 6, 0) as u32);
        assert_eq!(cpu.read_u32(SM0_ADDR), 12);
    }

    #[test]
    fn should_execute_jmp_not_y() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(SM0_INSTR, pio_set(PIO_DEST_Y, 6, 0) as u32);
        cpu.write_u32(SM0_INSTR, pio_jmp(PIO_COND_NOTY, 8, 0) as u32);
        assert_eq!(cpu.read_u32(SM0_ADDR), 0);
        cpu.write_u32(SM0_INSTR, pio_set(PIO_DEST_Y, 0, 0) as u32);
        cpu.write_u32(SM0_INSTR, pio_jmp(PIO_COND_NOTY, 8, 0) as u32);
        assert_eq!(cpu.read_u32(SM0_ADDR), 8);
    }

    #[test]
    fn should_execute_jmp_y_decrement() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(SM0_PINCTRL, 32 << OUT_COUNT_SHIFT);
        cpu.write_u32(SM0_INSTR, pio_set(PIO_DEST_Y, 15, 0) as u32);
        cpu.write_u32(SM0_INSTR, pio_jmp(PIO_COND_YDEC, 12, 0) as u32);
        assert_eq!(cpu.read_u32(SM0_ADDR), 12);
        cpu.write_u32(
            SM0_INSTR,
            pio_mov(PIO_DEST_PINS, PIO_OP_NONE, PIO_SRC_Y, 0) as u32,
        );
        assert_eq!(cpu.read_u32(DBG_PADOUT), 14);
        cpu.write_u32(SM0_INSTR, pio_set(PIO_DEST_Y, 0, 0) as u32);
        cpu.write_u32(SM0_INSTR, pio_jmp(PIO_COND_YDEC, 6, 0) as u32);
        assert_eq!(cpu.read_u32(SM0_ADDR), 12);
    }

    #[test]
    fn should_execute_jmp_x_not_equal_y() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(SM0_PINCTRL, 32 << OUT_COUNT_SHIFT);
        cpu.write_u32(SM0_INSTR, pio_set(PIO_DEST_X, 23, 0) as u32);
        cpu.write_u32(SM0_INSTR, pio_set(PIO_DEST_Y, 23, 0) as u32);
        cpu.write_u32(SM0_INSTR, pio_jmp(PIO_COND_XNEY, 26, 0) as u32);
        assert_eq!(cpu.read_u32(SM0_ADDR), 0);
        cpu.write_u32(SM0_INSTR, pio_set(PIO_DEST_X, 3, 0) as u32);
        cpu.write_u32(SM0_INSTR, pio_jmp(PIO_COND_XNEY, 26, 0) as u32);
        assert_eq!(cpu.read_u32(SM0_ADDR), 26);
    }

    #[test]
    fn should_execute_jmp_osre() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(SM0_PINCTRL, 32 << OUT_COUNT_SHIFT);
        cpu.write_u32(
            SM0_INSTR,
            pio_mov(PIO_MOV_DEST_OSR, PIO_OP_NONE, PIO_SRC_NULL, 0) as u32,
        );
        cpu.write_u32(SM0_INSTR, pio_jmp(PIO_COND_NOTEMPTYOSR, 11, 0) as u32);
        assert_eq!(cpu.read_u32(SM0_ADDR), 11);
        cpu.write_u32(SM0_INSTR, pio_out(PIO_DEST_NULL, 32, 0) as u32);
        cpu.write_u32(SM0_INSTR, pio_jmp(PIO_COND_NOTEMPTYOSR, 22, 0) as u32);
        assert_eq!(cpu.read_u32(SM0_ADDR), 11);
    }

    #[test]
    fn should_execute_a_program_with_pull() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(SM0_PINCTRL, 32 << OUT_COUNT_SHIFT);
        cpu.write_u32(TXF0, 0x42f0_0d43);
        assert_eq!(cpu.read_u32(FLEVEL), 1 << TX0_SHIFT);
        cpu.write_u32(SM0_INSTR, pio_pull(false, false, 0) as u32);
        assert_eq!(cpu.read_u32(FLEVEL), 0);
        cpu.write_u32(SM0_INSTR, pio_out(PIO_DEST_PINS, 32, 0) as u32);
        assert_eq!(cpu.read_u32(DBG_PADOUT), 0x42f0_0d43 & VALID_PINS_MASK);
    }

    #[test]
    fn should_execute_out_exec() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(SM0_PINCTRL, 32 << OUT_COUNT_SHIFT);
        cpu.write_u32(TXF0, pio_jmp(PIO_COND_ALWAYS, 16, 0) as u32);
        cpu.write_u32(SM0_INSTR, pio_pull(false, false, 0) as u32);
        assert_eq!(cpu.read_u32(SM0_ADDR), 0);
        cpu.write_u32(SM0_INSTR, pio_out(PIO_DEST_EXEC, 32, 0) as u32);
        assert_eq!(cpu.read_u32(SM0_ADDR), 16);
    }

    #[test]
    fn should_execute_out_pc() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(SM0_PINCTRL, 32 << OUT_COUNT_SHIFT);
        cpu.write_u32(TXF0, 29);
        cpu.write_u32(SM0_INSTR, pio_pull(false, false, 0) as u32);
        cpu.write_u32(SM0_INSTR, pio_out(PIO_DEST_PC, 32, 0) as u32);
        assert_eq!(cpu.read_u32(SM0_ADDR), 29);
    }

    #[test]
    fn should_execute_a_program_with_push() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        assert_eq!(cpu.read_u32(FLEVEL), 0);
        cpu.write_u32(SM0_INSTR, pio_set(PIO_DEST_X, 9, 0) as u32);
        cpu.write_u32(SM0_INSTR, pio_in(PIO_SRC_X, 32, 0) as u32);
        cpu.write_u32(SM0_INSTR, pio_push(false, false, 0) as u32);
        assert_eq!(cpu.read_u32(FLEVEL), 1 << RX0_SHIFT);
        cpu.write_u32(SM0_INSTR, pio_push(false, false, 0) as u32);
        assert_eq!(cpu.read_u32(FLEVEL), 2 << RX0_SHIFT);
        assert_eq!(cpu.read_u32(RXF0), 9);
        assert_eq!(cpu.read_u32(FLEVEL), 1 << RX0_SHIFT);
        assert_eq!(cpu.read_u32(RXF0), 0);
    }

    #[test]
    fn should_execute_irq_2() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(IRQ, 0xff);
        cpu.write_u32(SM0_INSTR, pio_irq(false, false, 2, 0) as u32);
        assert_eq!(cpu.read_u32(IRQ), 1 << 2);
        cpu.write_u32(SM0_INSTR, pio_irq(true, false, 2, 0) as u32);
        assert_eq!(cpu.read_u32(IRQ), 0);
    }

    #[test]
    fn should_execute_irq_3_relative() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(IRQ, 0xff);
        cpu.write_u32(SM2_INSTR, pio_irq(false, false, 0x13, 0) as u32);
        assert_eq!(cpu.read_u32(IRQ), 1 << 1);
        cpu.write_u32(SM2_INSTR, pio_irq(true, false, 0x13, 0) as u32);
        assert_eq!(cpu.read_u32(IRQ), 0);
        cpu.write_u32(SM0_INSTR, pio_irq(false, false, 0x13, 0) as u32);
        assert_eq!(cpu.read_u32(IRQ), 1 << 3);
    }

    #[test]
    fn should_execute_wait_irq_7() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(IRQ, 0xff);
        cpu.write_u32(
            INSTR_MEM0,
            pio_mov(PIO_MOV_DEST_X, PIO_OP_NONE, PIO_SRC_X, 0) as u32,
        );
        cpu.write_u32(INSTR_MEM1, pio_wait(true, PIO_WAIT_SRC_IRQ, 7, 0) as u32);
        cpu.write_u32(INSTR_MEM2, pio_jmp(PIO_COND_ALWAYS, 2, 0) as u32);
        cpu.write_u32(CTRL, 1);
        assert_eq!(cpu.read_u32(SM0_ADDR), 1);
        cpu.write_u32(SM2_INSTR, pio_irq(false, false, 5, 0) as u32);
        assert_eq!(cpu.read_u32(SM0_ADDR), 1);
        cpu.write_u32(SM2_INSTR, pio_irq(false, false, 7, 0) as u32);
        assert_eq!(cpu.read_u32(SM0_ADDR), 2);
        assert_eq!(cpu.read_u32(IRQ), 1 << 5);
    }

    #[test]
    fn should_execute_wait_0_irq_7() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(IRQ, 0xff);
        cpu.write_u32(SM0_INSTR, pio_irq(false, false, 7, 0) as u32);
        cpu.write_u32(
            INSTR_MEM0,
            pio_mov(PIO_MOV_DEST_X, PIO_OP_NONE, PIO_SRC_X, 0) as u32,
        );
        cpu.write_u32(INSTR_MEM1, pio_wait(false, PIO_WAIT_SRC_IRQ, 7, 0) as u32);
        cpu.write_u32(INSTR_MEM2, pio_jmp(PIO_COND_ALWAYS, 2, 0) as u32);
        cpu.write_u32(CTRL, 1);
        assert_eq!(cpu.read_u32(SM0_ADDR), 1);
        cpu.write_u32(IRQ, 1 << 7);
        assert_eq!(cpu.read_u32(SM0_ADDR), 2);
    }

    #[test]
    fn should_update_intr_after_irq_2() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(IRQ, 0xff);
        cpu.write_u32(SM2_INSTR, pio_irq(false, false, 2, 0) as u32);
        assert_eq!(cpu.read_u32(INTR) & 0xf00, 1 << 10);
    }

    #[test]
    fn should_compare_x_to_ffffffff_after_mov_x_invert_null() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(TXF0, 0xffff_ffff);
        cpu.write_u32(SM0_INSTR, pio_pull(false, false, 0) as u32);
        cpu.write_u32(
            SM0_INSTR,
            pio_mov(PIO_DEST_X, PIO_OP_INVERT, PIO_SRC_NULL, 0) as u32,
        );
        cpu.write_u32(SM0_INSTR, pio_out(PIO_DEST_Y, 32, 0) as u32);
        cpu.write_u32(SM0_INSTR, pio_jmp(PIO_COND_ALWAYS, 8, 0) as u32);
        cpu.write_u32(SM0_INSTR, pio_jmp(PIO_COND_XNEY, 16, 0) as u32);
        assert_eq!(cpu.read_u32(SM0_ADDR), 8);
    }

    #[test]
    fn should_wrap_the_program_when_it_reaches_wrap_top() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(
            SM0_EXECCTRL,
            (1 << EXECCTRL_WRAP_BOTTOM_SHIFT) | (2 << EXECCTRL_WRAP_TOP_SHIFT),
        );
        cpu.write_u32(INSTR_MEM0, pio_jmp(PIO_COND_ALWAYS, 2, 0) as u32);
        cpu.write_u32(INSTR_MEM1, pio_jmp(PIO_COND_ALWAYS, 1, 0) as u32);
        cpu.write_u32(
            INSTR_MEM2,
            pio_mov(PIO_DEST_X, PIO_OP_NONE, PIO_SRC_X, 0) as u32,
        );
        cpu.write_u32(INSTR_MEM3, pio_jmp(PIO_COND_ALWAYS, 3, 0) as u32);
        cpu.write_u32(CTRL, 1);
        assert_eq!(cpu.read_u32(SM0_ADDR), 1);
    }

    #[test]
    fn should_automatically_pull_when_autopull_enabled() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(
            SM0_SHIFTCTRL,
            SHIFTCTRL_AUTOPULL | (4 << SHIFTCTRL_PULL_THRESH_SHIFT) | OUT_SHIFTDIR,
        );
        cpu.write_u32(TXF0, 5);
        cpu.write_u32(TXF0, 6);
        cpu.write_u32(TXF0, 7);
        for _ in 0..3 {
            cpu.write_u32(SM0_INSTR, pio_out(PIO_DEST_X, 4, 0) as u32);
            cpu.write_u32(SM0_INSTR, pio_in(PIO_SRC_X, 4, 0) as u32);
        }
        cpu.write_u32(SM0_INSTR, pio_push(false, false, 0) as u32);
        assert_eq!(cpu.read_u32(RXF0), 0x567);
    }

    #[test]
    fn should_not_autopull_in_the_middle_of_out() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(
            SM0_SHIFTCTRL,
            SHIFTCTRL_AUTOPULL | (4 << SHIFTCTRL_PULL_THRESH_SHIFT) | OUT_SHIFTDIR,
        );
        cpu.write_u32(TXF0, 0x25);
        cpu.write_u32(TXF0, 0x36);
        cpu.write_u32(SM0_INSTR, pio_out(PIO_DEST_X, 8, 0) as u32);
        cpu.write_u32(SM0_INSTR, pio_in(PIO_SRC_X, 8, 0) as u32);
        cpu.write_u32(SM0_INSTR, pio_push(false, false, 0) as u32);
        assert_eq!(cpu.read_u32(RXF0), 0x25);
    }

    #[test]
    fn should_stall_until_tx_fifo_fills_with_autopull() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(
            SM0_SHIFTCTRL,
            SHIFTCTRL_AUTOPULL | (4 << SHIFTCTRL_PULL_THRESH_SHIFT) | OUT_SHIFTDIR,
        );
        cpu.write_u32(SM0_INSTR, pio_out(PIO_DEST_X, 4, 0) as u32);
        assert_eq!(
            cpu.read_u32(SM0_EXECCTRL) & EXECCTRL_EXEC_STALLED,
            EXECCTRL_EXEC_STALLED
        );
        cpu.write_u32(TXF0, 0x36);
        assert_eq!(cpu.read_u32(SM0_EXECCTRL) & EXECCTRL_EXEC_STALLED, 0);
    }

    #[test]
    fn should_automatically_push_when_autopush_enabled() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(
            SM0_SHIFTCTRL,
            SHIFTCTRL_AUTOPUSH | (8 << SHIFTCTRL_PUSH_THRESH_SHIFT) | OUT_SHIFTDIR,
        );
        cpu.write_u32(SM0_INSTR, pio_set(PIO_DEST_X, 0x13, 0) as u32);
        cpu.write_u32(SM0_INSTR, pio_in(PIO_SRC_X, 8, 0) as u32);
        assert_eq!(cpu.read_u32(RXF0), 0x13);
    }

    #[test]
    fn should_only_autopush_at_the_end_of_in() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(
            SM0_SHIFTCTRL,
            SHIFTCTRL_AUTOPUSH | (8 << SHIFTCTRL_PUSH_THRESH_SHIFT) | OUT_SHIFTDIR,
        );
        cpu.write_u32(
            SM0_INSTR,
            pio_mov(PIO_DEST_X, PIO_OP_INVERT, PIO_SRC_NULL, 0) as u32,
        );
        cpu.write_u32(SM0_INSTR, pio_in(PIO_SRC_X, 16, 0) as u32);
        assert_eq!(cpu.read_u32(RXF0), 0xffff);
    }

    #[test]
    fn should_stall_until_rx_fifo_has_capacity_with_autopush() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(
            SM0_SHIFTCTRL,
            SHIFTCTRL_AUTOPUSH | (8 << SHIFTCTRL_PUSH_THRESH_SHIFT) | OUT_SHIFTDIR,
        );
        for value in [15, 16, 17, 18, 19] {
            cpu.write_u32(SM0_INSTR, pio_set(PIO_DEST_X, value, 0) as u32);
            cpu.write_u32(SM0_INSTR, pio_in(PIO_SRC_X, 8, 0) as u32);
        }
        assert_eq!(
            cpu.read_u32(SM0_EXECCTRL) & EXECCTRL_EXEC_STALLED,
            EXECCTRL_EXEC_STALLED
        );
        assert_eq!(cpu.read_u16(RXF0), 15);
        assert_eq!(cpu.read_u32(SM0_EXECCTRL) & EXECCTRL_EXEC_STALLED, 0);
        assert_eq!(cpu.read_u16(RXF0), 16);
        assert_eq!(cpu.read_u16(RXF0), 17);
        assert_eq!(cpu.read_u16(RXF0), 18);
        assert_eq!(cpu.read_u16(RXF0), 19);
    }

    #[test]
    fn should_update_txnfull_flag_in_intr() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(IRQ0_INTE, INTR_SM0_TXNFULL);
        assert_eq!(cpu.read_u32(INTR) & INTR_SM0_TXNFULL, INTR_SM0_TXNFULL);
        assert_eq!(cpu.read_u32(NVIC_ISPR) & PIO_IRQ0, PIO_IRQ0);
        cpu.write_u32(TXF0, 1);
        cpu.write_u32(TXF0, 2);
        cpu.write_u32(TXF0, 3);
        cpu.write_u32(NVIC_ICPR, PIO_IRQ0);
        assert_eq!(cpu.read_u32(INTR) & INTR_SM0_TXNFULL, INTR_SM0_TXNFULL);
        cpu.write_u32(TXF0, 3);
        cpu.write_u32(NVIC_ICPR, PIO_IRQ0);
        assert_eq!(cpu.read_u32(INTR) & INTR_SM0_TXNFULL, 0);
        assert_eq!(cpu.read_u32(NVIC_ISPR) & PIO_IRQ0, 0);
        cpu.write_u32(SM0_INSTR, pio_pull(false, false, 0) as u32);
        assert_eq!(cpu.read_u32(INTR) & INTR_SM0_TXNFULL, INTR_SM0_TXNFULL);
        assert_eq!(cpu.read_u32(NVIC_ISPR) & PIO_IRQ0, PIO_IRQ0);
    }

    #[test]
    fn should_set_txstall_flag_and_only_clear_when_unstalled() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(FDEBUG, 0xffff_ffff);
        assert_eq!(cpu.read_u32(FDEBUG), 0);
        cpu.write_u32(SM0_INSTR, pio_pull(false, false, 0) as u32);
        assert_eq!(cpu.read_u32(FDEBUG) & FDEBUG_TXSTALL, FDEBUG_TXSTALL);
        cpu.write_u32(FDEBUG, FDEBUG_TXSTALL);
        assert_eq!(cpu.read_u32(FDEBUG) & FDEBUG_TXSTALL, FDEBUG_TXSTALL);
        cpu.write_u32(TXF0, 42);
        cpu.write_u32(FDEBUG, FDEBUG_TXSTALL);
        assert_eq!(cpu.read_u32(FDEBUG) & FDEBUG_TXSTALL, 0);
    }

    #[test]
    fn should_update_rxfnempty_flag_in_intr() {
        let cpu = TestDriver::new();
        reset_state_machines(&cpu);
        cpu.write_u32(IRQ0_INTE, INTR_SM0_RXNEMPTY);
        cpu.write_u32(NVIC_ICPR, PIO_IRQ0);
        assert_eq!(cpu.read_u32(INTR) & INTR_SM0_RXNEMPTY, 0);
        assert_eq!(cpu.read_u32(NVIC_ISPR) & PIO_IRQ0, 0);
        cpu.write_u32(SM0_INSTR, pio_push(false, false, 0) as u32);
        assert_eq!(cpu.read_u32(INTR) & INTR_SM0_RXNEMPTY, INTR_SM0_RXNEMPTY);
        assert_eq!(cpu.read_u32(NVIC_ISPR) & PIO_IRQ0, PIO_IRQ0);
        cpu.read_u32(RXF0);
        cpu.write_u32(NVIC_ICPR, PIO_IRQ0);
        assert_eq!(cpu.read_u32(INTR) & INTR_SM0_RXNEMPTY, 0);
        assert_eq!(cpu.read_u32(NVIC_ISPR) & PIO_IRQ0, 0);
    }
}
