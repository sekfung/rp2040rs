use super::interfaces::SetupRequest;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsbTransferStatus {
    Ack,
    Nak,
    Stall,
}

#[derive(Clone, Debug)]
pub struct UsbTransferResult {
    pub status: UsbTransferStatus,
    pub data: Option<Vec<u8>>,
}

pub trait UsbDevice {
    fn address(&self) -> u8;
    fn set_address(&mut self, address: u8);
    fn handle_setup_packet(&mut self, setup: &[u8]) -> UsbTransferResult;
    fn handle_data_out(&mut self, endpoint_address: u8, data: &[u8]) -> UsbTransferResult;
    fn handle_data_in(&mut self, endpoint_address: u8) -> UsbTransferResult;
    fn on_reset(&mut self);
    fn on_address_assigned(&mut self, _address: u8) {}
    fn on_configured(&mut self, _configuration_value: u16) {}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StandardRequestCode {
    GetStatus = 0,
    ClearFeature = 1,
    SetFeature = 3,
    SetAddress = 5,
    GetDescriptor = 6,
    SetDescriptor = 7,
    GetConfiguration = 8,
    SetConfiguration = 9,
    GetInterface = 10,
    SetInterface = 11,
    SynchFrame = 12,
}

impl StandardRequestCode {
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

pub fn is_standard_request(request: u8) -> bool {
    SetupRequest::GetStatus as u8 <= request && request <= SetupRequest::SynchFrame as u8
}
