use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::{Rc, Weak};

use crate::clock::Alarm;
use crate::irq::Irq;
use crate::peripherals::peripheral::Peripheral;
use crate::rp2040::BoardShared;

const ENDPOINT_COUNT: usize = 16;
const EP1_IN_CONTROL: u32 = 0x8;
const EP0_IN_BUFFER_CONTROL: u32 = 0x80;
const EP0_OUT_BUFFER_CONTROL: u32 = 0x84;
const EP15_OUT_BUFFER_CONTROL: u32 = 0xfc;
const ADDR_ENDP: u32 = 0x0;
const ADDR_ENDP1: u32 = 0x04;
const ADDR_ENDP15: u32 = 0x3c;
const MAIN_CTRL: u32 = 0x40;
const SOF_WR: u32 = 0x44;
const SOF_RD: u32 = 0x48;
const SIE_CTRL: u32 = 0x4c;
const SIE_STATUS: u32 = 0x50;
const INT_EP_CTRL: u32 = 0x54;
const BUFF_STATUS: u32 = 0x58;
const BUFF_CPU_SHOULD_HANDLE: u32 = 0x5c;
const EP_ABORT: u32 = 0x60;
const EP_ABORT_DONE: u32 = 0x64;
const EP_STALL_ARM: u32 = 0x68;
const NAK_POLL: u32 = 0x6c;
const EP_STATUS_STALL_NAK: u32 = 0x70;
const USB_MUXING: u32 = 0x74;
const USB_PWR: u32 = 0x78;
const INTR: u32 = 0x8c;
const INTE: u32 = 0x90;
const INTF: u32 = 0x94;
const INTS: u32 = 0x98;

const CONTROLLER_EN: u32 = 1;
const HOST_NDEVICE: u32 = 1 << 1;
const SIE_BUS_RESET: u32 = 1 << 19;
const SIE_SETUP_REC: u32 = 1 << 17;
const SIE_CONNECTED: u32 = 1 << 16;
const SIE_VBUS_DETECTED: u32 = 1;
const SIE_LINE_STATE_MASK: u32 = 0x3;
const SIE_LINE_STATE_SHIFT: u32 = 2;
const SIE_J_STATE: u32 = 1;
const SIE_WRITECLEAR_MASK: u32 = 0xffff_0000 | 0x0000_0c12;

const USB_BUF_CTRL_AVAILABLE: u32 = 1 << 10;
const USB_BUF_CTRL_FULL: u32 = 1 << 15;
const USB_BUF_CTRL_LEN_MASK: u32 = 0x3ff;
const USB_BUF1_SHIFT: u32 = 16;
const USB_CTRL_DOUBLE_BUF: u32 = 1 << 30;
const USB_CTRL_INTERRUPT_PER_TRANSFER: u32 = 1 << 29;

const INTR_BUFF_STATUS: u32 = 1 << 4;
const INTR_SETUP_REQ: u32 = 1 << 16;
const INTR_DEV_CONN_DIS: u32 = 1 << 13;
const INTR_BUS_RESET: u32 = 1 << 12;
const INTR_VBUS_DETECT: u32 = 1 << 11;
const PWR_VBUS_DETECT: u32 = 1 << 3;
const PWR_VBUS_DETECT_OVERRIDE_EN: u32 = 1 << 2;

type EndpointWriteFn = Box<dyn FnMut(u32, Vec<u8>)>;
type EndpointReadFn = Box<dyn FnMut(u32, usize)>;
type EmptyFn = Box<dyn FnMut()>;

