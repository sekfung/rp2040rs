use std::rc::Rc;

use rp2040rs::clock::SimulationClock;
use rp2040rs::rp2040::Rp2040;

const PWM_BASE: u32 = 0x4005_0000;
const CH0_CSR: u32 = PWM_BASE;
const CH0_DIV: u32 = PWM_BASE + 0x04;
const CH0_CTR: u32 = PWM_BASE + 0x08;
const CH0_CC: u32 = PWM_BASE + 0x0c;
const CH0_TOP: u32 = PWM_BASE + 0x10;
const PWM_INTR: u32 = PWM_BASE + 0xa4;
const PWM_INTE: u32 = PWM_BASE + 0xa8;
const CSR_EN: u32 = 1;

#[test]
fn pwm_wraps_periodically_and_drives_gpio() {
    let clock = Rc::new(SimulationClock::new());
    let rp = Rp2040::with_clock(clock.clone());
    rp.write_u32(0x4001_4004, 4); // IO_BANK0 GPIO0 FUNCSEL = PWM
    rp.write_u32(0x4001_400c, 4); // IO_BANK0 GPIO1 FUNCSEL = PWM
    rp.write_u32(CH0_TOP, 4);
    rp.write_u32(CH0_CC, 2);
    rp.write_u32(CH0_CSR, CSR_EN);

    clock.tick(40.0);
    assert_eq!(rp.read_u32(PWM_INTR) & 1, 1);
    assert_eq!(rp.read_u32(CH0_CTR), 0);
    assert!(rp.gpio[0].raw_output_enable());
    assert!(rp.gpio[0].raw_output_value());
    assert!(rp.gpio[1].raw_output_enable());

    clock.tick(16.0);
    assert!(!rp.gpio[0].raw_output_value());
    assert!(!rp.gpio[1].raw_output_value());
}

#[test]
fn pwm_divider_scales_the_wrap_period() {
    let clock = Rc::new(SimulationClock::new());
    let rp = Rp2040::with_clock(clock.clone());
    rp.write_u32(CH0_DIV, 2 << 4);
    rp.write_u32(CH0_TOP, 4);
    rp.write_u32(CH0_CSR, CSR_EN);

    clock.tick(79.0);
    assert_eq!(rp.read_u32(PWM_INTR) & 1, 0);
    clock.tick(1.0);
    assert_eq!(rp.read_u32(PWM_INTR) & 1, 1);
}

#[test]
fn pwm_cc_is_double_buffered_until_wrap() {
    let clock = Rc::new(SimulationClock::new());
    let rp = Rp2040::with_clock(clock.clone());
    rp.write_u32(CH0_TOP, 2);
    rp.write_u32(CH0_CC, 1);
    rp.write_u32(CH0_CSR, CSR_EN);
    rp.write_u32(CH0_CC, 2);
    assert_eq!(rp.read_u32(CH0_CC), 1);
    clock.tick(24.0);
    assert_eq!(rp.read_u32(CH0_CC), 2);
}

#[test]
fn sio_output_and_oe_reach_gpio_pad_state() {
    let rp = Rp2040::new();
    rp.write_u32(0x4001_4004, 5); // IO_BANK0 GPIO0 FUNCSEL = SIO
    rp.write_u32(0x4001_c000 + 0x04, 0x40); // PADS_BANK0 GPIO0 input enable
    rp.write_u32(0xd000_0000 + 0x014, 0x1); // SIO GPIO_OUT_SET
    rp.write_u32(0xd000_0000 + 0x024, 0x1); // SIO GPIO_OE_SET
    assert!(rp.gpio[0].raw_output_value());
    assert!(rp.gpio[0].raw_output_enable());
    assert_eq!(rp.read_u32(0xd000_0000 + 0x004) & 1, 1);
}

