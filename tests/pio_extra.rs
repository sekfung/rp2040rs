use std::rc::Rc;

use rp2040rs::clock::MockClock;
use rp2040rs::rp2040::Rp2040;
use rp2040rs::utils::pio_assembler::{
    pio_in, pio_irq, pio_jmp, pio_pull, pio_set, PIO_COND_ALWAYS, PIO_SRC_PINS,
};

const PIO0: u32 = 0x5020_0000;
const PIO1: u32 = 0x5030_0000;
const CTRL: u32 = PIO0;
const SM0_EXECCTRL: u32 = PIO0 + 0x0cc;
const SM0_SHIFTCTRL: u32 = PIO0 + 0x0d0;
const SM0_PINCTRL: u32 = PIO0 + 0x0dc;
const INSTR_MEM0: u32 = PIO0 + 0x048;
const INSTR_MEM1: u32 = PIO0 + 0x04c;
const TXF0: u32 = PIO0 + 0x010;
const RXF0: u32 = PIO0 + 0x020;
const IRQ: u32 = PIO0 + 0x030;
const INTR: u32 = PIO0 + 0x128;
const IRQ0_INTE: u32 = PIO0 + 0x12c;
const IRQ0_INTS: u32 = PIO0 + 0x134;

const SHIFTCTRL_AUTOPUSH: u32 = 1 << 16;
const SHIFTCTRL_IN_SHIFTDIR: u32 = 1 << 18;
const SHIFTCTRL_OUT_SHIFTDIR: u32 = 1 << 19;
const PUSH_THRESH_SHIFT: u32 = 20;
const EXECCTRL_WRAP_TOP_SHIFT: u32 = 12;
const INTR_SM0_TXNFULL: u32 = 1 << 4;
const INTR_IRQ0: u32 = 1 << 8;
const DBG_PADOUT: u32 = PIO0 + 0x03c;

#[test]
fn five_bit_sideset_drives_all_five_pins() {
    let rp = Rp2040::new();
    rp.write_u16(INSTR_MEM0, pio_set(1, 0, 0x1f)); // SET X, 0 with sideset 0x1f
    rp.write_u32(SM0_PINCTRL, 5 << 29); // SIDESET_COUNT=5, base=0
    rp.write_u32(CTRL, 1);
    // Execute the SET once; the following zero-filled instruction would clear
    // the side-set pins if the 1000-step burst kept running.
    rp.pio[0].step();

    assert_eq!(rp.read_u32(DBG_PADOUT) & 0x1f, 0x1f);
}

#[test]
fn pio_keeps_running_after_the_initial_burst() {
    let clock = Rc::new(MockClock::new());
    let rp = Rp2040::with_clock(clock.clone());

    rp.write_u16(INSTR_MEM0, pio_irq(false, false, 0, 0));
    rp.write_u16(INSTR_MEM1, pio_jmp(PIO_COND_ALWAYS, 0, 0));
    rp.write_u32(SM0_EXECCTRL, 1 << EXECCTRL_WRAP_TOP_SHIFT);
    rp.write_u32(CTRL, 1);

    assert_eq!(rp.read_u32(INTR) & INTR_IRQ0, INTR_IRQ0);
    rp.write_u32(IRQ, 1);
    assert_eq!(rp.read_u32(INTR) & INTR_IRQ0, 0);

    clock.advance(8); // one 1000-step burst at 125 MHz

    assert_eq!(rp.read_u32(INTR) & INTR_IRQ0, INTR_IRQ0);
}

#[test]
fn pio1_is_registered_at_its_upstream_base() {
    let rp = Rp2040::new();
    assert_eq!(rp.read_u32(PIO1), 0);
}

#[test]
fn blocking_pull_updates_txnfull_interrupt() {
    let rp = Rp2040::new();
    rp.write_u16(INSTR_MEM0, pio_pull(true, false, 0));
    rp.write_u32(SM0_EXECCTRL, 0);
    rp.write_u32(IRQ0_INTE, INTR_SM0_TXNFULL);

    for value in 0..4 {
        rp.write_u32(TXF0, value + 1);
    }
    assert_eq!(rp.read_u32(INTR) & INTR_SM0_TXNFULL, 0);

    rp.write_u32(CTRL, 1);

    assert_eq!(rp.read_u32(INTR) & INTR_SM0_TXNFULL, INTR_SM0_TXNFULL);
    assert_eq!(rp.read_u32(IRQ0_INTS) & INTR_SM0_TXNFULL, INTR_SM0_TXNFULL);
    assert_ne!(rp.core.pending_interrupts() & (1 << 7), 0);
}

#[test]
fn autopush_stall_does_not_reuse_stale_shift_data() {
    let rp = Rp2040::new();
    for pin in 0..8 {
        rp.gpio[pin].set_input_value(true);
        rp.write_u32(0x4001_c004 + (pin as u32) * 4, 0b1110110);
    }
    assert_eq!(rp.gpio_values() & 0xff, 0xff);

    rp.write_u16(INSTR_MEM0, pio_in(PIO_SRC_PINS, 8, 0));
    rp.write_u16(INSTR_MEM1, pio_jmp(PIO_COND_ALWAYS, 0, 0));
    rp.write_u32(
        SM0_SHIFTCTRL,
        SHIFTCTRL_AUTOPUSH
            | (8 << PUSH_THRESH_SHIFT)
            | SHIFTCTRL_IN_SHIFTDIR
            | SHIFTCTRL_OUT_SHIFTDIR,
    );
    rp.write_u32(SM0_PINCTRL, 0);
    rp.write_u32(SM0_EXECCTRL, 1 << EXECCTRL_WRAP_TOP_SHIFT);

    rp.write_u32(CTRL, 1);
    for _ in 0..4 {
        assert_eq!(rp.read_u32(RXF0), 0xff00_0000);
    }

    // The machine is now stalled on a full RX FIFO. Drain one slot, then
    // change the pins and run the next IN; it must not push stale 0xff data.
    rp.pio[0].step();
    assert_eq!(rp.read_u32(RXF0), 0xff00_0000);
    for pin in 0..8 {
        rp.gpio[pin].set_input_value(false);
    }
    rp.pio[0].step();

    assert_eq!(rp.read_u32(RXF0), 0);
}
