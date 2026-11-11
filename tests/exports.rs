//! Compile-time audit of the `rp2040js` `src/index.ts` public surface.

use rp2040rs::{
    create_setup_packet, get_descriptor_packet, parse_setup_packet, set_device_address_packet,
    set_device_configuration_packet, BasePeripheral, ClockListener, ConsoleLogger, DataDirection,
    DescriptorType, Fifo, GdbConnection, GdbServer, GpioPin, GpioPinState, I2c, I2cMode, I2cSpeed,
    ISetupPacketParams, LogLevel, Logger, Peripheral, Pio, Rp2040, SetupRecipient, SetupRequest,
    SetupType, Simulator, StandardRequest, StateMachine, UsbCdc, UsbController, UsbDescriptorType,
    UsbDevice, UsbTransferResult, UsbTransferStatus,
};

#[test]
fn all_upstream_index_exports_are_available() {
    fn assert_type<T>() {}
    assert_type::<BasePeripheral>();
    assert_type::<ConsoleLogger>();
    assert_type::<Fifo>();
    assert_type::<GdbConnection>();
    assert_type::<GdbServer>();
    assert_type::<GpioPin>();
    assert_type::<GpioPinState>();
    assert_type::<I2c>();
    assert_type::<I2cMode>();
    assert_type::<I2cSpeed>();
    assert_type::<Pio>();
    assert_type::<Rp2040>();
    assert_type::<Simulator>();
    assert_type::<StateMachine>();
    assert_type::<UsbCdc>();
    assert_type::<UsbController>();
    assert_type::<UsbDescriptorType>();
    assert_type::<UsbTransferResult>();
    assert_type::<UsbTransferStatus>();
    assert_type::<DataDirection>();
    assert_type::<DescriptorType>();
    assert_type::<SetupRecipient>();
    assert_type::<SetupRequest>();
    assert_type::<SetupType>();
    assert_type::<ISetupPacketParams>();
    assert_type::<StandardRequest>();
    assert_type::<Box<dyn Logger>>();
    assert_type::<Box<dyn Peripheral>>();
    assert_type::<ClockListener>();
    assert_type::<Option<&dyn UsbDevice>>();
    let _ = LogLevel::Debug;
    let _ = create_setup_packet;
    let _ = get_descriptor_packet;
    let _ = parse_setup_packet;
    let _ = set_device_address_packet;
    let _ = set_device_configuration_packet;
}