#[test]
fn pwm_and_sio_do_not_clobber_each_other() {
    let clock = Rc::new(SimulationClock::new());
    let rp = Rp2040::with_clock(clock.clone());
    rp.write_u32(0x4001_4004, 4); // GPIO0 = PWM
    rp.write_u32(0x4001_40cc, 5); // GPIO25 = SIO
    rp.write_u32(CH0_TOP, 4);
    rp.write_u32(CH0_CC, 2);
    rp.write_u32(CH0_CSR, CSR_EN);
    clock.tick(40.0);
    assert!(rp.gpio[0].raw_output_enable());

    rp.write_u32(0xd000_0000 + 0x014, 1 << 25); // SIO GPIO_OUT_SET GPIO25
    rp.write_u32(0xd000_0000 + 0x024, 1 << 25); // SIO GPIO_OE_SET GPIO25
    assert!(rp.gpio[0].raw_output_enable());
    assert!(rp.gpio[25].raw_output_enable());
    assert!(rp.gpio[25].raw_output_value());
}

#[test]
fn iobank0_funcsel_selects_the_active_output_source() {
    let rp = Rp2040::new();
    rp.write_u32(0x4001_4004, 5); // GPIO0 = SIO
    rp.write_u32(0xd000_0000 + 0x014, 1);
    rp.write_u32(0xd000_0000 + 0x024, 1);
    assert!(rp.gpio[0].raw_output_value());
    assert!(rp.gpio[0].raw_output_enable());

    rp.write_u32(0x4001_4004, 4); // switch to PWM, which has not driven yet
    assert!(!rp.gpio[0].raw_output_value());
    assert!(!rp.gpio[0].raw_output_enable());

    rp.write_u32(0x4001_4004, 5); // back to SIO
    assert!(rp.gpio[0].raw_output_value());
    assert!(rp.gpio[0].raw_output_enable());
}

#[test]
fn pwm_polarity_inverts_a_and_b_outputs() {
    let clock = Rc::new(SimulationClock::new());
    let rp = Rp2040::with_clock(clock.clone());
    rp.write_u32(0x4001_4004, 4);
    rp.write_u32(0x4001_400c, 4);
    rp.write_u32(CH0_TOP, 4);
    rp.write_u32(CH0_CC, 2);
    rp.write_u32(CH0_CSR, CSR_EN | (1 << 2)); // A inverted
    clock.tick(40.0);
    assert!(!rp.gpio[0].raw_output_value());
    assert!(!rp.gpio[1].raw_output_value());
}

#[test]
fn pwm_divmode_gates_b_pin_direction() {
    let clock = Rc::new(SimulationClock::new());
    let rp = Rp2040::with_clock(clock.clone());
    rp.write_u32(0x4001_4004, 4);
    rp.write_u32(0x4001_400c, 4);
    rp.write_u32(CH0_TOP, 4);
    rp.write_u32(CH0_CSR, CSR_EN | (1 << 4)); // BGated
    assert!(rp.gpio[0].raw_output_enable());
    assert!(!rp.gpio[1].raw_output_enable());
    rp.write_u32(CH0_CSR, CSR_EN); // FreeRunning
    assert!(rp.gpio[1].raw_output_enable());
}

#[test]
fn pwm_phase_correct_counts_down_after_top() {
    let clock = Rc::new(SimulationClock::new());
    let rp = Rp2040::with_clock(clock.clone());
    rp.write_u32(CH0_TOP, 4);
    rp.write_u32(CH0_CSR, CSR_EN | (1 << 1)); // PH_CORRECT
    clock.tick(8.0 * 8.0);
    assert_eq!(rp.read_u32(CH0_CTR), 1);
    clock.tick(8.0 * 3.0);
    assert_eq!(rp.read_u32(CH0_CTR), 1);
    assert_eq!(rp.read_u32(PWM_INTR) & 1, 1);
}

