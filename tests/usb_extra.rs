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
