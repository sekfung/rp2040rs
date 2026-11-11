use super::interfaces::{
    DataDirection, DescriptorType, SetupPacketParams, SetupRecipient, SetupRequest, SetupType,
};

pub fn create_setup_packet(params: SetupPacketParams) -> [u8; 8] {
    [
        ((params.data_direction as u8) << 7)
            | ((params.setup_type as u8) << 5)
            | (params.recipient as u8),
        params.b_request,
        params.w_value as u8,
        (params.w_value >> 8) as u8,
        params.w_index as u8,
        (params.w_index >> 8) as u8,
        params.w_length as u8,
        (params.w_length >> 8) as u8,
    ]
}

pub fn set_device_address_packet(address: u8) -> [u8; 8] {
    create_setup_packet(SetupPacketParams {
        data_direction: DataDirection::HostToDevice,
        setup_type: SetupType::Standard,
        recipient: SetupRecipient::Device,
        b_request: SetupRequest::SetAddress as u8,
        w_value: u16::from(address),
        w_index: 0,
        w_length: 0,
    })
}

pub fn get_descriptor_packet(descriptor_type: DescriptorType, length: u16, index: u16) -> [u8; 8] {
    create_setup_packet(SetupPacketParams {
        data_direction: DataDirection::DeviceToHost,
        setup_type: SetupType::Standard,
        recipient: SetupRecipient::Device,
        b_request: SetupRequest::GetDescriptor as u8,
        w_value: (descriptor_type as u16) << 8,
        w_index: index,
        w_length: length,
    })
}

pub fn set_device_configuration_packet(configuration: u8) -> [u8; 8] {
    create_setup_packet(SetupPacketParams {
        data_direction: DataDirection::HostToDevice,
        setup_type: SetupType::Standard,
        recipient: SetupRecipient::Device,
        b_request: SetupRequest::SetDeviceConfiguration as u8,
        w_value: u16::from(configuration),
        w_index: 0,
        w_length: 0,
    })
}

pub fn parse_setup_packet(setup: &[u8; 8]) -> ParsedSetupPacket {
    ParsedSetupPacket {
        bm_request_type: setup[0],
        b_request: setup[1],
        w_value: u16::from(setup[2]) | (u16::from(setup[3]) << 8),
        w_index: u16::from(setup[4]) | (u16::from(setup[5]) << 8),
        w_length: u16::from(setup[6]) | (u16::from(setup[7]) << 8),
    }
}

pub struct ParsedSetupPacket {
    pub bm_request_type: u8,
    pub b_request: u8,
    pub w_value: u16,
    pub w_index: u16,
    pub w_length: u16,
}

impl ParsedSetupPacket {
    pub fn direction_in(&self) -> bool {
        self.bm_request_type & 0x80 != 0
    }

    pub fn request_type(&self) -> u8 {
        (self.bm_request_type >> 5) & 0x03
    }

    pub fn recipient(&self) -> u8 {
        self.bm_request_type & 0x1f
    }
}
