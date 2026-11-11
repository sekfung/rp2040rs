use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::cortex_m0_core::Core;
use crate::rp2040::BoardShared;

const CPUID: u32 = 0x000;
const GPIO_IN: u32 = 0x004;
const GPIO_HI_IN: u32 = 0x008;
const GPIO_OUT: u32 = 0x010;
const GPIO_OUT_SET: u32 = 0x014;
const GPIO_OUT_CLR: u32 = 0x018;
const GPIO_OUT_XOR: u32 = 0x01c;
const GPIO_OE: u32 = 0x020;
const GPIO_OE_SET: u32 = 0x024;
const GPIO_OE_CLR: u32 = 0x028;
const GPIO_OE_XOR: u32 = 0x02c;
const GPIO_HI_OUT: u32 = 0x030;
const GPIO_HI_OUT_SET: u32 = 0x034;
const GPIO_HI_OUT_CLR: u32 = 0x038;
const GPIO_HI_OUT_XOR: u32 = 0x03c;
const GPIO_HI_OE: u32 = 0x040;
const GPIO_HI_OE_SET: u32 = 0x044;
const GPIO_HI_OE_CLR: u32 = 0x048;
const GPIO_HI_OE_XOR: u32 = 0x04c;
const GPIO_MASK: u32 = 0x3fff_ffff;

const DIV_UDIVIDEND: u32 = 0x060;
const DIV_UDIVISOR: u32 = 0x064;
const DIV_SDIVIDEND: u32 = 0x068;
const DIV_SDIVISOR: u32 = 0x06c;
const DIV_QUOTIENT: u32 = 0x070;
const DIV_REMAINDER: u32 = 0x074;
const DIV_CSR: u32 = 0x078;

const SPINLOCK_ST: u32 = 0x05c;
const SPINLOCK0: u32 = 0x100;
const SPINLOCK31: u32 = 0x17c;

#[derive(Default)]
struct SioState {
    gpio_value: u32,
    gpio_output_enable: u32,
    qspi_gpio_value: u32,
    qspi_gpio_output_enable: u32,
    div_dividend: u32,
    div_dividend_value: i64,
    div_divisor: u32,
    div_quotient: u32,
    div_remainder: u32,
    div_csr: u32,
    spin_lock: u32,
}

#[derive(Clone)]
pub struct Sio {
    core: Core,
    board: Weak<RefCell<BoardShared>>,
    state: Rc<RefCell<SioState>>,
}

impl Sio {
    pub fn new(core: Core, board: Weak<RefCell<BoardShared>>) -> Self {
        Self {
            core,
            board,
            state: Rc::new(RefCell::new(SioState::default())),
        }
    }

    fn warn(&self, message: &str) {
        if let Some(board) = self.board.upgrade() {
            let logger = board.borrow().logger.clone();
            logger.borrow().warn("SIO", message);
        }
    }

    fn gpio_values(&self) -> u32 {
        self.board
            .upgrade()
            .map(|board| {
                board
                    .borrow()
                    .gpio
                    .iter()
                    .enumerate()
                    .fold(0, |acc, (index, pin)| {
                        if pin.input_value() {
                            acc | (1 << index)
                        } else {
                            acc
                        }
                    })
            })
            .unwrap_or(0)
    }

    fn qspi_values(&self) -> u32 {
        self.board
            .upgrade()
            .map(|board| {
                board
                    .borrow()
                    .qspi
                    .iter()
                    .enumerate()
                    .fold(0, |acc, (index, pin)| {
                        if pin.input_value() {
                            acc | (1 << index)
                        } else {
                            acc
                        }
                    })
            })
            .unwrap_or(0)
    }

    fn update_hardware_divider(&self, signed: bool) {
        let (dividend, dividend_value, divisor);
        {
            let state = self.state.borrow();
            dividend = state.div_dividend;
            dividend_value = state.div_dividend_value;
            divisor = state.div_divisor;
        }
        let (quotient, remainder) = if divisor == 0 {
            let quotient = if dividend_value > 0 { 0xffff_ffff } else { 1 };
            (quotient, dividend)
        } else if signed {
            (
                ((dividend as i32) / (divisor as i32)) as u32,
                ((dividend as i32) % (divisor as i32)) as u32,
            )
        } else {
            (dividend / divisor, dividend % divisor)
        };
        let mut state = self.state.borrow_mut();
        state.div_quotient = quotient;
        state.div_remainder = remainder;
        state.div_csr = 0b11;
        drop(state);
        self.core.set_cycles(self.core.cycles().wrapping_add(8));
    }