struct UsbState {
    addr_endp: u32,
    main_ctrl: u32,
    int_raw: u32,
    int_enable: u32,
    int_force: u32,
    sie_ctrl: u32,
    sie_status: u32,
    sof_frame: u32,
    buff_status: u32,
    int_ep_ctrl: u32,
    int_ep_addr: [u32; 15],
    usb_pwr: u32,
    ep_abort: u32,
    ep_abort_done: u32,
    ep_stall_arm: u32,
    nak_poll: u32,
    ep_status_stall_nak: u32,
    host_mode: bool,
    read_alarms: Vec<Alarm>,
    write_alarms: Vec<Alarm>,
    reset_alarm: Alarm,
    read_buffers: Vec<VecDeque<Vec<u8>>>,
    write_buffers: Vec<VecDeque<Vec<u8>>>,
    on_usb_enabled: Option<EmptyFn>,
    on_reset_received: Option<EmptyFn>,
    on_endpoint_write: Option<EndpointWriteFn>,
    on_endpoint_read: Option<EndpointReadFn>,
    write_delay_micros: u64,
}

#[derive(Clone)]
pub struct UsbController {
    name: String,
    state: Rc<RefCell<UsbState>>,
    board: Weak<RefCell<BoardShared>>,
}

impl UsbController {
    pub fn new(name: impl Into<String>, board: Weak<RefCell<BoardShared>>) -> Self {
        let clock = board
            .upgrade()
            .map(|b| b.borrow().clock.clone())
            .expect("USB requires a board clock");
        let state = Rc::new(RefCell::new(UsbState {
            addr_endp: 0,
            main_ctrl: 0,
            int_raw: 0,
            int_enable: 0,
            int_force: 0,
            sie_ctrl: 0,
            sie_status: 0,
            sof_frame: 0,
            buff_status: 0,
            int_ep_ctrl: 0,
            int_ep_addr: [0; 15],
            usb_pwr: 0,
            ep_abort: 0,
            ep_abort_done: 0,
            ep_stall_arm: 0,
            nak_poll: 0,
            ep_status_stall_nak: 0,
            host_mode: false,
            read_alarms: Vec::new(),
            write_alarms: Vec::new(),
            reset_alarm: clock.create_alarm(Box::new(|| {})),
            read_buffers: (0..ENDPOINT_COUNT).map(|_| VecDeque::new()).collect(),
            write_buffers: (0..ENDPOINT_COUNT).map(|_| VecDeque::new()).collect(),
            on_usb_enabled: None,
            on_reset_received: None,
            on_endpoint_write: None,
            on_endpoint_read: None,
            write_delay_micros: 10,
        }));
        let mut read_alarms = Vec::with_capacity(ENDPOINT_COUNT);
        let mut write_alarms = Vec::with_capacity(ENDPOINT_COUNT);
        for index in 0..ENDPOINT_COUNT {
            let read_state = state.clone();
            let read_board = board.clone();
            read_alarms.push(clock.create_alarm(Box::new(move || {
                finish_read(&read_state, &read_board, index);
            })));
            let write_state = state.clone();
            let write_board = board.clone();
            write_alarms.push(clock.create_alarm(Box::new(move || {
                flush_endpoint_write(&write_state, &write_board, index);
            })));
        }
        let reset_state = state.clone();
        let reset_board = board.clone();
        let reset_alarm = clock.create_alarm(Box::new(move || {
            reset_received(&reset_state, &reset_board);
        }));
        {
            let mut state = state.borrow_mut();
            state.read_alarms = read_alarms;
            state.write_alarms = write_alarms;
            state.reset_alarm = reset_alarm;
        }
        Self {
            name: name.into(),
            state,
            board,
        }
    }

    pub fn reset_device(&self) {
        let alarm = self.state.borrow().reset_alarm.clone();
        let mut alarm = alarm;
        alarm.schedule(10_000_000.0);
        self.state.borrow_mut().reset_alarm = alarm;
    }

    pub fn send_setup_packet(&self, packet: &[u8; 8]) {
        if let Some(board) = self.board.upgrade() {
            board.borrow().usb_dpram.copy_from_slice(0, packet);
        }
        let mut state = self.state.borrow_mut();
        state.sie_status |= SIE_SETUP_REC;
        drop(state);
        self.sie_status_updated();
    }

