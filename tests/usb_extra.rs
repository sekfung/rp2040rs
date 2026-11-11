use std::cell::{Cell, RefCell};
use std::rc::Rc;

use rp2040rs::clock::MockClock;
use rp2040rs::rp2040::Rp2040;
use rp2040rs::usb::UsbCdc;

#[test]
fn cdc_constructs_and_queues_serial_bytes() {
    let rp = Rp2040::new();
    let cdc = UsbCdc::new(rp.usb_ctrl.clone());
    cdc.send_serial_byte(0x41);
    cdc.send_serial_byte(0x42);
    assert_eq!(cdc.tx_fifo_item_count(), 2);
}

#[test]
fn main_ctrl_write_fires_enable_without_touching_dpram() {
    let rp = Rp2040::new();
    let enabled = Rc::new(Cell::new(false));
    let flag = enabled.clone();
    rp.usb_ctrl
        .set_on_usb_enabled(Box::new(move || flag.set(true)));

    rp.write_u32(0x5011_0040, 1);

    assert!(enabled.get());
    assert_eq!(rp.usb_dpram.byte(0x40), 0);
}

#[test]
fn out_endpoint_completion_uses_out_control_register() {
    let clock = Rc::new(MockClock::new());
    let rp = Rp2040::with_clock(clock.clone());
    // Endpoint 2 OUT control: buffer offset 0x200; buffer-control length 4.
    rp.write_u32(0x5010_0014, 0x200);
    rp.write_u32(0x5010_0094, 4);
    rp.usb_ctrl.endpoint_read_done(2, &[0xaa, 0xbb], 0);
    clock.advance(10);

    let control = rp.read_u32(0x5010_0094);
    assert_eq!(control & 0x3ff, 2);
    assert_ne!(control & (1 << 15), 0);
    assert_eq!(rp.usb_dpram.byte(0x200), 0xaa);
    assert_eq!(rp.usb_dpram.byte(0x201), 0xbb);
}

#[test]
fn dpram_update_clears_available_and_full_and_dispatches_bytes() {
    let clock = Rc::new(MockClock::new());
    let rp = Rp2040::with_clock(clock.clone());
    let written = Rc::new(RefCell::new(Vec::new()));
    let sink = written.clone();
    rp.usb_ctrl
        .set_on_endpoint_write(Box::new(move |_endpoint, buffer| {
            sink.borrow_mut().extend_from_slice(&buffer);
        }));
    rp.usb_dpram.set_byte(0x100, 0x12);
    rp.usb_dpram.set_byte(0x101, 0x34);

    // Endpoint 0 IN transfer: AVAILABLE | FULL | length 2.
    rp.write_u32(0x5010_0080, (1 << 10) | (1 << 15) | 2);

    let control = rp.read_u32(0x5010_0080);
    assert_eq!(control & (1 << 10), 0);
    assert_eq!(control & (1 << 15), 0);
    clock.advance(10);
    assert_eq!(&*written.borrow(), &[0x12, 0x34]);
}

#[test]
fn controller_interrupt_registers_do_not_alias_dpram() {
    let rp = Rp2040::new();
    rp.usb_dpram.set_byte(0x90, 0x55);
    rp.write_u32(0x5011_0090, 0x2);
    assert_eq!(rp.read_u32(0x5011_0090), 0x2);
    assert_eq!(rp.usb_dpram.byte(0x90), 0x55);
}

#[test]
fn double_buffer_in_completion_clears_buffer1_available_and_full() {
    let clock = Rc::new(MockClock::new());
    let rp = Rp2040::with_clock(clock.clone());
    // Endpoint 2 IN control: double buffer, buffer offset 0x200.
    rp.write_u32(0x5010_0010, (1 << 30) | 0x200);
    // Buffer 0 and buffer 1 both available; buffer 1 length 1, data at 0x240.
    let value = (1 << 10) | (1 << 15) | 1 | ((1 << 10) << 16) | ((1 << 15) << 16) | (1 << 16);
    rp.write_u32(0x5010_0090, value);

    let control = rp.read_u32(0x5010_0090);
    assert_eq!(control & ((1 << 10) << 16), 0);
    assert_eq!(control & ((1 << 15) << 16), 0);
}

#[test]
fn usb_atomic_aliases_merge_non_w1c_registers() {
    let rp = Rp2040::new();

    rp.write_u32(0x5011_0040, 1); // MAIN_CTRL.CONTROLLER_EN
    rp.write_u32(0x5011_3040, 1); // CLR alias
    assert_eq!(rp.read_u32(0x5011_0040) & 1, 0);

    rp.write_u32(0x5011_004c, 0x200); // SIE_CTRL bit 9
    rp.write_u32(0x5011_204c, 0x100); // SET alias, bit 8
    assert_eq!(rp.read_u32(0x5011_004c), 0x300);

    rp.write_u32(0x5011_0004, 1); // ADDR_ENDP1
    rp.write_u32(0x5011_2004, 2); // SET alias
    assert_eq!(rp.read_u32(0x5011_0004), 3);
}
