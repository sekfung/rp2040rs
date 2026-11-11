use rp2040rs::peripherals::Peripheral;
use rp2040rs::rp2040::Rp2040;
use std::cell::Cell;
use std::rc::Rc;

use rp2040rs::clock::MockClock;

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
const SPI0: u32 = 0x4003_c000;
const I2C0: u32 = 0x4004_4000;
const WATCHDOG: u32 = 0x4005_8000;

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
fn spi_enabling_sse_kicks_queued_transmit() {
    let rp = Rp2040::new();
    rp.spi[0].set_on_transmit(Box::new(|byte| byte.wrapping_add(1)));
    rp.write_u32(SPI0, 7); // 8-bit data
    rp.write_u32(SPI0 + 0x04, 0); // SSE disabled
    rp.write_u32(SPI0 + 0x08, 0x41);
    assert_eq!(rp.read_u32(SPI0 + 0x08), 0);
    rp.write_u32(SPI0 + 0x04, 2); // enable SSE
    assert_eq!(rp.read_u32(SPI0 + 0x08), 0x42);
}

#[test]
fn watchdog_trigger_callback_can_read_registers() {
    let rp = Rp2040::new();
    let rp2 = rp.clone();
    rp.watchdog.set_on_watchdog_trigger(Box::new(move || {
        let _ = rp2.read_u32(WATCHDOG + 0x04);
    }));
    rp.write_u32(WATCHDOG, (1 << 31) | (1 << 30));
    assert_eq!(rp.read_u32(WATCHDOG + 0x08), 2); // REASON_FORCE
}