    pub fn endpoint_read_done(&self, endpoint: u32, buffer: &[u8], delay_micros: u64) {
        if (endpoint as usize) >= ENDPOINT_COUNT {
            return;
        }
        let mut state = self.state.borrow_mut();
        state.read_buffers[endpoint as usize].push_back(buffer.to_vec());
        let mut alarm = state.read_alarms[endpoint as usize].clone();
        alarm.schedule(delay_micros as f64 * 1000.0);
        state.read_alarms[endpoint as usize] = alarm;
    }

    pub fn set_on_usb_enabled(&self, callback: EmptyFn) {
        self.state.borrow_mut().on_usb_enabled = Some(callback);
    }

    pub fn set_on_reset_received(&self, callback: EmptyFn) {
        self.state.borrow_mut().on_reset_received = Some(callback);
    }

    pub fn set_on_endpoint_write(&self, callback: EndpointWriteFn) {
        self.state.borrow_mut().on_endpoint_write = Some(callback);
    }

    pub fn set_on_endpoint_read(&self, callback: EndpointReadFn) {
        self.state.borrow_mut().on_endpoint_read = Some(callback);
    }

    pub fn dpram_updated(&self, offset: u32, value: u32) {
        let host_mode = self.state.borrow().host_mode;
        if host_mode {
            return;
        }
        if value & USB_BUF_CTRL_AVAILABLE == 0
            || !(EP0_IN_BUFFER_CONTROL..=EP15_OUT_BUFFER_CONTROL).contains(&offset)
        {
            return;
        }
        let endpoint = (offset - EP0_IN_BUFFER_CONTROL) >> 3;
        if endpoint as usize >= ENDPOINT_COUNT {
            return;
        }
        let buffer_out = offset & 4 != 0;
        let length = value & USB_BUF_CTRL_LEN_MASK;
        let (double, interrupt) = if endpoint == 0 {
            (false, true)
        } else {
            let control = self.read_endpoint_control(endpoint, buffer_out);
            (
                control & USB_CTRL_DOUBLE_BUF != 0,
                control & USB_CTRL_INTERRUPT_PER_TRANSFER != 0,
            )
        };
        let buffer1_available = (value >> USB_BUF1_SHIFT) & USB_BUF_CTRL_AVAILABLE != 0;
        if double && buffer1_available {
            let buffer1 = (value >> USB_BUF1_SHIFT) & USB_BUF_CTRL_LEN_MASK;
            let buffer_offset = self.endpoint_buffer_offset(endpoint, buffer_out) + 64;
            let mut cleared = value & !(USB_BUF_CTRL_AVAILABLE << USB_BUF1_SHIFT);
            self.write_dpram(offset, &cleared.to_le_bytes());
            if buffer_out {
                self.call_endpoint_read(endpoint, buffer1 as usize);
            } else {
                cleared &= !(USB_BUF_CTRL_FULL << USB_BUF1_SHIFT);
                self.write_dpram(offset, &cleared.to_le_bytes());
                let buffer = self.read_dpram(buffer_offset, buffer1 as usize);
                self.indicate_buffer_ready(endpoint, false);
                self.schedule_write(endpoint, buffer);
            }
        }
        let buffer_offset = self.endpoint_buffer_offset(endpoint, buffer_out);
        let mut cleared = value & !USB_BUF_CTRL_AVAILABLE;
        self.write_dpram(offset, &cleared.to_le_bytes());
        if buffer_out {
            self.call_endpoint_read(endpoint, length as usize);
        } else {
            cleared &= !USB_BUF_CTRL_FULL;
            self.write_dpram(offset, &cleared.to_le_bytes());
            let buffer = self.read_dpram(buffer_offset, length as usize);
            if interrupt || !double {
                self.indicate_buffer_ready(endpoint, false);
            }
            self.schedule_write(endpoint, buffer);
        }
    }

    fn schedule_write(&self, endpoint: u32, buffer: Vec<u8>) {
        let mut state = self.state.borrow_mut();
        state.write_buffers[endpoint as usize].push_back(buffer);
        let mut alarm = state.write_alarms[endpoint as usize].clone();
        alarm.schedule(state.write_delay_micros as f64 * 1000.0);
        state.write_alarms[endpoint as usize] = alarm;
    }

