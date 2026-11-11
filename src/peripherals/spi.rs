use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::irq::Irq;
use crate::peripherals::peripheral::{atomic_update, Peripheral};
use crate::rp2040::BoardShared;
use crate::utils::Fifo;

const SSPCR0: u32 = 0x000;
const SSPCR1: u32 = 0x004;
const SSPDR: u32 = 0x008;
const SSPSR: u32 = 0x00c;
const SSPCPSR: u32 = 0x010;
const SSPIMSC: u32 = 0x014;
const SSPRIS: u32 = 0x018;
const SSPMIS: u32 = 0x01c;
const SSPICR: u32 = 0x020;
const SSPDMACR: u32 = 0x024;
const SSPPERIPHID0: u32 = 0xfe0;
const SSPPERIPHID1: u32 = 0xfe4;
const SSPPERIPHID2: u32 = 0xfe8;
const SSPPERIPHID3: u32 = 0xfec;
const SSPPCELLID0: u32 = 0xff0;

const DSS_MASK: u32 = 0xf;
const BSY: u32 = 1 << 4;
const RFF: u32 = 1 << 3;
const RNE: u32 = 1 << 2;
const TNF: u32 = 1 << 1;
const TFE: u32 = 1 << 0;
const SSPTXINTR: u32 = 1 << 3;
const SSPRXINTR: u32 = 1 << 2;
const SSPRORINTR: u32 = 1 << 0;

#[derive(Clone)]
pub struct Spi {
    name: String,
    state: Rc<RefCell<SpiState>>,
    board: Weak<RefCell<BoardShared>>,
    irq: u32,
}

struct SpiState {
    rx_fifo: Fifo,
    tx_fifo: Fifo,
    busy: bool,
    control0: u32,
    control1: u32,
    dma_control: u32,
    clock_divisor: u32,
    int_raw: u32,
    int_enable: u32,
    on_transmit: Option<Box<dyn FnMut(u8) -> u8>>,
}

impl Spi {
    pub fn new(name: impl Into<String>, irq: Irq, board: Weak<RefCell<BoardShared>>) -> Self {
        Self {
            name: name.into(),
            state: Rc::new(RefCell::new(SpiState {
                rx_fifo: Fifo::new(8),
                tx_fifo: Fifo::new(8),
                busy: false,
                control0: 0,
                control1: 0,
                dma_control: 0,
                clock_divisor: 0,
                int_raw: 0,
                int_enable: 0,
                on_transmit: None,
            })),
            board,
            irq: irq as u32,
        }
    }

    pub fn set_on_transmit(&self, callback: Box<dyn FnMut(u8) -> u8>) {
        self.state.borrow_mut().on_transmit = Some(callback);
    }

    fn data_bits(&self) -> u32 {
        ((self.state.borrow().control0 & DSS_MASK) + 1).min(16)
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

    fn fifos_updated(&self) {
        {
            let mut state = self.state.borrow_mut();
            if state.tx_fifo.item_count() <= state.tx_fifo.size() / 2 {
                state.int_raw |= SSPTXINTR;
            } else {
                state.int_raw &= !SSPTXINTR;
            }
            if state.rx_fifo.item_count() >= state.rx_fifo.size() / 2 {
                state.int_raw |= SSPRXINTR;
            } else {
                state.int_raw &= !SSPRXINTR;
            }
        }
        self.check_interrupts();
    }

    fn do_tx(&self) {
        let value = {
            let mut state = self.state.borrow_mut();
            if state.busy || state.tx_fifo.empty() || state.control1 & 2 == 0 {
                return;
            }
            let value = state.tx_fifo.pull() as u8;
            state.busy = true;
            value
        };
        let mut callback = self.state.borrow_mut().on_transmit.take();
        let response = match callback.as_mut() {
            Some(callback) => callback(value),
            None => 0,
        };
        self.state.borrow_mut().on_transmit = callback;
        self.complete_transmit(response);
    }

    pub fn complete_transmit(&self, value: u8) {
        {
            let mut state = self.state.borrow_mut();
            state.busy = false;
            if state.rx_fifo.full() {
                state.int_raw |= SSPRORINTR;
            } else {
                state.rx_fifo.push(u32::from(value));
            }
        }
        self.fifos_updated();
        self.do_tx();
    }
}

impl Peripheral for Spi {
    fn name(&self) -> &str {
        &self.name
    }

    fn read_u32(&self, offset: u32) -> u32 {
        match offset {
            SSPCR0 => self.state.borrow().control0,
            SSPCR1 => self.state.borrow().control1,
            SSPDR => {
                let mut state = self.state.borrow_mut();
                if state.rx_fifo.empty() {
                    0
                } else {
                    let value = state.rx_fifo.pull();
                    drop(state);
                    self.fifos_updated();
                    value
                }
            }
            SSPSR => {
                let state = self.state.borrow();
                (if state.busy || !state.tx_fifo.empty() {
                    BSY
                } else {
                    0
                }) | if state.rx_fifo.full() { RFF } else { 0 }
                    | if !state.rx_fifo.empty() { RNE } else { 0 }
                    | if !state.tx_fifo.full() { TNF } else { 0 }
                    | if state.tx_fifo.empty() { TFE } else { 0 }
            }
            SSPCPSR => self.state.borrow().clock_divisor,
            SSPIMSC => self.state.borrow().int_enable,
            SSPRIS => self.state.borrow().int_raw,
            SSPMIS => self.int_status(),
            SSPDMACR => self.state.borrow().dma_control,
            SSPPERIPHID0 => 0x22,
            SSPPERIPHID1 => 0x10,
            SSPPERIPHID2 => 0x34,
            SSPPERIPHID3 => 0x00,
            SSPPCELLID0 => 0x0d,
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
            SSPCR0 => self.state.borrow_mut().control0 = value,
            SSPCR1 => self.state.borrow_mut().control1 = value,
            SSPDR => {
                let bits = self.data_bits();
                let value = value & ((1u32 << bits) - 1);
                let mut state = self.state.borrow_mut();
                if !state.tx_fifo.full() {
                    state.tx_fifo.push(value);
                }
                drop(state);
                self.do_tx();
                self.fifos_updated();
            }
            SSPCPSR => self.state.borrow_mut().clock_divisor = value & 0xfe,
            SSPIMSC => {
                self.state.borrow_mut().int_enable = value;
                self.check_interrupts();
            }
            SSPDMACR => self.state.borrow_mut().dma_control = value,
            SSPICR => {
                self.state.borrow_mut().int_raw &= !(value & (1 << 1 | SSPRORINTR));
                self.check_interrupts();
            }
            _ => {}
        }
    }
}
