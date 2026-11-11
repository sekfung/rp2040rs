use std::rc::Rc;

use rp2040rs::clock::MockClock;
use rp2040rs::peripherals::peripheral::Peripheral;
use rp2040rs::rp2040::Rp2040;

const PWM_BASE: u32 = 0x4005_0000;
const CH0_CSR: u32 = PWM_BASE;
const CH0_CTR: u32 = PWM_BASE + 0x08;
const CH0_TOP: u32 = PWM_BASE + 0x10;
const EN: u32 = PWM_BASE + 0xa0;
const INTR: u32 = PWM_BASE + 0xa4;
const INTE: u32 = PWM_BASE + 0xa8;
const ATOMIC_SET: u32 = 0x2000;
const CSR_EN: u32 = 1;
const CSR_PH_ADV: u32 = 1 << 7;

const TIMERAWL: u32 = 0x4005_4028;
const UART_IBRD: u32 = 0x24;
const UART_FBRD: u32 = 0x28;

#[test]
fn pwm_wrap_interrupt_is_periodic() {
    let clock = Rc::new(MockClock::new());
    let rp = Rp2040::with_clock(clock.clone());
    rp.write_u32(CH0_TOP, 999);
    rp.write_u32(CH0_CSR, CSR_EN);
    rp.write_u32(CH0_CTR, 0);

    clock.advance(8);
    assert_eq!(rp.read_u32(INTR) & 1, 1);
    rp.write_u32(INTR, 1);
    clock.advance(8);
    assert_eq!(rp.read_u32(INTR) & 1, 1);
}

#[test]
fn pwm_shared_registers_honor_atomic_aliases() {
    let rp = Rp2040::new();
    rp.write_u32(EN, 1);
    rp.write_u32(EN | ATOMIC_SET, 2);
    assert_eq!(rp.read_u32(EN), 3);

    rp.write_u32(INTE, 1);
    rp.write_u32(INTE | ATOMIC_SET, 2);
    assert_eq!(rp.read_u32(INTE), 3);
}

#[test]
fn pwm_advance_wraps_modulo_top_plus_one() {
    let rp = Rp2040::new();
    rp.write_u32(CH0_TOP, 999);
    rp.write_u32(CH0_CSR, CSR_EN);
    rp.write_u32(CH0_CTR, 999);
    rp.write_u32(CH0_CSR | ATOMIC_SET, CSR_PH_ADV);
    assert_eq!(rp.read_u32(CH0_CTR), 0);
}

#[test]
fn pwm_reset_clears_enable_and_interrupts() {
    let rp = Rp2040::new();
    rp.write_u32(CH0_CSR, CSR_EN);
    rp.reset();
    assert_eq!(rp.read_u32(CH0_CSR) & CSR_EN, 0);
    assert_eq!(rp.read_u32(INTR), 0);
}

#[test]
fn timer_low_word_wraps_modulo_2_32() {
    let clock = Rc::new(MockClock::new());
    let rp = Rp2040::with_clock(clock.clone());
    clock.advance(4_294_967_296 + 5);
    assert_eq!(rp.read_u32(TIMERAWL), 5);
}

#[test]
fn uart_clearing_divider_updates_baud_rate() {
    let rp = Rp2040::new();
    let uart = rp.uart[0].clone();
    uart.write_u32_atomic(UART_IBRD, 67, 0);
    uart.write_u32_atomic(UART_FBRD, 52, 0);
    assert_eq!(uart.baud_rate(), 115_207);
    uart.write_u32_atomic(UART_IBRD, 0, 0);
    uart.write_u32_atomic(UART_FBRD, 0, 0);
    assert_eq!(uart.baud_rate(), 0);
}