    pub fn read_u32(&self, offset: u32) -> u32 {
        if (SPINLOCK0..=SPINLOCK31).contains(&offset) {
            let bit = 1u32 << ((offset - SPINLOCK0) / 4);
            let mut state = self.state.borrow_mut();
            if state.spin_lock & bit != 0 {
                return 0;
            }
            state.spin_lock |= bit;
            return bit;
        }
        let mut state = self.state.borrow_mut();
        match offset {
            GPIO_IN => self.gpio_values(),
            GPIO_HI_IN => self.qspi_values(),
            GPIO_OUT => state.gpio_value,
            GPIO_OE => state.gpio_output_enable,
            GPIO_HI_OUT => state.qspi_gpio_value,
            GPIO_HI_OE => state.qspi_gpio_output_enable,
            GPIO_OUT_SET | GPIO_OUT_CLR | GPIO_OUT_XOR | GPIO_OE_SET | GPIO_OE_CLR
            | GPIO_OE_XOR | GPIO_HI_OUT_SET | GPIO_HI_OUT_CLR | GPIO_HI_OUT_XOR
            | GPIO_HI_OE_SET | GPIO_HI_OE_CLR | GPIO_HI_OE_XOR => 0,
            CPUID => 0,
            SPINLOCK_ST => state.spin_lock,
            DIV_UDIVIDEND | DIV_SDIVIDEND => state.div_dividend,
            DIV_UDIVISOR | DIV_SDIVISOR => state.div_divisor,
            DIV_QUOTIENT => {
                state.div_csr &= !0b10;
                state.div_quotient
            }
            DIV_REMAINDER => state.div_remainder,
            DIV_CSR => state.div_csr,
            _ => {
                drop(state);
                self.warn(&format!("Read from invalid SIO address: {offset:x}"));
                0xffff_ffff
            }
        }
    }

