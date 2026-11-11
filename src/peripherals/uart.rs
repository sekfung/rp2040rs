use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::peripherals::peripheral::{atomic_update, Peripheral};
use crate::rp2040::BoardShared;

const UARTIBRD: u32 = 0x24;
const UARTFBRD: u32 = 0x28;
const UARTLCR_H: u32 = 0x2c;

type BaudCallback = Box<dyn FnMut(u32)>;
type BaudCallbackSlot = Rc<RefCell<Option<BaudCallback>>>;

struct UartState {
    ibrd: u32,
    fbrd: u32,
    lcr_h: u32,
    baud_rate: u32,
    on_baud_rate_change: BaudCallbackSlot,
}

struct BaudCallbackGuard<'a> {
    slot: &'a RefCell<Option<BaudCallback>>,
    callback: Option<BaudCallback>,
}

impl Drop for BaudCallbackGuard<'_> {
    fn drop(&mut self) {
        if self.slot.borrow().is_none() {
            *self.slot.borrow_mut() = self.callback.take();
        }
    }
}

#[derive(Clone)]
pub struct Uart {
    name: String,
    state: Rc<RefCell<UartState>>,
    board: Weak<RefCell<BoardShared>>,
}

impl Uart {
    pub fn new(name: impl Into<String>, board: Weak<RefCell<BoardShared>>) -> Self {
        Self {
            name: name.into(),
            state: Rc::new(RefCell::new(UartState {
                ibrd: 0,
                fbrd: 0,
                lcr_h: 0,
                baud_rate: 0,
                on_baud_rate_change: Rc::new(RefCell::new(None)),
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

    pub fn set_on_baud_rate_change(&self, callback: Box<dyn FnMut(u32)>) {
        *self.state.borrow().on_baud_rate_change.borrow_mut() = Some(callback);
    }

    pub fn clk_peri_changed(&self, clk_peri: f64) {
        self.recompute_baud(clk_peri);
    }

    fn read_register(&self, offset: u32) -> u32 {
        let state = self.state.borrow();
        match offset {
            UARTIBRD => state.ibrd,
            UARTFBRD => state.fbrd,
            UARTLCR_H => state.lcr_h,
            _ => 0xffff_ffff,
        }
    }

    fn write_register(&self, offset: u32, value: u32) {
        match offset {
            UARTIBRD => {
                self.state.borrow_mut().ibrd = value & 0xffff;
                self.recompute_baud(self.clk_peri());
            }
            UARTFBRD => {
                self.state.borrow_mut().fbrd = value & 0x3f;
                self.recompute_baud(self.clk_peri());
            }
            UARTLCR_H => self.state.borrow_mut().lcr_h = value,
            _ => {
                if let Some(board) = self.board.upgrade() {
                    let logger = board.borrow().logger.clone();
                    logger.borrow().warn(&self.name, "Unimplemented UART write");
                }
            }
        }
    }

    fn clk_peri(&self) -> f64 {
        self.board
            .upgrade()
            .map(|b| b.borrow().clk_peri)
            .unwrap_or(125_000_000.0)
    }

    fn recompute_baud(&self, clk_peri: f64) {
        let (divider_set, baud_rate) = {
            let state = self.state.borrow();
            let divider = f64::from(state.ibrd) + f64::from(state.fbrd) / 64.0;
            if divider == 0.0 {
                (false, 0)
            } else {
                (true, (clk_peri / (16.0 * divider)).round() as u32)
            }
        };
        if !divider_set {
            return;
        }
        let slot = {
            let mut state = self.state.borrow_mut();
            state.baud_rate = baud_rate;
            state.on_baud_rate_change.clone()
        };
        let callback = slot.borrow_mut().take();
        if let Some(callback) = callback {
            let mut guard = BaudCallbackGuard {
                slot: &slot,
                callback: Some(callback),
            };
            if let Some(callback) = guard.callback.as_mut() {
                callback(baud_rate);
            }
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
