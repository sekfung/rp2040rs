#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataDirection {
    HostToDevice = 0,
    DeviceToHost = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupType {
    Standard = 0,
    Class = 1,
    Vendor = 2,
    Reserved = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupRecipient {
    Device = 0,
    Interface = 1,
    Endpoint = 2,
    Other = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupRequest {
    GetStatus = 0,
    ClearFeature = 1,
    Reserved1 = 2,
    SetFeature = 3,
    Reserved2 = 4,
    SetAddress = 5,
    GetDescriptor = 6,
    SetDescriptor = 7,
    GetConfiguration = 8,
    SetDeviceConfiguration = 9,
    GetInterface = 10,
    SetInterface = 11,
    SynchFrame = 12,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DescriptorType {
    Device = 1,
    Configuration = 2,
    String = 3,
    Interface = 4,
    Endpoint = 5,
    DeviceQualifier = 6,
    OtherSpeedConfiguration = 7,
    InterfacePower = 8,
    Hid = 0x21,
    HidReport = 0x22,
    HidPhysical = 0x23,
}

pub struct SetupPacketParams {
    pub data_direction: DataDirection,
    pub setup_type: SetupType,
    pub recipient: SetupRecipient,
    pub b_request: u8,
    pub w_value: u16,
    pub w_index: u16,
    pub w_length: u16,
}