    pub fn write_u32(&self, offset: u32, value: u32) {
        if (SPINLOCK0..=SPINLOCK31).contains(&offset) {
            let bit = 1u32 << ((offset - SPINLOCK0) / 4);
            self.state.borrow_mut().spin_lock &= !bit;
            return;
        }
        let mut state = self.state.borrow_mut();
        match offset {
            GPIO_OUT => state.gpio_value = value & GPIO_MASK,
            GPIO_OUT_SET => state.gpio_value |= value & GPIO_MASK,
            GPIO_OUT_CLR => state.gpio_value &= !value,
            GPIO_OUT_XOR => state.gpio_value ^= value & GPIO_MASK,
            GPIO_OE => state.gpio_output_enable = value & GPIO_MASK,
            GPIO_OE_SET => state.gpio_output_enable |= value & GPIO_MASK,
            GPIO_OE_CLR => state.gpio_output_enable &= !value,
            GPIO_OE_XOR => state.gpio_output_enable ^= value & GPIO_MASK,
            GPIO_HI_OUT => state.qspi_gpio_value = value & GPIO_MASK,
            GPIO_HI_OUT_SET => state.qspi_gpio_value |= value & GPIO_MASK,
            GPIO_HI_OUT_CLR => state.qspi_gpio_value &= !value,
            GPIO_HI_OUT_XOR => state.qspi_gpio_value ^= value & GPIO_MASK,
            GPIO_HI_OE => state.qspi_gpio_output_enable = value & GPIO_MASK,
            GPIO_HI_OE_SET => state.qspi_gpio_output_enable |= value & GPIO_MASK,
            GPIO_HI_OE_CLR => state.qspi_gpio_output_enable &= !value,
            GPIO_HI_OE_XOR => state.qspi_gpio_output_enable ^= value & GPIO_MASK,
            DIV_UDIVIDEND => {
                state.div_dividend = value;
                state.div_dividend_value = value as i64;
                drop(state);
                self.update_hardware_divider(false);
            }
            DIV_SDIVIDEND => {
                state.div_dividend = value;
                state.div_dividend_value = (value as i32) as i64;
                drop(state);
                self.update_hardware_divider(true);
            }
            DIV_UDIVISOR => {
                state.div_divisor = value;
                drop(state);
                self.update_hardware_divider(false);
            }
            DIV_SDIVISOR => {
                state.div_divisor = value;
                drop(state);
                self.update_hardware_divider(true);
            }
            DIV_QUOTIENT => {
                state.div_quotient = value;
                state.div_csr = 0b11;
            }
            DIV_REMAINDER => {
                state.div_remainder = value;
                state.div_csr = 0b11;
            }
            _ => {
                drop(state);
                self.warn(&format!(
                    "Write to invalid SIO address: {offset:x}, value={value:x}"
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::rp2040::{Rp2040, SIO_START_ADDRESS};

    const SIO_DIV_UDIVIDEND: u32 = SIO_START_ADDRESS + 0x060;
    const SIO_DIV_UDIVISOR: u32 = SIO_START_ADDRESS + 0x064;
    const SIO_DIV_SDIVIDEND: u32 = SIO_START_ADDRESS + 0x068;
    const SIO_DIV_SDIVISOR: u32 = SIO_START_ADDRESS + 0x06c;
    const SIO_DIV_QUOTIENT: u32 = SIO_START_ADDRESS + 0x070;
    const SIO_DIV_REMAINDER: u32 = SIO_START_ADDRESS + 0x074;
    const SIO_DIV_CSR: u32 = SIO_START_ADDRESS + 0x078;
    const SIO_SPINLOCK10: u32 = SIO_START_ADDRESS + 0x128;
    const SIO_SPINLOCKST: u32 = SIO_START_ADDRESS + 0x05c;

    #[test]
    fn should_perform_a_signed_hardware_divider_123456_div_neg_321() {
        let rp = Rp2040::new();
        rp.write_u32(SIO_DIV_SDIVIDEND, 123456);
        assert_eq!(rp.read_u32(SIO_DIV_SDIVIDEND) as i32, 123456);
        rp.write_u32(SIO_DIV_SDIVISOR, (-321i32) as u32);
        assert_eq!(rp.read_u32(SIO_DIV_CSR), 3);
        assert_eq!(rp.read_u32(SIO_DIV_SDIVISOR) as i32, -321);
        assert_eq!(rp.read_u32(SIO_DIV_REMAINDER) as i32, 192);
        assert_eq!(rp.read_u32(SIO_DIV_QUOTIENT) as i32, -384);
        assert_eq!(rp.read_u32(SIO_DIV_CSR), 1);
    }

    #[test]
    fn should_perform_a_signed_hardware_divider_neg_3000_div_2() {
        let rp = Rp2040::new();
        rp.write_u32(SIO_DIV_SDIVIDEND, (-3000i32) as u32);
        assert_eq!(rp.read_u32(SIO_DIV_SDIVIDEND) as i32, -3000);
        rp.write_u32(SIO_DIV_SDIVISOR, 2);
        assert_eq!(rp.read_u32(SIO_DIV_CSR), 3);
        assert_eq!(rp.read_u32(SIO_DIV_SDIVISOR) as i32, 2);
        assert_eq!(rp.read_u32(SIO_DIV_REMAINDER) as i32, 0);
        assert_eq!(rp.read_u32(SIO_DIV_QUOTIENT) as i32, -1500);
        assert_eq!(rp.read_u32(SIO_DIV_CSR), 1);
    }

    #[test]
    fn should_perform_an_unsigned_hardware_divider_123456_div_321() {
        let rp = Rp2040::new();
        rp.write_u32(SIO_DIV_UDIVIDEND, 123456);
        rp.write_u32(SIO_DIV_UDIVISOR, 321);
        assert_eq!(rp.read_u32(SIO_DIV_CSR), 3);
        assert_eq!(rp.read_u32(SIO_DIV_REMAINDER), 192);
        assert_eq!(rp.read_u32(SIO_DIV_QUOTIENT), 384);
        assert_eq!(rp.read_u32(SIO_DIV_CSR), 1);
    }

    #[test]
    fn should_store_restore_quotient_and_remainder() {
        let rp = Rp2040::new();
        rp.write_u32(SIO_DIV_SDIVIDEND, 123456);
        rp.write_u32(SIO_DIV_SDIVISOR, (-321i32) as u32);
        let remainder = rp.read_u32(SIO_DIV_REMAINDER) as i32;
        let quotient = rp.read_u32(SIO_DIV_QUOTIENT) as i32;
        assert_eq!(remainder, 192);
        assert_eq!(quotient, -384);
        rp.write_u32(SIO_DIV_UDIVIDEND, 123);
        rp.write_u32(SIO_DIV_UDIVISOR, 7);
        assert_eq!(rp.read_u32(SIO_DIV_REMAINDER), 4);
        assert_eq!(rp.read_u32(SIO_DIV_QUOTIENT), 17);
        rp.write_u32(SIO_DIV_REMAINDER, remainder as u32);
        rp.write_u32(SIO_DIV_QUOTIENT, quotient as u32);
        assert_eq!(rp.read_u32(SIO_DIV_CSR), 3);
        assert_eq!(rp.read_u32(SIO_DIV_REMAINDER) as i32, 192);
        assert_eq!(rp.read_u32(SIO_DIV_QUOTIENT) as i32, -384);
    }

    #[test]
    fn should_handle_unsigned_division_by_zero_123456() {
        let rp = Rp2040::new();
        rp.write_u32(SIO_DIV_UDIVIDEND, 123456);
        rp.write_u32(SIO_DIV_UDIVISOR, 0);
        assert_eq!(rp.read_u32(SIO_DIV_REMAINDER), 123456);
        assert_eq!(rp.read_u32(SIO_DIV_QUOTIENT), 0xffff_ffff);
    }

    #[test]
    fn should_handle_unsigned_division_by_zero_0x80000000() {
        let rp = Rp2040::new();
        rp.write_u32(SIO_DIV_UDIVIDEND, 0x8000_0000);
        rp.write_u32(SIO_DIV_UDIVISOR, 0);
        assert_eq!(rp.read_u32(SIO_DIV_REMAINDER), 0x8000_0000);
        assert_eq!(rp.read_u32(SIO_DIV_QUOTIENT), 0xffff_ffff);
    }

    #[test]
    fn should_handle_signed_division_by_zero_3000() {
        let rp = Rp2040::new();
        rp.write_u32(SIO_DIV_SDIVIDEND, 3000);
        rp.write_u32(SIO_DIV_SDIVISOR, 0);
        assert_eq!(rp.read_u32(SIO_DIV_REMAINDER) as i32, 3000);
        assert_eq!(rp.read_u32(SIO_DIV_QUOTIENT) as i32, -1);
    }

    #[test]
    fn should_handle_signed_division_by_zero_neg_3000() {
        let rp = Rp2040::new();
        rp.write_u32(SIO_DIV_SDIVIDEND, (-3000i32) as u32);
        rp.write_u32(SIO_DIV_SDIVISOR, 0);
        assert_eq!(rp.read_u32(SIO_DIV_REMAINDER) as i32, -3000);
        assert_eq!(rp.read_u32(SIO_DIV_QUOTIENT) as i32, 1);
    }

    #[test]
    fn should_handle_signed_division_0x80000000_div_2() {
        let rp = Rp2040::new();
        rp.write_u32(SIO_DIV_SDIVIDEND, 0x8000_0000);
        rp.write_u32(SIO_DIV_SDIVISOR, 2);
        assert_eq!(rp.read_u32(SIO_DIV_REMAINDER), 0);
        assert_eq!(rp.read_u32(SIO_DIV_QUOTIENT), 0xc000_0000);
    }

    #[test]
    fn should_handle_unsigned_division_0x80000000_div_2() {
        let rp = Rp2040::new();
        rp.write_u32(SIO_DIV_UDIVIDEND, 0x8000_0000);
        rp.write_u32(SIO_DIV_UDIVISOR, 2);
        assert_eq!(rp.read_u32(SIO_DIV_REMAINDER), 0);
        assert_eq!(rp.read_u32(SIO_DIV_QUOTIENT), 0x4000_0000);
    }

    #[test]
    fn should_unlock_lock_and_check_spinlock10() {
        let rp = Rp2040::new();
        rp.write_u32(SIO_SPINLOCK10, 1);
        assert_eq!(rp.read_u32(SIO_SPINLOCK10), 1024);
        assert_eq!(rp.read_u32(SIO_SPINLOCKST), 1024);
        assert_eq!(rp.read_u32(SIO_SPINLOCK10), 0);
        assert_eq!(rp.read_u32(SIO_SPINLOCKST), 1024);
        rp.write_u32(SIO_SPINLOCK10, 1);
        assert_eq!(rp.read_u32(SIO_SPINLOCKST), 0);
    }
}
