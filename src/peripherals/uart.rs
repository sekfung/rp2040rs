use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::irq::Irq;
use crate::peripherals::dma::DreqChannel;
use crate::peripherals::peripheral::{atomic_update, Peripheral};
use crate::rp2040::BoardShared;
use crate::utils::Fifo;

const UARTDR: u32 = 0x00;
const UARTFR: u32 = 0x18;
const UARTIBRD: u32 = 0x24;
const UARTFBRD: u32 = 0x28;
const UARTLCR_H: u32 = 0x2c;
const UARTCR: u32 = 0x30;
const UARTIMSC: u32 = 0x38;
const UARTIRIS: u32 = 0x3c;
const UARTIMIS: u32 = 0x40;
const UARTICR: u32 = 0x44;
const UARTPERIPHID0: u32 = 0xfe0;
const UARTPERIPHID1: u32 = 0xfe4;
const UARTPERIPHID2: u32 = 0xfe8;
const UARTPERIPHID3: u32 = 0xfec;
const UARTPCELLID0: u32 = 0xff0;
const UARTPCELLID1: u32 = 0xff4;
const UARTPCELLID2: u32 = 0xff8;
const UARTPCELLID3: u32 = 0xffc;

const TXFE: u32 = 1 << 7;
const RXFF: u32 = 1 << 6;
const RXFE: u32 = 1 << 4;
const FEN: u32 = 1 << 4;
const RXE: u32 = 1 << 9;
const TXE: u32 = 1 << 8;
const UARTEN: u32 = 1 << 0;
const UARTTXINTR: u32 = 1 << 5;
const UARTRXINTR: u32 = 1 << 4;
const INT_MASK: u32 = 0x7ff;

type ByteCallback = Box<dyn FnMut(u8)>;
type LineCallback = Box<dyn FnMut(String)>;
type BaudCallback = Box<dyn FnMut(u32)>;
type ByteSlot = Rc<RefCell<Option<ByteCallback>>>;
type LineSlot = Rc<RefCell<Option<LineCallback>>>;
type BaudSlot = Rc<RefCell<Option<BaudCallback>>>;

struct UartState {
    ibrd: u32,
    fbrd: u32,
    lcr_h: u32,
    ctrl: u32,
    int_mask: u32,
    int_status: u32,
    baud_rate: u32,
    rx_fifo: Fifo,
    line_buffer: String,
    on_byte: ByteSlot,
    on_line: LineSlot,
    on_baud_rate_change: BaudSlot,
    dreq_tx: bool,
}

fn take_byte(slot: &ByteSlot) -> Option<ByteCallback> {
    slot.borrow_mut().take()
}

fn restore_byte(slot: &ByteSlot, callback: Option<ByteCallback>) {
    let mut current = slot.borrow_mut();
    if current.is_none() {
        *current = callback;
    }
}

fn take_line(slot: &LineSlot) -> Option<LineCallback> {
    slot.borrow_mut().take()
}

fn restore_line(slot: &LineSlot, callback: Option<LineCallback>) {
    let mut current = slot.borrow_mut();
    if current.is_none() {
        *current = callback;
    }
}

#[derive(Clone)]
pub struct Uart {
    name: String,
    irq: u32,
    state: Rc<RefCell<UartState>>,
    board: Weak<RefCell<BoardShared>>,
}

impl Uart {
    pub fn new(name: impl Into<String>, board: Weak<RefCell<BoardShared>>) -> Self {
        let name = name.into();
        let irq = if name.ends_with('1') {
            Irq::Uart1 as u32
        } else {
            Irq::Uart0 as u32
        };
        Self {
            name,
            irq,
            state: Rc::new(RefCell::new(UartState {
                ibrd: 0,
                fbrd: 0,
                lcr_h: 0,
                ctrl: RXE | TXE,
                int_mask: 0,
                int_status: 0,
                baud_rate: 0,
                rx_fifo: Fifo::new(32),
                line_buffer: String::new(),
                on_byte: Rc::new(RefCell::new(None)),
                on_line: Rc::new(RefCell::new(None)),
                on_baud_rate_change: Rc::new(RefCell::new(None)),
                dreq_tx: false,
            })),
            board,
        }
    }

    pub fn word_length(&self) -> u32 {
        ((self.state.borrow().lcr_h >> 5) & 0x3) + 5
    }

