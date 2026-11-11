pub mod cdc;
pub mod interfaces;
pub mod setup;
pub mod usb_device;

pub use cdc::{extract_endpoint_numbers, EndpointNumbers, UsbCdc};
pub use interfaces::{
    DataDirection, DescriptorType, SetupPacketParams, SetupRecipient, SetupRequest, SetupType,
};
pub use setup::{
    create_setup_packet, get_descriptor_packet, parse_setup_packet, set_device_address_packet,
    set_device_configuration_packet, ParsedSetupPacket,
};
pub use usb_device::{
    is_standard_request, StandardRequestCode, UsbDevice, UsbTransferResult, UsbTransferStatus,
};

/// Upstream `StandardRequest` is the same enum as `StandardRequestCode`.
pub type StandardRequest = StandardRequestCode;
/// Upstream aliases `DescriptorType` as `USBDescriptorType`.
pub use interfaces::DescriptorType as UsbDescriptorType;
/// Upstream's `ISetupPacketParams` is our `SetupPacketParams`.
pub type ISetupPacketParams = SetupPacketParams;