    fn call_endpoint_read(&self, endpoint: u32, length: usize) {
        let callback = self.state.borrow_mut().on_endpoint_read.take();
        if let Some(mut callback) = callback {
            callback(endpoint, length);
            self.state.borrow_mut().on_endpoint_read = Some(callback);
        }
    }

    fn read_endpoint_control(&self, endpoint: u32, out: bool) -> u32 {
        let offset = EP1_IN_CONTROL + 8 * (endpoint - 1) + if out { 4 } else { 0 };
        self.dpram_word(offset)
    }

    fn is_dpram_register(offset: u32) -> bool {
        (EP1_IN_CONTROL..=ADDR_ENDP15).contains(&offset)
            || (EP0_IN_BUFFER_CONTROL..=EP15_OUT_BUFFER_CONTROL).contains(&offset)
    }

    fn endpoint_buffer_offset(&self, endpoint: u32, out: bool) -> u32 {
        if endpoint == 0 {
            0x100
        } else {
            self.read_endpoint_control(endpoint, out) & 0xffc0
        }
    }

    fn dpram_word(&self, offset: u32) -> u32 {
        self.board
            .upgrade()
            .map(|b| {
                let memory = b.borrow().usb_dpram.clone();
                let bytes = memory.borrow();
                let offset = offset as usize;
                if offset + 4 <= bytes.len() {
                    u32::from_le_bytes([
                        bytes[offset],
                        bytes[offset + 1],
                        bytes[offset + 2],
                        bytes[offset + 3],
                    ])
                } else {
                    0
                }
            })
            .unwrap_or(0)
    }

    fn read_dpram(&self, offset: u32, length: usize) -> Vec<u8> {
        self.board
            .upgrade()
            .map(|b| {
                let memory = b.borrow().usb_dpram.clone();
                let bytes = memory.borrow();
                let start = offset as usize;
                let end = (start + length).min(bytes.len());
                bytes[start..end].to_vec()
            })
            .unwrap_or_default()
    }

    fn write_dpram(&self, offset: u32, data: &[u8]) {
        if let Some(board) = self.board.upgrade() {
            board
                .borrow()
                .usb_dpram
                .copy_from_slice(offset as usize, data);
        }
    }

    fn indicate_buffer_ready(&self, endpoint: u32, out: bool) {
        let mut state = self.state.borrow_mut();
        state.buff_status |= 1 << (endpoint * 2 + u32::from(out));
        if state.buff_status != 0 {
            state.int_raw |= INTR_BUFF_STATUS;
        }
        drop(state);
        self.check_interrupts();
    }

    fn sie_status_updated(&self) {
        let mut state = self.state.borrow_mut();
        let mapping = [
            (SIE_SETUP_REC, INTR_SETUP_REQ),
            (SIE_BUS_RESET, INTR_BUS_RESET),
            (SIE_VBUS_DETECTED, INTR_VBUS_DETECT),
            (SIE_CONNECTED, INTR_DEV_CONN_DIS),
        ];
        for (sie_bit, int_bit) in mapping {
            if state.sie_status & sie_bit != 0 {
                state.int_raw |= int_bit;
            } else {
                state.int_raw &= !int_bit;
            }
        }
        drop(state);
        self.check_interrupts();
    }

    fn check_interrupts(&self) {
        let status = self.int_status();
        if let Some(board) = self.board.upgrade() {
            let core = board.borrow().core.clone();
            core.set_interrupt(Irq::UsbCtrl as u32, status != 0);
        }
    }

    fn int_status(&self) -> u32 {
        let state = self.state.borrow();
        (state.int_raw & state.int_enable) | state.int_force
    }
}

