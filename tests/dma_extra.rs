use std::rc::Rc;

use rp2040rs::clock::MockClock;
use rp2040rs::peripherals::DreqChannel;
use rp2040rs::rp2040::Rp2040;

const DMA_BASE: u32 = 0x5000_0000;
const CH0_READ_ADDR: u32 = DMA_BASE;
const CH0_WRITE_ADDR: u32 = DMA_BASE + 0x004;
const CH0_TRANS_COUNT: u32 = DMA_BASE + 0x008;
const CH0_CTRL_TRIG: u32 = DMA_BASE + 0x00c;
const CH0_AL1_TRANS_COUNT_TRIG: u32 = DMA_BASE + 0x01c;
const INTR: u32 = DMA_BASE + 0x400;
const INTE0: u32 = DMA_BASE + 0x404;
const TIMER0: u32 = DMA_BASE + 0x420;

const EN: u32 = 1 << 0;
const DATA_SIZE_SHIFT: u32 = 2;
const INCR_WRITE: u32 = 1 << 5;
const INCR_READ: u32 = 1 << 4;
const TREQ_SEL_SHIFT: u32 = 15;
const IRQ_QUIET: u32 = 1 << 21;
const TREQ_TIMER0: u32 = 0x3b;
const UART0_TX: u32 = DreqChannel::Uart0Tx as u32;

#[test]
fn dreq_wakes_the_channel_waiting_on_that_trigger() {
    let clock = Rc::new(MockClock::new());
    let rp = Rp2040::with_clock(clock.clone());
    rp.write_u32(0x2000_0000, 0x1234_5678);
    rp.write_u32(CH0_READ_ADDR, 0x2000_0000);
    rp.write_u32(CH0_WRITE_ADDR, 0x2000_0100);
    rp.write_u32(CH0_TRANS_COUNT, 1);
    rp.write_u32(
        CH0_CTRL_TRIG,
        EN | INCR_READ | INCR_WRITE | (UART0_TX << TREQ_SEL_SHIFT) | (2 << DATA_SIZE_SHIFT),
    );

    clock.advance(10);
    assert_eq!(rp.read_u32(0x2000_0100), 0);

    rp.dma.set_dreq(DreqChannel::Uart0Tx);
    clock.advance(1);

    assert_eq!(rp.read_u32(0x2000_0100), 0x1234_5678);
}

#[test]
fn dma_general_registers_honor_atomic_aliases() {
    let rp = Rp2040::new();
    rp.write_u32(INTE0, 1);
    rp.write_u32(INTE0 | 0x2000, 2);
    assert_eq!(rp.read_u32(INTE0), 3);

    rp.write_u32(TIMER0, 0x5);
    rp.write_u32(TIMER0 | 0x1000, 3);
    assert_eq!(rp.read_u32(TIMER0), 0x6);
}

#[test]
fn null_trigger_raises_irq_quiet_from_any_alias() {
    let rp = Rp2040::new();
    rp.write_u32(CH0_CTRL_TRIG, EN | IRQ_QUIET);
    rp.write_u32(CH0_AL1_TRANS_COUNT_TRIG, 0);
    assert_eq!(rp.read_u32(INTR) & 1, 1);
}

#[test]
fn non_ctrl_writes_do_not_rearm_a_paced_transfer() {
    let clock = Rc::new(MockClock::new());
    let rp = Rp2040::with_clock(clock.clone());
    rp.write_u32(0x2000_0000, 0xdead_beef);
    rp.write_u32(TIMER0, (1000 << 16) | 1);
    rp.write_u32(CH0_READ_ADDR, 0x2000_0000);
    rp.write_u32(CH0_WRITE_ADDR, 0x2000_0100);
    rp.write_u32(CH0_TRANS_COUNT, 1);
    rp.write_u32(
        CH0_CTRL_TRIG,
        EN | INCR_READ | INCR_WRITE | (TREQ_TIMER0 << TREQ_SEL_SHIFT) | (2 << DATA_SIZE_SHIFT),
    );

    clock.advance(4_000);
    rp.write_u32(CH0_READ_ADDR, 0x2000_0000);
    clock.advance(4_000);

    assert_eq!(rp.read_u32(0x2000_0100), 0xdead_beef);
}