    pub fn baud_rate(&self) -> u32 {
        self.state.borrow().baud_rate
    }

    pub fn enabled(&self) -> bool {
        self.state.borrow().ctrl & UARTEN != 0
    }

    pub fn tx_enabled(&self) -> bool {
        self.state.borrow().ctrl & TXE != 0
    }

    pub fn rx_enabled(&self) -> bool {
        self.state.borrow().ctrl & RXE != 0
    }

    pub fn fifos_enabled(&self) -> bool {
        self.state.borrow().lcr_h & FEN != 0
    }

    pub fn set_on_byte(&self, callback: Box<dyn FnMut(u8)>) {
        *self.state.borrow().on_byte.borrow_mut() = Some(callback);
    }

    pub fn set_on_line(&self, callback: Box<dyn FnMut(String)>) {
        *self.state.borrow().on_line.borrow_mut() = Some(callback);
    }

    pub fn set_on_baud_rate_change(&self, callback: Box<dyn FnMut(u32)>) {
        *self.state.borrow().on_baud_rate_change.borrow_mut() = Some(callback);
    }

    pub fn feed_byte(&self, value: u8) {
        let (line, line_slot) = {
            let mut state = self.state.borrow_mut();
            state.rx_fifo.push(u32::from(value));
            state.int_status |= UARTRXINTR;
            let line_slot = state.on_line.clone();
            let mut line = None;
            if state.lcr_h & FEN != 0 {
                let ch = value as char;
                if ch == '\n' {
                    let complete = std::mem::take(&mut state.line_buffer);
                    line = Some(complete);
                } else if ch != '\r' {
                    state.line_buffer.push(ch);
                }
            }
            (line, line_slot)
        };
        self.check_interrupts();
        if let Some(line) = line {
            if let Some(mut callback) = take_line(&line_slot) {
                callback(line);
                restore_line(&line_slot, Some(callback));
            }
        }
    }

    pub fn clk_peri_changed(&self, clk_peri: f64) {
        let divider = {
            let state = self.state.borrow();
            f64::from(state.ibrd) + f64::from(state.fbrd) / 64.0
        };
        if divider != 0.0 {
            self.recompute_baud(clk_peri);
        }
    }

    fn read_register(&self, offset: u32) -> u32 {
        match offset {
            UARTDR => {
                let value = {
                    let mut state = self.state.borrow_mut();
                    let value = state.rx_fifo.pull();
                    if state.rx_fifo.empty() {
                        state.int_status &= !UARTRXINTR;
                    } else {
                        state.int_status |= UARTRXINTR;
                    }
                    value
                };
                self.check_interrupts();
                value
            }
            UARTFR => {
                let state = self.state.borrow();
                (if state.rx_fifo.full() { RXFF } else { 0 })
                    | (if state.rx_fifo.empty() { RXFE } else { 0 })
                    | TXFE
            }
            UARTIBRD => self.state.borrow().ibrd,
            UARTFBRD => self.state.borrow().fbrd,
            UARTLCR_H => self.state.borrow().lcr_h,
            UARTCR => self.state.borrow().ctrl,
            UARTIMSC => self.state.borrow().int_mask,
            UARTIRIS => self.state.borrow().int_status,
            UARTIMIS => {
                let state = self.state.borrow();
                state.int_status & state.int_mask
            }
            UARTPERIPHID0 => 0x11,
            UARTPERIPHID1 => 0x10,
            UARTPERIPHID2 => 0x34,
            UARTPERIPHID3 => 0x00,
            UARTPCELLID0 => 0x0d,
            UARTPCELLID1 => 0xf0,
            UARTPCELLID2 => 0x05,
            UARTPCELLID3 => 0xb1,
            _ => 0xffff_ffff,
        }
    }