#[test]
fn pwm_schedules_only_observable_events_and_reads_intermediate_counts() {
    let clock = Rc::new(SimulationClock::new());
    let rp = Rp2040::with_clock(clock.clone());
    rp.write_u32(CH0_TOP, 999);
    rp.write_u32(CH0_CC, (700 << 16) | 300);
    rp.write_u32(CH0_CSR, CSR_EN);

    assert_eq!(clock.nanos_to_next_alarm(), Some(2400.0));
    clock.tick(2399.0);
    assert_eq!(rp.read_u32(CH0_CTR), 299);
    clock.tick(1.0);
    assert_eq!(rp.read_u32(CH0_CTR), 300);
    assert_eq!(clock.nanos_to_next_alarm(), Some(3200.0));
    clock.tick(3200.0);
    assert_eq!(rp.read_u32(CH0_CTR), 700);
    clock.tick(2400.0);
    assert_eq!(rp.read_u32(CH0_CTR), 0);
    assert_eq!(rp.read_u32(PWM_INTR) & 1, 1);
}

#[test]
fn pwm_divider_write_preserves_the_next_count_and_changes_following_counts() {
    let clock = Rc::new(SimulationClock::new());
    let rp = Rp2040::with_clock(clock.clone());
    rp.write_u32(CH0_TOP, 500);
    rp.write_u32(CH0_CSR, CSR_EN);
    clock.tick(35.0);
    assert_eq!(rp.read_u32(CH0_CTR), 4);
    rp.write_u32(CH0_DIV, 0x28); // 2.5 cycles = 20 ns
    clock.tick(5.0);
    assert_eq!(rp.read_u32(CH0_CTR), 5);
    clock.tick(19.0);
    assert_eq!(rp.read_u32(CH0_CTR), 5);
    clock.tick(1.0);
    assert_eq!(rp.read_u32(CH0_CTR), 6);

    rp.write_u32(CH0_CSR, 0);
    clock.tick(1_000.0);
    assert_eq!(rp.read_u32(CH0_CTR), 6);
    rp.write_u32(CH0_CSR, CSR_EN);
    clock.tick(20.0);
    assert_eq!(rp.read_u32(CH0_CTR), 7);
}

#[test]
fn pwm_reasserts_the_pending_irq_while_the_wrap_flag_is_set() {
    const PWM_IRQ: u32 = 1 << 4;

    let clock = Rc::new(SimulationClock::new());
    let rp = Rp2040::with_clock(clock.clone());
    rp.write_u32(CH0_TOP, 99);
    rp.write_u32(PWM_INTE, 1);
    rp.write_u32(CH0_CSR, CSR_EN);
    clock.tick(800.0);
    assert_eq!(rp.core.pending_interrupts() & PWM_IRQ, PWM_IRQ);

    rp.core
        .set_pending_interrupts(rp.core.pending_interrupts() & !PWM_IRQ);
    assert_eq!(rp.core.pending_interrupts() & PWM_IRQ, 0);
    clock.tick(8.0);
    assert_eq!(rp.core.pending_interrupts() & PWM_IRQ, PWM_IRQ);

    rp.write_u32(PWM_INTR, 1);
    assert_eq!(rp.core.pending_interrupts() & PWM_IRQ, 0);
    assert_eq!(clock.nanos_to_next_alarm(), Some(792.0));
}

#[test]
fn pwm_running_clock_change_preserves_count_and_retimes_compare() {
    let clock = Rc::new(SimulationClock::new());
    let rp = Rp2040::with_clock(clock.clone());
    rp.write_u32(CH0_TOP, 999);
    rp.write_u32(CH0_CC, 10);
    rp.write_u32(CH0_CSR, CSR_EN);
    clock.tick(35.0);
    rp.pwm.set_clock_freq(100_000_000.0);
    assert_eq!(rp.read_u32(CH0_CTR), 4);
    assert_eq!(clock.nanos_to_next_alarm(), Some(60.0));
    clock.tick(59.0);
    assert_eq!(rp.read_u32(CH0_CTR), 9);
    clock.tick(1.0);
    assert_eq!(rp.read_u32(CH0_CTR), 10);
}