fn finish_read(state: &Rc<RefCell<UsbState>>, board: &Weak<RefCell<BoardShared>>, endpoint: usize) {
    let buffer = {
        let mut state = state.borrow_mut();
        state.read_buffers[endpoint].pop_front()
    };
    let Some(buffer) = buffer else { return };
    if let Some(board) = board.upgrade() {
        if let Some(usb) = board.borrow().usb_ctrl.clone() {
            let out = true;
            let offset = usb.endpoint_buffer_offset(endpoint as u32, out);
            let control_reg = EP0_OUT_BUFFER_CONTROL + endpoint as u32 * 8;
            let mut control = usb.dpram_word(control_reg);
            let requested = control & USB_BUF_CTRL_LEN_MASK;
            let length = buffer.len().min(requested as usize) as u32;
            control |= USB_BUF_CTRL_FULL;
            control = (control & !USB_BUF_CTRL_LEN_MASK) | length;
            usb.write_dpram(control_reg, &control.to_le_bytes());
            usb.write_dpram(offset, &buffer[..length as usize]);
            usb.indicate_buffer_ready(endpoint as u32, out);
        }
    }
}

fn flush_endpoint_write(
    state: &Rc<RefCell<UsbState>>,
    board: &Weak<RefCell<BoardShared>>,
    endpoint: usize,
) {
    let buffers = {
        let mut state = state.borrow_mut();
        std::mem::take(&mut state.write_buffers[endpoint])
    };
    if buffers.is_empty() {
        return;
    }
    if let Some(board) = board.upgrade() {
        let callback = {
            let mut state = state.borrow_mut();
            state.on_endpoint_write.take()
        };
        if let Some(mut callback) = callback {
            for buffer in buffers {
                callback(endpoint as u32, buffer);
            }
            state.borrow_mut().on_endpoint_write = Some(callback);
        }
        drop(board);
    }
}

fn reset_received(state: &Rc<RefCell<UsbState>>, board: &Weak<RefCell<BoardShared>>) {
    if let Some(board) = board.upgrade() {
        if let Some(usb) = board.borrow().usb_ctrl.clone() {
            state.borrow_mut().sie_status |= SIE_BUS_RESET;
            usb.sie_status_updated();
        }
    }
}

impl Peripheral for UsbController {
    fn name(&self) -> &str {
        &self.name
    }

    fn read_u32(&self, offset: u32) -> u32 {
        if (ADDR_ENDP1..=ADDR_ENDP15).contains(&offset) && offset & 3 == 0 {
            return self.state.borrow().int_ep_addr[((offset - ADDR_ENDP1) >> 2) as usize];
        }
        if Self::is_dpram_register(offset) {
            return self.dpram_word(offset);
        }
        let state = self.state.borrow();
        match offset {
            ADDR_ENDP => state.addr_endp & 0x7807f,
            MAIN_CTRL => state.main_ctrl,
            SOF_WR => 0,
            SOF_RD => state.sof_frame & 0x7ff,
            SIE_CTRL => state.sie_ctrl,
            SIE_STATUS => state.sie_status,
            INT_EP_CTRL => state.int_ep_ctrl,
            BUFF_STATUS => state.buff_status,
            BUFF_CPU_SHOULD_HANDLE => 0,
            EP_ABORT => state.ep_abort,
            EP_ABORT_DONE => state.ep_abort_done,
            EP_STALL_ARM => state.ep_stall_arm,
            NAK_POLL => state.nak_poll,
            EP_STATUS_STALL_NAK => state.ep_status_stall_nak,
            USB_PWR => state.usb_pwr,
            INTR => state.int_raw,
            INTE => state.int_enable,
            INTF => state.int_force,
            INTS => (state.int_raw & state.int_enable) | state.int_force,
            _ => 0xffff_ffff,
        }
    }