    fn write_register(&self, offset: u32, value: u32) {
        match offset {
            UARTDR => {
                if self.tx_enabled() {
                    let slot = self.state.borrow().on_byte.clone();
                    if let Some(mut callback) = take_byte(&slot) {
                        callback((value & 0xff) as u8);
                        restore_byte(&slot, Some(callback));
                    }
                }
            }
            UARTIBRD => {
                self.state.borrow_mut().ibrd = value & 0xffff;
                self.recompute_baud(self.clk_peri());
            }
            UARTFBRD => {
                self.state.borrow_mut().fbrd = value & 0x3f;
                self.recompute_baud(self.clk_peri());
            }
            UARTLCR_H => self.state.borrow_mut().lcr_h = value,
            UARTCR => {
                let mut state = self.state.borrow_mut();
                state.ctrl = value;
                if value & (UARTEN | TXE) == (UARTEN | TXE) {
                    state.int_status |= UARTTXINTR;
                }
                drop(state);
                self.update_dreq();
                self.check_interrupts();
            }
            UARTIMSC => {
                self.state.borrow_mut().int_mask = value & INT_MASK;
                self.check_interrupts();
            }
            UARTICR => {
                self.state.borrow_mut().int_status &= !value;
                self.check_interrupts();
            }
            _ => {
                if let Some(board) = self.board.upgrade() {
                    let logger = board.borrow().logger.clone();
                    logger.borrow().warn(&self.name, "Unimplemented UART write");
                }
            }
        }
    }

    fn update_dreq(&self) {
        let enabled = self.enabled();
        let (changed, channel) = {
            let mut state = self.state.borrow_mut();
            let changed = state.dreq_tx != enabled;
            state.dreq_tx = enabled;
            let channel = if self.name.ends_with('1') {
                DreqChannel::Uart1Tx
            } else {
                DreqChannel::Uart0Tx
            };
            (changed, channel)
        };
        if !changed {
            return;
        }
        if let Some(board) = self.board.upgrade() {
            let dma = board.borrow().dma.clone();
            if let Some(dma) = dma {
                if enabled {
                    dma.set_dreq(channel);
                } else {
                    dma.clear_dreq(channel);
                }
            }
        }
    }

    fn check_interrupts(&self) {
        let active = {
            let state = self.state.borrow();
            state.int_status & state.int_mask != 0
        };
        if let Some(board) = self.board.upgrade() {
            let core = board.borrow().core.clone();
            core.set_interrupt(self.irq, active);
        }
    }

    fn clk_peri(&self) -> f64 {
        self.board
            .upgrade()
            .map(|b| b.borrow().clk_peri)
            .unwrap_or(125_000_000.0)
    }

    fn recompute_baud(&self, clk_peri: f64) {
        let baud_rate = {
            let state = self.state.borrow();
            let divider = f64::from(state.ibrd) + f64::from(state.fbrd) / 64.0;
            if divider == 0.0 {
                0
            } else {
                (clk_peri / (16.0 * divider)).round() as u32
            }
        };
        let slot = {
            let mut state = self.state.borrow_mut();
            state.baud_rate = baud_rate;
            state.on_baud_rate_change.clone()
        };
        let mut callback = {
            let mut guard = slot.borrow_mut();
            guard.take()
        };
        if let Some(callback) = callback.as_mut() {
            callback(baud_rate);
        }
        let mut current = slot.borrow_mut();
        if current.is_none() {
            *current = callback;
        }
    }
}

impl Peripheral for Uart {
    fn name(&self) -> &str {
        &self.name
    }

    fn read_u32(&self, offset: u32) -> u32 {
        self.read_register(offset)
    }

    fn write_u32_atomic(&self, offset: u32, value: u32, atomic_type: u32) {
        if offset == UARTICR {
            self.write_register(offset, value);
            return;
        }
        let value = if atomic_type == 0 {
            value
        } else {
            atomic_update(self.read_register(offset), atomic_type, value)
        };
        self.write_register(offset, value);
    }
}

#[cfg(test)]
mod tests {
    use crate::peripherals::peripheral::Peripheral;
    use crate::rp2040::Rp2040;

    const UARTIBRD: u32 = 0x24;
    const UARTFBRD: u32 = 0x28;
    const UARTLCR_H: u32 = 0x2c;

    #[test]
    fn should_return_word_length_based_on_uartlcr_h() {
        let rp = Rp2040::new();
        let uart = rp.uart[0].clone();
        uart.write_u32_atomic(UARTLCR_H, 0x70, 0);
        assert_eq!(uart.word_length(), 8);
    }

    #[test]
    fn should_calculate_baud_rate_from_ibrd_fbrd() {
        let rp = Rp2040::new();
        let uart = rp.uart[0].clone();
        uart.write_u32_atomic(UARTIBRD, 67, 0);
        uart.write_u32_atomic(UARTFBRD, 52, 0);
        assert_eq!(uart.baud_rate(), 115207);
    }
}
