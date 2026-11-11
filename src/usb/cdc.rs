use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::peripherals::usb::UsbController;
use crate::utils::Fifo;

use super::interfaces::{
    DataDirection, DescriptorType, SetupPacketParams, SetupRecipient, SetupType,
};
use super::setup::{
    create_setup_packet, get_descriptor_packet, set_device_address_packet,
    set_device_configuration_packet,
};

const CDC_REQUEST_SET_CONTROL_LINE_STATE: u8 = 0x22;
const CDC_DTR: u16 = 1 << 0;
const CDC_RTS: u16 = 1 << 1;
const CDC_DATA_CLASS: u8 = 10;
const ENDPOINT_BULK: u8 = 2;
const TX_FIFO_SIZE: usize = 512;
const ENDPOINT_ZERO: u32 = 0;
const CONFIGURATION_DESCRIPTOR_SIZE: usize = 9;

type SerialDataCallback = Box<dyn FnMut(&[u8])>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EndpointNumbers {
    pub in_endpoint: i32,
    pub out_endpoint: i32,
}

pub fn extract_endpoint_numbers(descriptors: &[u8]) -> EndpointNumbers {
    let mut index = 0;
    let mut found_interface = false;
    let mut result = EndpointNumbers {
        in_endpoint: -1,
        out_endpoint: -1,
    };
    while index < descriptors.len() {
        let len = descriptors[index] as usize;
        if len < 2 || descriptors.len() < index + len {
            break;
        }
        let descriptor_type = descriptors[index + 1];
        if descriptor_type == DescriptorType::Interface as u8 && len == 9 {
            let num_endpoints = descriptors[index + 4];
            let interface_class = descriptors[index + 5];
            found_interface = num_endpoints == 2 && interface_class == CDC_DATA_CLASS;
        }
        if found_interface && descriptor_type == DescriptorType::Endpoint as u8 && len == 7 {
            let address = descriptors[index + 2];
            let attributes = descriptors[index + 3];
            if attributes & 0x3 == ENDPOINT_BULK {
                if address & 0x80 != 0 {
                    result.in_endpoint = i32::from(address & 0xf);
                } else {
                    result.out_endpoint = i32::from(address & 0xf);
                }
            }
        }
        index += len;
    }
    result
}

struct CdcState {
    usb: UsbController,
    tx_fifo: Fifo,
    initialized: bool,
    descriptors_size: Option<usize>,
    descriptors: Vec<u8>,
    out_endpoint: i32,
    in_endpoint: i32,
    on_serial_data: Option<SerialDataCallback>,
    on_device_connected: Option<Box<dyn FnMut()>>,
}

#[derive(Clone)]
pub struct UsbCdc {
    state: Rc<RefCell<CdcState>>,
}

impl UsbCdc {
    pub fn new(usb: UsbController) -> Self {
        let state = Rc::new(RefCell::new(CdcState {
            usb: usb.clone(),
            tx_fifo: Fifo::new(TX_FIFO_SIZE),
            initialized: false,
            descriptors_size: None,
            descriptors: Vec::new(),
            out_endpoint: -1,
            in_endpoint: -1,
            on_serial_data: None,
            on_device_connected: None,
        }));
        let weak: Weak<RefCell<CdcState>> = Rc::downgrade(&state);
        usb.set_on_usb_enabled(Box::new({
            let weak = weak.clone();
            move || {
                if let Some(state) = weak.upgrade() {
                    state.borrow().usb.reset_device();
                }
            }
        }));
        usb.set_on_reset_received(Box::new({
            let weak = weak.clone();
            move || {
                if let Some(state) = weak.upgrade() {
                    state
                        .borrow()
                        .usb
                        .send_setup_packet(&set_device_address_packet(1));
                }
            }
        }));
        usb.set_on_endpoint_write(Box::new({
            let weak = weak.clone();
            move |endpoint, buffer: Vec<u8>| {
                if let Some(state) = weak.upgrade() {
                    handle_endpoint_write(&state, endpoint, &buffer);
                }
            }
        }));
        usb.set_on_endpoint_read(Box::new({
            let weak = weak.clone();
            move |endpoint, size| {
                if let Some(state) = weak.upgrade() {
                    let buffer = {
                        let mut state = state.borrow_mut();
                        let out_endpoint = state.out_endpoint;
                        if endpoint as i32 != out_endpoint {
                            return;
                        }
                        let count = size.min(state.tx_fifo.item_count());
                        let mut buffer = Vec::with_capacity(count);
                        for _ in 0..count {
                            buffer.push(state.tx_fifo.pull() as u8);
                        }
                        buffer
                    };
                    state.borrow().usb.endpoint_read_done(endpoint, &buffer, 10);
                }
            }
        }));
        Self { state }
    }

