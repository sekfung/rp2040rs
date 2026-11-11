use rp2040rs::rp2040::Rp2040;

const SYSINFO: u32 = 0x4000_0000;
const SYSCFG: u32 = 0x4000_4000;
const RESETS: u32 = 0x4000_c000;
const PSM: u32 = 0x4001_0000;
const IO_BANK0: u32 = 0x4001_4000;
const PADS_BANK0: u32 = 0x4001_c000;
const PADS_QSPI: u32 = 0x4002_0000;
const BUSCTRL: u32 = 0x4003_0000;
const TBMAN: u32 = 0x4006_c000;
const SIO: u32 = 0xd000_0000;

#[test]
fn sysinfo_and_tbman_identity_registers_are_implemented() {
    let rp = Rp2040::new();
    assert_eq!(rp.read_u32(SYSINFO), 0x1000_2927);
    assert_eq!(rp.read_u32(SYSINFO + 0x4), 2);
    assert_eq!(rp.read_u32(SYSINFO + 0x40), 0xe0c9_12e8);
    assert_eq!(rp.read_u32(TBMAN), 1);
}

#[test]
fn syscfg_nmi_mask_round_trips() {
    let rp = Rp2040::new();
    rp.write_u32(SYSCFG, 0x5a);
    assert_eq!(rp.read_u32(SYSCFG), 0x5a);
}

#[test]
fn resets_and_psm_keep_simple_state() {
    let rp = Rp2040::new();
    rp.write_u32(RESETS, 0x1ff_ffff);
    assert_eq!(rp.read_u32(RESETS), 0x1ff_ffff);
    rp.write_u32(PSM + 0x4, 0x1);
    assert_eq!(rp.read_u32(PSM + 0x4), 1);
    assert_eq!(rp.read_u32(PSM + 0xc) & 1, 0);
}

#[test]
fn busctrl_perf_select_and_counter_work() {
    let rp = Rp2040::new();
    rp.write_u32(BUSCTRL + 0x0c, 0x1f);
    assert_eq!(rp.read_u32(BUSCTRL + 0x0c), 0x1f);
    rp.write_u32(BUSCTRL + 0x08, 0x1234);
    assert_eq!(rp.read_u32(BUSCTRL + 0x08), 0);
}

#[test]
fn pads_and_io_bank0_round_trip_pin_state() {
    let rp = Rp2040::new();
    rp.write_u32(PADS_BANK0 + 0x4, 0b0110110);
    assert_eq!(rp.read_u32(PADS_BANK0 + 0x4), 0b0110110);
    rp.write_u32(PADS_QSPI + 0x4, 0b0100110);
    assert_eq!(rp.read_u32(PADS_QSPI + 0x4), 0b0100110);
    rp.write_u32(IO_BANK0 + 0x4, 0x1f);
    assert_eq!(rp.read_u32(IO_BANK0 + 0x4) & 0x1f, 0x1f);
}

#[test]
fn io_bank0_irq_registers_aggregate() {
    let rp = Rp2040::new();
    rp.write_u32(IO_BANK0 + 0x110, 0x1); // PROC0_INTF0 pin 0
    assert_eq!(rp.read_u32(IO_BANK0 + 0x120) & 0xf, 0x1);
}

#[test]
fn sio_interpolator_registers_are_wired() {
    let rp = Rp2040::new();
    rp.write_u32(SIO + 0x0ac, 31 << 10); // INTERP0_CTRL_LANE0: 32-bit mask
    rp.write_u32(SIO + 0x080, 10); // ACCUM0
    rp.write_u32(SIO + 0x088, 5); // BASE0
    assert_eq!(rp.read_u32(SIO + 0x0a0), 15); // PEEK_LANE0 = base0 + result0
}

#[test]
fn new_peripherals_honor_atomic_aliases() {
    let rp = Rp2040::new();

    rp.write_u32(PSM, 1);
    rp.write_u32(PSM | 0x2000, 4);
    assert_eq!(rp.read_u32(PSM), 5);

    rp.write_u32(RESETS, 1);
    rp.write_u32(RESETS | 0x2000, 2);
    assert_eq!(rp.read_u32(RESETS), 3);

    rp.write_u32(PADS_BANK0 + 4, 0b0110110);
    rp.write_u32((PADS_BANK0 + 4) | 0x2000, 1);
    assert_eq!(rp.read_u32(PADS_BANK0 + 4), 0b0110111);

    rp.write_u32(IO_BANK0 + 4, 0x1f);
    rp.write_u32((IO_BANK0 + 4) | 0x2000, 0x20);
    assert_eq!(rp.read_u32(IO_BANK0 + 4), 0x3f);

    rp.write_u32(SYSCFG, 1);
    rp.write_u32(SYSCFG | 0x2000, 2);
    assert_eq!(rp.read_u32(SYSCFG), 3);

    rp.write_u32(IO_BANK0 + 0x100, 1);
    rp.write_u32((IO_BANK0 + 0x100) | 0x2000, 2);
    assert_eq!(rp.read_u32(IO_BANK0 + 0x100) & 0xf, 3);
}

#[test]
fn interpolator_blend_uses_signed_js_difference() {
    let rp = Rp2040::new();
    rp.write_u32(SIO + 0x0b0, 7 << 10); // INTERP0_CTRL_LANE1
    rp.write_u32(SIO + 0x0ac, (7 << 10) | (1 << 21)); // mask 8 bits, blend
    rp.write_u32(SIO + 0x088, 100); // BASE0
    rp.write_u32(SIO + 0x08c, 0); // BASE1
    rp.write_u32(SIO + 0x084, 128); // ACCUM1 -> alpha 128
    assert_eq!(rp.read_u32(SIO + 0x0a4), 50); // PEEK_LANE1
}

#[test]
fn gpio_values_respect_pad_input_enable() {
    let rp = Rp2040::new();
    rp.gpio[0].set_input_value(true);
    assert_eq!(rp.gpio_values() & 1, 0);
    rp.write_u32(PADS_BANK0 + 4, 0b1110110);
    assert_eq!(rp.gpio_values() & 1, 1);
}
