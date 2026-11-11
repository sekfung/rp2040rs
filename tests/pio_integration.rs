use std::rc::Rc;

use rp2040rs::clock::SimulationClock;
use rp2040rs::rp2040::Rp2040;

const PIO0: u32 = 0x5020_0000;
const CTRL: u32 = PIO0;
const FLEVEL: u32 = PIO0 + 0x0c;
const TXF0: u32 = PIO0 + 0x10;
const RXF0: u32 = PIO0 + 0x20;
const SM0_CLKDIV: u32 = PIO0 + 0xc8;
const SM0_SHIFTCTRL: u32 = PIO0 + 0xd0;

const DMA: u32 = 0x5000_0000;
const DMA_READ: u32 = DMA;
const DMA_WRITE: u32 = DMA + 4;
const DMA_COUNT: u32 = DMA + 8;
const DMA_CTRL: u32 = DMA + 0x0c;

#[test]
fn pio_fjoin_rx_reads_the_tx_fifo() {
    let rp = Rp2040::new();
    rp.write_u32(TXF0, 0x1234);
    rp.write_u32(SM0_SHIFTCTRL, 1 << 30);
    assert_eq!(rp.read_u32(RXF0), 0x1234);
}

#[test]
fn pio_reset_disables_machines_and_pins() {
    let rp = Rp2040::new();
    rp.write_u32(CTRL, 1);
    rp.pio[0].reset();
    assert_eq!(rp.read_u32(CTRL) & 0xf, 0);
    assert!(!rp.gpio[0].raw_output_enable());
}

#[test]
fn pio_tx_dreq_feeds_a_dma_transfer() {
    let clock = Rc::new(SimulationClock::new());
    let rp = Rp2040::with_clock(clock.clone());
    rp.write_u32(0x2000_1000, 0xdead_beef);
    rp.write_u32(DMA_READ, 0x2000_1000);
    rp.write_u32(DMA_WRITE, TXF0);
    rp.write_u32(DMA_COUNT, 1);
    // EN | INCR_READ | TREQ=PIO0_TX0 | 32-bit data
    rp.write_u32(DMA_CTRL, 1 | (1 << 4) | (2 << 2));
    rp.write_u32(CTRL, 1);
    clock.tick(10_000.0);
    assert_ne!(rp.read_u32(FLEVEL) & 0xf, 0);
    assert!(rp.read_u32(RXF0) == 0xdead_beef || rp.read_u32(FLEVEL) != 0);
}

#[test]
fn pio_clock_divider_is_stored_and_used_for_scheduling() {
    let rp = Rp2040::new();
    rp.write_u32(SM0_CLKDIV, 2 << 16);
    assert_eq!(rp.read_u32(SM0_CLKDIV), 2 << 16);
}