    fn write_u32_atomic(&self, offset: u32, value: u32, atomic_type: u32) {
        if (ADDR_ENDP1..=ADDR_ENDP15).contains(&offset) && offset & 3 == 0 {
            self.state.borrow_mut().int_ep_addr[((offset - ADDR_ENDP1) >> 2) as usize] = value;
            return;
        }
        if Self::is_dpram_register(offset) {
            self.write_dpram(offset, &value.to_le_bytes());
            if offset >= EP0_IN_BUFFER_CONTROL {
                self.dpram_updated(offset, value);
            }
            return;
        }
        match offset {
            ADDR_ENDP => self.state.borrow_mut().addr_endp = value,
            MAIN_CTRL => {
                let enabled = value & CONTROLLER_EN != 0;
                let host = value & HOST_NDEVICE != 0;
                {
                    let mut state = self.state.borrow_mut();
                    state.main_ctrl = value & 0x8000_0003;
                    state.host_mode = host;
                }
                if enabled && !host {
                    let callback = self.state.borrow_mut().on_usb_enabled.take();
                    if let Some(mut callback) = callback {
                        callback();
                        self.state.borrow_mut().on_usb_enabled = Some(callback);
                    }
                }
            }
            SIE_CTRL => self.state.borrow_mut().sie_ctrl = value,
            SIE_STATUS => {
                self.state.borrow_mut().sie_status &= !(value & SIE_WRITECLEAR_MASK);
                if value & SIE_BUS_RESET != 0 {
                    let callback = self.state.borrow_mut().on_reset_received.take();
                    if let Some(mut callback) = callback {
                        callback();
                        self.state.borrow_mut().on_reset_received = Some(callback);
                    }
                    let mut state = self.state.borrow_mut();
                    state.sie_status &= !(SIE_LINE_STATE_MASK << SIE_LINE_STATE_SHIFT);
                    state.sie_status |= (SIE_J_STATE << SIE_LINE_STATE_SHIFT) | SIE_CONNECTED;
                }
                self.sie_status_updated();
            }
            INT_EP_CTRL => self.state.borrow_mut().int_ep_ctrl = value,
            BUFF_STATUS => {
                self.state.borrow_mut().buff_status &= !value;
                let status = self.state.borrow().buff_status;
                if status == 0 {
                    self.state.borrow_mut().int_raw &= !INTR_BUFF_STATUS;
                }
                self.check_interrupts();
            }
            EP_ABORT => {
                let mut state = self.state.borrow_mut();
                state.ep_abort = value;
                state.ep_abort_done |= value;
            }
            EP_ABORT_DONE => self.state.borrow_mut().ep_abort_done &= !value,
            EP_STALL_ARM => self.state.borrow_mut().ep_stall_arm = value,
            NAK_POLL => self.state.borrow_mut().nak_poll = value,
            EP_STATUS_STALL_NAK => self.state.borrow_mut().ep_status_stall_nak &= !value,
            USB_MUXING => {
                if value & 0x4 != 0 && value & 0x1 == 0 {
                    self.state.borrow_mut().sie_status |= SIE_CONNECTED;
                }
            }
            USB_PWR => {
                self.state.borrow_mut().usb_pwr = value;
                if value & PWR_VBUS_DETECT_OVERRIDE_EN != 0 {
                    if value & PWR_VBUS_DETECT != 0 {
                        self.state.borrow_mut().sie_status |= SIE_VBUS_DETECTED;
                    } else {
                        self.state.borrow_mut().sie_status &= !SIE_VBUS_DETECTED;
                    }
                }
            }
            INTE => {
                let current = self.state.borrow().int_enable;
                self.state.borrow_mut().int_enable = if atomic_type == 0 {
                    value & 0xfffff
                } else {
                    crate::peripherals::peripheral::atomic_update(current, atomic_type, value)
                        & 0xfffff
                };
                self.check_interrupts();
            }
            INTF => {
                let current = self.state.borrow().int_force;
                self.state.borrow_mut().int_force = if atomic_type == 0 {
                    value & 0xfffff
                } else {
                    crate::peripherals::peripheral::atomic_update(current, atomic_type, value)
                        & 0xfffff
                };
                self.check_interrupts();
            }
            SOF_WR => self.state.borrow_mut().sof_frame = value & 0x7ff,
            _ => {}
        }
    }
}