    pub fn set_on_serial_data(&self, callback: SerialDataCallback) {
        self.state.borrow_mut().on_serial_data = Some(callback);
    }

    pub fn set_on_device_connected(&self, callback: Box<dyn FnMut()>) {
        self.state.borrow_mut().on_device_connected = Some(callback);
    }

    pub fn send_serial_byte(&self, data: u8) {
        self.state.borrow_mut().tx_fifo.push(u32::from(data));
    }

    pub fn tx_fifo_item_count(&self) -> usize {
        self.state.borrow().tx_fifo.item_count()
    }
}

fn handle_endpoint_write(cdc: &Rc<RefCell<CdcState>>, endpoint: u32, buffer: &[u8]) {
    let mut send_descriptor = false;
    let mut send_config = false;
    let mut control_line = false;
    let mut notify_connected = false;
    let serial;
    let mut descriptors_size;
    let mut descriptors;
    let mut initialized;
    let mut endpoints_update = None;
    let usb;
    {
        let state = cdc.borrow();
        usb = state.usb.clone();
        descriptors_size = state.descriptors_size;
        descriptors = state.descriptors.clone();
        initialized = state.initialized;
        if endpoint == ENDPOINT_ZERO && buffer.is_empty() {
            if descriptors_size.is_none() {
                send_descriptor = true;
            } else if !initialized {
                control_line = true;
                notify_connected = true;
                initialized = true;
            }
        }
        if endpoint == ENDPOINT_ZERO && buffer.len() > 1 {
            if buffer.len() == CONFIGURATION_DESCRIPTOR_SIZE
                && buffer[1] == DescriptorType::Configuration as u8
                && descriptors_size.is_none()
            {
                descriptors_size = Some(usize::from(buffer[2]) | (usize::from(buffer[3]) << 8));
                send_descriptor = true;
            } else if let Some(size) = descriptors_size {
                if descriptors.len() < size {
                    descriptors.extend_from_slice(buffer);
                }
            }
            if let Some(size) = descriptors_size {
                if descriptors.len() == size {
                    let endpoints = extract_endpoint_numbers(&descriptors);
                    send_config = true;
                    endpoints_update = Some(endpoints);
                }
            }
        }
        serial = endpoint as i32 == state.in_endpoint;
    }
    if let Some(endpoints) = endpoints_update {
        let mut state = cdc.borrow_mut();
        state.in_endpoint = endpoints.in_endpoint;
        state.out_endpoint = endpoints.out_endpoint;
    }
    if send_descriptor {
        let length = descriptors_size.unwrap_or(CONFIGURATION_DESCRIPTOR_SIZE) as u16;
        usb.send_setup_packet(&get_descriptor_packet(
            DescriptorType::Configuration,
            length,
            0,
        ));
    }
    if control_line {
        usb.send_setup_packet(&create_setup_packet(SetupPacketParams {
            data_direction: DataDirection::HostToDevice,
            setup_type: SetupType::Class,
            recipient: SetupRecipient::Device,
            b_request: CDC_REQUEST_SET_CONTROL_LINE_STATE,
            w_value: CDC_DTR | CDC_RTS,
            w_index: 0,
            w_length: 0,
        }));
    }
    if send_config {
        usb.send_setup_packet(&set_device_configuration_packet(1));
    }
    {
        let mut state = cdc.borrow_mut();
        state.initialized = initialized;
        state.descriptors_size = descriptors_size;
        state.descriptors = descriptors;
    }
    if notify_connected {
        let callback = cdc.borrow_mut().on_device_connected.take();
        if let Some(mut callback) = callback {
            callback();
            cdc.borrow_mut().on_device_connected = Some(callback);
        }
    }
    if serial {
        let callback = cdc.borrow_mut().on_serial_data.take();
        if let Some(mut callback) = callback {
            callback(buffer);
            cdc.borrow_mut().on_serial_data = Some(callback);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::extract_endpoint_numbers;

    #[test]
    fn should_not_die_if_descriptors_are_invalid() {
        assert_eq!(extract_endpoint_numbers(&[0]).in_endpoint, -1);
        assert_eq!(extract_endpoint_numbers(&[0]).out_endpoint, -1);
        assert_eq!(extract_endpoint_numbers(&[9]).in_endpoint, -1);
        assert_eq!(extract_endpoint_numbers(&[9]).out_endpoint, -1);
    }

    #[test]
    fn should_extract_endpoint_numbers_from_pi_pico_sdk_descriptors() {
        let descriptors = [
            9, 2, 84, 0, 3, 1, 0, 128, 125, 8, 11, 0, 2, 2, 2, 0, 0, 9, 4, 0, 0, 1, 2, 2, 0, 4, 5,
            36, 0, 32, 1, 5, 36, 1, 0, 1, 4, 36, 2, 2, 5, 36, 6, 0, 1, 7, 5, 129, 3, 8, 0, 16, 9,
            4, 1, 0, 2, 10, 0, 0, 0, 7, 5, 2, 2, 64, 0, 0, 7, 5, 130, 2, 64, 0, 0, 9, 4, 2, 0, 0,
            255, 0, 1, 5,
        ];
        let result = extract_endpoint_numbers(&descriptors);
        assert_eq!(result.in_endpoint, 2);
        assert_eq!(result.out_endpoint, 2);
    }

    #[test]
    fn should_extract_endpoint_numbers_from_micropython_descriptors() {
        let descriptors = [
            9, 2, 75, 0, 2, 1, 0, 128, 125, 8, 11, 0, 2, 2, 2, 0, 0, 9, 4, 0, 0, 1, 2, 2, 0, 4, 5,
            36, 0, 32, 1, 5, 36, 1, 0, 1, 4, 36, 2, 2, 5, 36, 6, 0, 1, 7, 5, 129, 3, 8, 0, 16, 9,
            4, 1, 0, 2, 10, 0, 0, 0, 7, 5, 2, 2, 64, 0, 0, 7, 5, 130, 2, 64, 0, 0,
        ];
        let result = extract_endpoint_numbers(&descriptors);
        assert_eq!(result.in_endpoint, 2);
        assert_eq!(result.out_endpoint, 2);
    }

    #[test]
    fn should_extract_endpoint_numbers_from_circuitpython_descriptors() {
        let descriptors = [
            9, 2, 218, 0, 6, 1, 0, 128, 50, 8, 11, 0, 2, 2, 2, 0, 0, 9, 4, 0, 0, 1, 2, 2, 0, 4, 5,
            36, 0, 16, 1, 5, 36, 1, 1, 1, 4, 36, 2, 2, 5, 36, 6, 0, 1, 7, 5, 129, 3, 64, 0, 16, 9,
            4, 1, 0, 2, 10, 0, 0, 5, 7, 5, 2, 2, 64, 0, 0, 7, 5, 130, 2, 64, 0, 0, 9, 4, 2, 0, 2,
            8, 6, 80, 6, 7, 5, 131, 2, 64, 0, 0, 7, 5, 3, 2, 64, 0, 0, 9, 4, 3, 0, 2, 3, 0, 0, 7,
            9, 33, 17, 1, 0, 1, 34, 195, 0, 7, 5, 132, 3, 64, 0, 8, 7, 5, 4, 3, 64, 0, 8, 9, 4, 4,
            0, 0, 1, 1, 0, 11, 9, 36, 1, 0, 1, 9, 0, 1, 5, 9, 4, 5, 0, 2, 1, 3, 0, 10, 7, 36, 1, 0,
            1, 37, 0, 6, 36, 2, 1, 1, 8, 6, 36, 2, 2, 2, 0, 9, 36, 3, 1, 3, 1, 2, 1, 9, 9, 36, 3,
            2, 4, 1, 1, 1, 0, 7, 5, 5, 2, 64, 0, 0, 5, 37, 1, 1, 1, 7, 5, 133, 2, 64, 0, 0, 5, 37,
            1, 1, 3,
        ];
        let result = extract_endpoint_numbers(&descriptors);
        assert_eq!(result.in_endpoint, 2);
        assert_eq!(result.out_endpoint, 2);
    }

    #[test]
    fn should_extract_endpoint_numbers_from_arduino_core_descriptors() {
        let descriptors = [
            9, 2, 75, 0, 2, 1, 0, 192, 250, 8, 11, 0, 2, 2, 2, 0, 0, 9, 4, 0, 0, 1, 2, 2, 1, 0, 5,
            36, 0, 16, 1, 5, 36, 1, 3, 1, 4, 36, 2, 6, 5, 36, 6, 0, 1, 7, 5, 130, 3, 64, 0, 16, 9,
            4, 1, 0, 2, 10, 0, 0, 0, 7, 5, 129, 2, 64, 0, 0, 7, 5, 1, 2, 64, 0, 0,
        ];
        let result = extract_endpoint_numbers(&descriptors);
        assert_eq!(result.in_endpoint, 1);
        assert_eq!(result.out_endpoint, 1);
    }
}