#[test]
fn i2c_user_abort_raises_tx_abrt_and_stops_command_chain() {
    let rp = Rp2040::new();
    let i2c = rp.i2c[0].clone();
    let i2c2 = i2c.clone();
    i2c.set_on_start(Box::new(move || {
        i2c2.write_u32_atomic(0x6c, 2, 0); // IC_ENABLE.ABORT
    }));
    rp.write_u32(I2C0 + 0x6c, 1); // enable
    rp.write_u32(I2C0 + 0x04, 0x55); // target
    rp.write_u32(I2C0 + 0x10, 0x100 | 0x200); // read + STOP
    assert_ne!(rp.read_u32(I2C0 + 0x34) & (1 << 6), 0); // TX_ABRT
    assert_ne!(rp.read_u32(I2C0 + 0x80) & (1 << 16), 0); // ABRT_USER_ABRT
    assert_eq!(rp.read_u32(I2C0 + 0x78), 0); // RXFLR
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

const ADC: u32 = 0x4004_c000;
const SSI: u32 = 0x1800_0000;
const RTC: u32 = 0x4005_c000;

const ADC_CS: u32 = ADC;
const ADC_FCS: u32 = ADC + 0x08;
const ADC_RESULT: u32 = ADC + 0x04;
const I2C_DATA_CMD: u32 = I2C0 + 0x10;
const I2C_RAW_INTR: u32 = I2C0 + 0x34;
const I2C_INTR_MASK: u32 = I2C0 + 0x30;
const I2C_ENABLE: u32 = I2C0 + 0x6c;
const I2C_RXFLR: u32 = I2C0 + 0x78;
const I2C_CLR_INTR: u32 = I2C0 + 0x40;
const SPI_DR: u32 = SPI0 + 0x08;
const SPI_IMSC: u32 = SPI0 + 0x14;
const SPI_MIS: u32 = SPI0 + 0x1c;

#[test]
fn adc_conversion_reaches_result_and_fifo() {
    let clock = Rc::new(MockClock::new());
    let rp = Rp2040::with_clock(clock.clone());
    rp.adc.set_channel_value(0, 0x123);
    rp.write_u32(ADC + 0x08, 1); // FCS.EN
    rp.write_u32(ADC, 1 | (1 << 2)); // CS.EN | START_ONE
    clock.advance(3);
    assert_eq!(rp.read_u32(ADC + 0x04), 0x123);
    assert_eq!(rp.read_u32(ADC + 0x0c) & 0xfff, 0x123);
}

#[test]
fn spi_fifo_status_and_identifiers() {
    let rp = Rp2040::new();
    rp.write_u32(SPI0, 7); // SSPCR0 DSS=7 (8 bits)
    rp.write_u32(SPI0 + 0x04, 2); // SSE
    assert_eq!(rp.read_u32(SPI0 + 0x0c) & 1, 1); // TFE
    rp.write_u32(SPI0 + 0x08, 0x5a);
    assert_eq!(rp.read_u32(SPI0 + 0x08), 0); // default response
    assert_eq!(rp.read_u32(SPI0 + 0xfe0), 0x22);
}

#[test]
fn ssi_identity_and_data_register() {
    let rp = Rp2040::new();
    assert_eq!(rp.read_u32(SSI + 0x58), 0x5153_5049);
    assert_eq!(rp.read_u32(SSI + 0x5c), 0x3430_312a);
    rp.write_u32(SSI + 0x60, 5);
    assert_eq!(rp.read_u32(SSI + 0x60), 0);
}

#[test]
fn i2c_target_enable_and_status() {
    let rp = Rp2040::new();
    rp.write_u32(I2C0 + 0x04, 0x42);
    assert_eq!(rp.read_u32(I2C0 + 0x04), 0x42);
    rp.write_u32(I2C0 + 0x6c, 1);
    assert_eq!(rp.read_u32(I2C0 + 0x6c) & 1, 1);
    assert_eq!(rp.read_u32(I2C0 + 0x9c) & 1, 1);
    assert_eq!(rp.read_u32(I2C0 + 0xfc), 0x4457_0140);
}

#[test]
fn rtc_load_and_read_produces_setup_date() {
    let rp = Rp2040::new();
    rp.write_u32(RTC + 0x04, (2024 << 12) | (5 << 8) | 17);
    rp.write_u32(RTC + 0x08, (12 << 16) | (34 << 8) | 56);
    rp.write_u32(RTC + 0x0c, 0x10 | 0x1); // LOAD | ENABLE
    assert_eq!(rp.read_u32(RTC + 0x18), (2024 << 12) | (5 << 8) | 17);
    assert_eq!(
        rp.read_u32(RTC + 0x1c) & 0x00ff_ffff,
        (12 << 16) | (34 << 8) | 56
    );
}

#[test]
fn watchdog_scratch_and_forced_trigger() {
    let rp = Rp2040::new();
    let fired = Rc::new(Cell::new(false));
    let flag = fired.clone();
    rp.watchdog
        .set_on_watchdog_trigger(Box::new(move || flag.set(true)));
    rp.write_u32(WATCHDOG + 0x0c, 0xdead_beef);
    assert_eq!(rp.read_u32(WATCHDOG + 0x0c), 0xdead_beef);
    rp.write_u32(WATCHDOG, 1 << 31); // TRIGGER
    assert!(fired.get());
    assert_eq!(rp.read_u32(WATCHDOG + 0x08) & 2, 2);
}

#[test]
fn adc_honors_ainsel_channel() {
    let clock = Rc::new(MockClock::new());
    let rp = Rp2040::with_clock(clock.clone());
    rp.adc.set_channel_value(1, 0x321);
    rp.write_u32(ADC_FCS, 1);
    rp.write_u32(ADC_CS, 1 | (1 << 2) | (1 << 12));
    clock.advance(3);
    assert_eq!(rp.read_u32(ADC_RESULT), 0x321);
}

#[test]
fn spi_normal_write_does_not_consume_rx_fifo() {
    let rp = Rp2040::new();
    rp.write_u32(SPI0, 7);
    rp.write_u32(SPI0 + 0x04, 2);
    rp.spi[0].set_on_transmit(Box::new(|value| value));
    rp.write_u32(SPI_DR, 0xaa);
    rp.write_u32(SPI_DR, 0xbb);
    assert_eq!(rp.read_u32(SPI_DR) & 0xff, 0xaa);
    assert_eq!(rp.read_u32(SPI_DR) & 0xff, 0xbb);
}

#[test]
fn i2c_normal_write_does_not_set_rx_under() {
    let rp = Rp2040::new();
    rp.write_u32(I2C_ENABLE, 1);
    rp.write_u32(I2C_DATA_CMD, 0x55);
    assert_eq!(rp.read_u32(I2C_RAW_INTR) & 1, 0);
}

#[test]
fn i2c_master_callbacks_drive_write_read_and_stop() {
    let rp = Rp2040::new();
    let started = Rc::new(Cell::new(false));
    let connected = Rc::new(Cell::new(false));
    let wrote = Rc::new(Cell::new(false));
    let stopped = Rc::new(Cell::new(false));
    let s = started.clone();
    let c = connected.clone();
    let w = wrote.clone();
    let p = stopped.clone();
    rp.i2c[0].set_on_start(Box::new(move || s.set(true)));
    rp.i2c[0].set_on_connect(Box::new(move |_addr, _read| {
        c.set(true);
        true
    }));
    rp.i2c[0].set_on_write(Box::new(move |_value| {
        w.set(true);
        true
    }));
    rp.i2c[0].set_on_read(Box::new(|| 0x5a));
    rp.i2c[0].set_on_stop(Box::new(move || p.set(true)));

    rp.write_u32(I2C_INTR_MASK, 0xffff);
    rp.write_u32(I2C_ENABLE, 1);
    rp.write_u32(I2C_DATA_CMD, 0x33); // write 0x33
    rp.write_u32(I2C_DATA_CMD, (1 << 8) | (1 << 9)); // read + STOP

    assert!(started.get());
    assert!(connected.get());
    assert!(wrote.get());
    assert!(stopped.get());
    assert_eq!(rp.read_u32(I2C_RAW_INTR) & (1 << 2), 1 << 2);
    assert_eq!(rp.read_u32(I2C_RXFLR), 1);
    assert_eq!(rp.read_u32(I2C_DATA_CMD) & 0xff, 0x5a);
    let _ = rp.read_u32(I2C_CLR_INTR);
    assert_eq!(rp.read_u32(I2C_RAW_INTR), 0);
}

#[test]
fn i2c_callback_can_queue_a_chained_read() {
    let rp = Rp2040::new();
    let i2c = rp.i2c[0].clone();
    let reads = Rc::new(Cell::new(0u32));
    let reads_cb = reads.clone();
    let i2c_cb = i2c.clone();
    i2c.set_on_connect(Box::new(|_addr, _read| true));
    i2c.set_on_read(Box::new(move || {
        let n = reads_cb.get();
        reads_cb.set(n + 1);
        if n == 0 {
            i2c_cb.write_u32_atomic(0x10, 1 << 8, 0);
            0x11
        } else {
            0x22
        }
    }));

    rp.write_u32(I2C_INTR_MASK, 0xffff);
    rp.write_u32(I2C_ENABLE, 1);
    rp.write_u32(I2C_DATA_CMD, (1 << 8) | (1 << 9));

    assert_eq!(reads.get(), 2);
    assert_eq!(rp.read_u32(I2C_RXFLR), 2);
    let first = rp.read_u32(I2C_DATA_CMD);
    let second = rp.read_u32(I2C_DATA_CMD);
    assert_eq!(first & 0xff, 0x11);
    assert_eq!(second & 0xff, 0x22);
    assert_eq!(first & (1 << 10), 1 << 10);
    assert_eq!(second & (1 << 10), 1 << 10);
}

#[test]
fn spi_interrupt_deasserts_when_fifo_status_clears() {
    let rp = Rp2040::new();
    rp.write_u32(SPI_IMSC, 1 << 3);
    for _ in 0..5 {
        rp.write_u32(SPI_DR, 0);
    }
    assert_eq!(rp.read_u32(SPI_MIS) & (1 << 3), 0);
}

#[test]
fn watchdog_counts_down_and_honors_atomic_aliases() {
    let clock = Rc::new(MockClock::new());
    let rp = Rp2040::with_clock(clock.clone());
    rp.write_u32(WATCHDOG + 0x04, 1000);
    rp.write_u32(WATCHDOG, 1 << 30);
    clock.advance(100);
    let counter = rp.read_u32(WATCHDOG) & 0x00ff_ffff;
    assert!(counter < 1000);
    rp.write_u32(WATCHDOG + 0x0c, 1);
    rp.write_u32((WATCHDOG + 0x0c) | 0x2000, 2);
    assert_eq!(rp.read_u32(WATCHDOG + 0x0c), 3);
}
