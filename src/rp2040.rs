use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::cortex_m0_core::Core;
use crate::peripherals::clocks::Clocks;
use crate::peripherals::pll::Pll;
use crate::peripherals::ppb::Ppb;
use crate::peripherals::pwm::Pwm;
use crate::peripherals::uart::Uart;
use crate::peripherals::xosc::Xosc;
use crate::peripherals::{BasePeripheral, Peripheral};
use crate::utils::logging::{ConsoleLogger, LogLevel, Logger};

pub const FLASH_START_ADDRESS: u32 = 0x1000_0000;
pub const FLASH_END_ADDRESS: u32 = 0x1400_0000;
pub const RAM_START_ADDRESS: u32 = 0x2000_0000;
pub const APB_START_ADDRESS: u32 = 0x4000_0000;
pub const DPRAM_START_ADDRESS: u32 = 0x5010_0000;
pub const SIO_START_ADDRESS: u32 = 0xd000_0000;

const BOOTROM_SIZE: usize = 4 * 1024;
const SRAM_SIZE: usize = 264 * 1024;
const FLASH_SIZE: usize = 16 * 1024 * 1024;
const USB_DPRAM_SIZE: usize = 4 * 1024;

#[derive(Clone)]
pub struct Memory(Rc<RefCell<Vec<u8>>>);

impl Memory {
    pub fn new(size: usize) -> Self {
        Self(Rc::new(RefCell::new(vec![0; size])))
    }

    pub fn len(&self) -> usize {
        self.0.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn borrow(&self) -> std::cell::Ref<'_, Vec<u8>> {
        self.0.borrow()
    }

    pub fn byte(&self, index: usize) -> u8 {
        self.0.borrow().get(index).copied().unwrap_or(0xff)
    }

    pub fn set_byte(&self, index: usize, value: u8) {
        if let Some(byte) = self.0.borrow_mut().get_mut(index) {
            *byte = value;
        }
    }

    pub fn fill(&self, value: u8) {
        self.0.borrow_mut().fill(value);
    }

    pub fn copy_from_slice(&self, offset: usize, data: &[u8]) {
        let mut bytes = self.0.borrow_mut();
        if offset < bytes.len() {
            let end = (offset + data.len()).min(bytes.len());
            bytes[offset..end].copy_from_slice(&data[..end - offset]);
        }
    }

    fn read_u32(&self, offset: usize) -> Option<u32> {
        let bytes = self.0.borrow();
        if offset.checked_add(4)? > bytes.len() {
            return None;
        }
        Some(u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]))
    }

    fn read_u16(&self, offset: usize) -> Option<u16> {
        let bytes = self.0.borrow();
        if offset.checked_add(2)? > bytes.len() {
            return None;
        }
        Some(u16::from_le_bytes([bytes[offset], bytes[offset + 1]]))
    }

    fn write_u32(&self, offset: usize, value: u32) {
        let mut bytes = self.0.borrow_mut();
        if offset + 4 <= bytes.len() {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
    }

    fn write_u16(&self, offset: usize, value: u16) {
        let mut bytes = self.0.borrow_mut();
        if offset + 2 <= bytes.len() {
            bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        }
    }
}

pub struct BoardShared {
    pub logger: Rc<RefCell<Box<dyn Logger>>>,
    pub peripherals: BTreeMap<u32, Rc<dyn Peripheral>>,
    pub ppb: Option<Ppb>,
    pub on_break: Option<Box<dyn FnMut(u32)>>,
    pub bootrom: Memory,
    pub sram: Memory,
    pub flash: Memory,
    pub usb_dpram: Memory,
    pub xosc_freq: f64,
    pub rosc_freq: f64,
    pub clk_sys: f64,
    pub clk_peri: f64,
    pub clock_listeners: BTreeMap<u64, Box<dyn FnMut(f64, f64)>>,
    pub next_clock_listener_id: u64,
    pub clocks: Option<Clocks>,
    pub pll_sys: Option<Pll>,
    pub pll_usb: Option<Pll>,
    pub xosc: Option<Xosc>,
    pub pwm: Option<Pwm>,
    pub uart: [Option<Uart>; 2],
}

#[derive(Clone)]
pub struct Rp2040 {
    shared: Rc<RefCell<BoardShared>>,
    pub core: Core,
    pub ppb: Ppb,
    pub bootrom: Memory,
    pub sram: Memory,
    pub flash: Memory,
    pub usb_dpram: Memory,
    pub clocks: Clocks,
    pub pll_sys: Pll,
    pub pll_usb: Pll,
    pub xosc: Xosc,
    pub pwm: Pwm,
    pub uart: [Uart; 2],
}

impl Rp2040 {
    pub fn new() -> Self {
        let bootrom = Memory::new(BOOTROM_SIZE);
        let sram = Memory::new(SRAM_SIZE);
        let flash = Memory::new(FLASH_SIZE);
        let usb_dpram = Memory::new(USB_DPRAM_SIZE);
        let shared = Rc::new(RefCell::new(BoardShared {
            logger: Rc::new(RefCell::new(Box::new(ConsoleLogger::new(
                LogLevel::Debug,
                true,
            )))),
            peripherals: BTreeMap::new(),
            ppb: None,
            on_break: None,
            bootrom: bootrom.clone(),
            sram: sram.clone(),
            flash: flash.clone(),
            usb_dpram: usb_dpram.clone(),
            xosc_freq: 12_000_000.0,
            rosc_freq: 6_500_000.0,
            clk_sys: 125_000_000.0,
            clk_peri: 125_000_000.0,
            clock_listeners: BTreeMap::new(),
            next_clock_listener_id: 0,
            clocks: None,
            pll_sys: None,
            pll_usb: None,
            xosc: None,
            pwm: None,
            uart: [None, None],
        }));
        let core = Core::new(Rc::downgrade(&shared));
        let ppb = Ppb::new(core.clone());
        let weak = Rc::downgrade(&shared);
        let clocks = Clocks::new("CLOCKS_BASE", weak.clone());
        let pll_sys = Pll::new("PLL_SYS_BASE", weak.clone());
        let pll_usb = Pll::new("PLL_USB_BASE", weak.clone());
        let xosc = Xosc::new("XOSC_BASE", weak.clone());
        let pwm = Pwm::new("PWM_BASE", weak.clone());
        let uart = [
            Uart::new("UART0", weak.clone()),
            Uart::new("UART1", weak.clone()),
        ];
        {
            let mut board = shared.borrow_mut();
            board.ppb = Some(ppb.clone());
            board.clocks = Some(clocks.clone());
            board.pll_sys = Some(pll_sys.clone());
            board.pll_usb = Some(pll_usb.clone());
            board.xosc = Some(xosc.clone());
            board.pwm = Some(pwm.clone());
            board.uart = [Some(uart[0].clone()), Some(uart[1].clone())];
        }
        let rp2040 = Self {
            shared: shared.clone(),
            core,
            ppb,
            bootrom,
            sram,
            flash,
            usb_dpram,
            clocks,
            pll_sys,
            pll_usb,
            xosc,
            pwm,
            uart,
        };
        rp2040.register_stubs();
        rp2040.register_real_peripherals();
        rp2040.reset();
        rp2040
    }

    fn register_stubs(&self) {
        let board = Rc::downgrade(&self.shared);
        let entries = [
            (0x18000, "SSI"),
            (0x40000, "SYSINFO_BASE"),
            (0x40004, "SYSCFG"),
            (0x40008, "CLOCKS_BASE"),
            (0x4000c, "RESETS_BASE"),
            (0x40010, "PSM_BASE"),
            (0x40014, "IO_BANK0_BASE"),
            (0x40018, "IO_QSPI_BASE"),
            (0x4001c, "PADS_BANK0_BASE"),
            (0x40020, "PADS_QSPI_BASE"),
            (0x40024, "XOSC_BASE"),
            (0x40028, "PLL_SYS_BASE"),
            (0x4002c, "PLL_USB_BASE"),
            (0x40030, "BUSCTRL_BASE"),
            (0x40034, "UART0"),
            (0x40038, "UART1"),
            (0x4003c, "SPI0"),
            (0x40040, "SPI1"),
            (0x40044, "I2C0"),
            (0x40048, "I2C1"),
            (0x4004c, "ADC"),
            (0x40050, "PWM_BASE"),
            (0x40054, "TIMER_BASE"),
            (0x40058, "WATCHDOG_BASE"),
            (0x4005c, "RTC_BASE"),
            (0x40060, "ROSC_BASE"),
            (0x40064, "VREG_AND_CHIP_RESET_BASE"),
            (0x4006c, "TBMAN_BASE"),
            (0x50000, "DMA"),
            (0x50110, "USB"),
            (0x50200, "PIO0"),
            (0x50300, "PIO1"),
        ];
        for (key, name) in entries {
            let peripheral: Rc<dyn Peripheral> =
                Rc::new(BasePeripheral::with_board(name, board.clone()));
            self.shared.borrow_mut().peripherals.insert(key, peripheral);
        }
    }

    fn register_real_peripherals(&self) {
        let entries: [(u32, Rc<dyn Peripheral>); 5] = [
            (0x40008, Rc::new(self.clocks.clone())),
            (0x40024, Rc::new(self.xosc.clone())),
            (0x40028, Rc::new(self.pll_sys.clone())),
            (0x4002c, Rc::new(self.pll_usb.clone())),
            (0x40050, Rc::new(self.pwm.clone())),
        ];
        for (key, peripheral) in entries {
            self.set_peripheral(key, peripheral);
        }
        self.set_peripheral(0x40034, Rc::new(self.uart[0].clone()));
        self.set_peripheral(0x40038, Rc::new(self.uart[1].clone()));
    }

    pub fn reset(&self) {
        self.core.reset();
        self.flash.fill(0xff);
    }

    pub fn load_bootrom(&self, data: &[u32]) {
        let mut bytes = Vec::with_capacity(data.len() * 4);
        for word in data {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        self.bootrom.copy_from_slice(0, &bytes);
        self.reset();
    }

    pub fn set_logger(&self, logger: Box<dyn Logger>) {
        let slot = self.shared.borrow().logger.clone();
        *slot.borrow_mut() = logger;
    }

    pub fn set_on_break(&self, callback: Box<dyn FnMut(u32)>) {
        self.shared.borrow_mut().on_break = Some(callback);
    }

    pub fn set_peripheral(&self, key: u32, peripheral: Rc<dyn Peripheral>) {
        self.shared.borrow_mut().peripherals.insert(key, peripheral);
    }

    pub fn clk_sys(&self) -> f64 {
        self.shared.borrow().clk_sys
    }

    pub fn clk_peri(&self) -> f64 {
        self.shared.borrow().clk_peri
    }

    pub fn set_xosc_freq(&self, frequency: f64) {
        self.shared.borrow_mut().xosc_freq = frequency;
        self.update_clocks();
    }

    pub fn set_rosc_freq(&self, frequency: f64) {
        self.shared.borrow_mut().rosc_freq = frequency;
        self.update_clocks();
    }

    pub fn add_clock_listener(&self, listener: Box<dyn FnMut(f64, f64)>) -> u64 {
        let mut board = self.shared.borrow_mut();
        let id = board.next_clock_listener_id;
        board.next_clock_listener_id += 1;
        board.clock_listeners.insert(id, listener);
        id
    }

    pub fn remove_clock_listener(&self, id: u64) -> bool {
        self.shared
            .borrow_mut()
            .clock_listeners
            .remove(&id)
            .is_some()
    }

    pub fn update_clocks(&self) {
        update_clocks(&self.shared);
    }

    pub fn read_u32(&self, address: u32) -> u32 {
        Bus(self.shared.clone()).read_u32(address)
    }

    pub fn read_u16(&self, address: u32) -> u16 {
        Bus(self.shared.clone()).read_u16(address)
    }

    pub fn read_u8(&self, address: u32) -> u8 {
        Bus(self.shared.clone()).read_u8(address)
    }

    pub fn write_u32(&self, address: u32, value: u32) {
        Bus(self.shared.clone()).write_u32(address, value)
    }

    pub fn write_u16(&self, address: u32, value: u16) {
        Bus(self.shared.clone()).write_u16(address, value)
    }

    pub fn write_u8(&self, address: u32, value: u8) {
        Bus(self.shared.clone()).write_u8(address, value)
    }

    pub fn execute_instruction(&self) -> u32 {
        self.core.execute_instruction()
    }

    pub fn step(&self) {
        self.core.step();
    }
}

impl Default for Rp2040 {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn update_clocks(shared: &Rc<RefCell<BoardShared>>) {
    let (clocks, pll_sys, pll_usb, pwm, uart, xosc_freq, rosc_freq, old_sys, old_peri, ppb) = {
        let board = shared.borrow();
        (
            board.clocks.clone(),
            board.pll_sys.clone(),
            board.pll_usb.clone(),
            board.pwm.clone(),
            board.uart.clone(),
            board.xosc_freq,
            board.rosc_freq,
            board.clk_sys,
            board.clk_peri,
            board.ppb.clone(),
        )
    };
    let Some(clocks) = clocks else {
        return;
    };
    let pll_sys_freq = pll_sys
        .as_ref()
        .map(|pll| pll.frequency_from(xosc_freq))
        .unwrap_or(0.0);
    let pll_usb_freq = pll_usb
        .as_ref()
        .map(|pll| pll.frequency_from(xosc_freq))
        .unwrap_or(0.0);
    let sys_freq = clocks.sys_freq(xosc_freq, rosc_freq, pll_sys_freq, pll_usb_freq);
    let peri_freq = clocks.peri_freq(xosc_freq, rosc_freq, pll_sys_freq, pll_usb_freq);

    let sys_changed = sys_freq != 0.0 && sys_freq != old_sys;
    if sys_changed {
        let mut listeners = {
            let mut board = shared.borrow_mut();
            board.clk_sys = sys_freq;
            std::mem::take(&mut board.clock_listeners)
                .into_iter()
                .collect::<Vec<_>>()
        };
        if let Some(ppb) = ppb {
            ppb.systick_timer.set_frequency(sys_freq);
        }
        if let Some(pwm) = pwm.clone() {
            pwm.set_clock_freq(sys_freq);
        }
        for (_, listener) in listeners.iter_mut() {
            listener(sys_freq, old_sys);
        }
        let mut board = shared.borrow_mut();
        for (id, listener) in listeners {
            board.clock_listeners.entry(id).or_insert(listener);
        }
    }

    let peri_changed = peri_freq != 0.0 && peri_freq != old_peri;
    if peri_changed {
        shared.borrow_mut().clk_peri = peri_freq;
        for uart in uart.into_iter().flatten() {
            uart.clk_peri_changed(peri_freq);
        }
    }
}

pub(crate) struct Bus(pub(crate) Rc<RefCell<BoardShared>>);

impl Bus {
    fn warn(&self, message: &str) {
        let logger = self.0.borrow().logger.clone();
        logger.borrow().warn("RP2040", message);
    }

    fn error(&self, message: &str) {
        let logger = self.0.borrow().logger.clone();
        logger.borrow().error("RP2040", message);
    }

    fn find_peripheral(&self, address: u32) -> Option<Rc<dyn Peripheral>> {
        let key = (address >> 14) << 2;
        self.0.borrow().peripherals.get(&key).cloned()
    }

    pub(crate) fn read_u32(&self, address: u32) -> u32 {
        if address & 0x3 != 0 {
            self.error(&format!(
                "read from address {address:x}, which is not 32 bit aligned"
            ));
        }
        let board = self.0.borrow();
        let bootrom_len = board.bootrom.len();
        if (address as usize) < bootrom_len {
            if let Some(value) = board.bootrom.read_u32(address as usize) {
                return value;
            }
        } else if (FLASH_START_ADDRESS..FLASH_END_ADDRESS).contains(&address) {
            let offset = (address & 0x00ff_ffff) as usize;
            if offset + 3 < board.flash.len() {
                if let Some(value) = board.flash.read_u32(offset) {
                    return value;
                }
            }
        } else if address >= RAM_START_ADDRESS
            && address.wrapping_add(3) < RAM_START_ADDRESS + board.sram.len() as u32
        {
            if let Some(value) = board.sram.read_u32((address - RAM_START_ADDRESS) as usize) {
                return value;
            }
        } else if address >= DPRAM_START_ADDRESS
            && address.wrapping_add(3) < DPRAM_START_ADDRESS + board.usb_dpram.len() as u32
        {
            if let Some(value) = board
                .usb_dpram
                .read_u32((address - DPRAM_START_ADDRESS) as usize)
            {
                return value;
            }
        } else if address >> 12 == 0xe000e {
            if let Some(ppb) = board.ppb.clone() {
                drop(board);
                return ppb.read_u32(address & 0xfff);
            }
        } else if (SIO_START_ADDRESS..SIO_START_ADDRESS + 0x1000_0000).contains(&address) {
            drop(board);
            return 0xffff_ffff;
        }
        drop(board);
        if let Some(peripheral) = self.find_peripheral(address) {
            return peripheral.read_u32(address & 0x3fff);
        }
        self.warn(&format!("Read from invalid memory address: {address:x}"));
        0xffff_ffff
    }

    pub(crate) fn read_u16(&self, address: u32) -> u16 {
        let board = self.0.borrow();
        if address >= FLASH_START_ADDRESS
            && address.wrapping_add(1) < FLASH_START_ADDRESS + board.flash.len() as u32
        {
            if let Some(value) = board
                .flash
                .read_u16((address - FLASH_START_ADDRESS) as usize)
            {
                return value;
            }
        } else if address >= RAM_START_ADDRESS
            && address.wrapping_add(1) < RAM_START_ADDRESS + board.sram.len() as u32
        {
            if let Some(value) = board.sram.read_u16((address - RAM_START_ADDRESS) as usize) {
                return value;
            }
        }
        drop(board);
        let value = self.read_u32(address & !0x3);
        if address & 0x2 != 0 {
            (value >> 16) as u16
        } else {
            value as u16
        }
    }

    pub(crate) fn read_u8(&self, address: u32) -> u8 {
        let board = self.0.borrow();
        if address >= FLASH_START_ADDRESS
            && address < FLASH_START_ADDRESS + board.flash.len() as u32
        {
            return board.flash.byte((address - FLASH_START_ADDRESS) as usize);
        } else if address >= RAM_START_ADDRESS
            && address < RAM_START_ADDRESS + board.sram.len() as u32
        {
            return board.sram.byte((address - RAM_START_ADDRESS) as usize);
        }
        drop(board);
        let value = self.read_u16(address & !0x1);
        if address & 1 != 0 {
            (value >> 8) as u8
        } else {
            value as u8
        }
    }

    pub(crate) fn write_u32(&self, address: u32, value: u32) {
        let board = self.0.borrow();
        let bootrom_len = board.bootrom.len();
        if let Some(peripheral) = board.peripherals.get(&((address >> 14) << 2)).cloned() {
            let atomic_type = (address & 0x3000) >> 12;
            drop(board);
            peripheral.write_u32_atomic(address & 0xfff, value, atomic_type);
        } else if (address as usize) < bootrom_len {
            board.bootrom.write_u32(address as usize, value);
        } else if address >= FLASH_START_ADDRESS
            && address.wrapping_add(3) < FLASH_START_ADDRESS + board.flash.len() as u32
        {
            board
                .flash
                .write_u32((address - FLASH_START_ADDRESS) as usize, value);
        } else if address >= RAM_START_ADDRESS
            && address.wrapping_add(3) < RAM_START_ADDRESS + board.sram.len() as u32
        {
            board
                .sram
                .write_u32((address - RAM_START_ADDRESS) as usize, value);
        } else if address >= DPRAM_START_ADDRESS
            && address.wrapping_add(3) < DPRAM_START_ADDRESS + board.usb_dpram.len() as u32
        {
            board
                .usb_dpram
                .write_u32((address - DPRAM_START_ADDRESS) as usize, value);
        } else if (SIO_START_ADDRESS..SIO_START_ADDRESS + 0x1000_0000).contains(&address) {
            // SIO is implemented in Plan 3.
        } else if address >> 12 == 0xe000e {
            if let Some(ppb) = board.ppb.clone() {
                drop(board);
                ppb.write_u32_atomic(address & 0xfff, value, 0);
            }
        } else {
            drop(board);
            self.warn(&format!("Write to undefined address: {address:x}"));
        }
    }

    pub(crate) fn write_u8(&self, address: u32, value: u8) {
        let board = self.0.borrow();
        if address >= RAM_START_ADDRESS && address < RAM_START_ADDRESS + board.sram.len() as u32 {
            board
                .sram
                .set_byte((address - RAM_START_ADDRESS) as usize, value);
            return;
        }
        let aligned = address & !0x3;
        if let Some(peripheral) = board.peripherals.get(&((address >> 14) << 2)).cloned() {
            let atomic_type = (aligned & 0x3000) >> 12;
            let repeated = u32::from(value)
                | (u32::from(value) << 8)
                | (u32::from(value) << 16)
                | (u32::from(value) << 24);
            drop(board);
            peripheral.write_u32_atomic(aligned & 0xfff, repeated, atomic_type);
            return;
        }
        drop(board);
        let mut bytes = self.read_u32(aligned).to_le_bytes();
        bytes[(address & 0x3) as usize] = value;
        self.write_u32(aligned, u32::from_le_bytes(bytes));
    }

    pub(crate) fn write_u16(&self, address: u32, value: u16) {
        let board = self.0.borrow();
        if address >= RAM_START_ADDRESS
            && address.wrapping_add(1) < RAM_START_ADDRESS + board.sram.len() as u32
        {
            board
                .sram
                .write_u16((address - RAM_START_ADDRESS) as usize, value);
            return;
        }
        let aligned = address & !0x3;
        if let Some(peripheral) = board.peripherals.get(&((address >> 14) << 2)).cloned() {
            let atomic_type = (aligned & 0x3000) >> 12;
            let repeated = u32::from(value) | (u32::from(value) << 16);
            drop(board);
            peripheral.write_u32_atomic(aligned & 0xfff, repeated, atomic_type);
            return;
        }
        if address & 0x3 == 3 {
            drop(board);
            self.write_u8(address, value as u8);
            self.write_u8(address.wrapping_add(1), (value >> 8) as u8);
            return;
        }
        drop(board);
        let mut bytes = self.read_u32(aligned).to_le_bytes();
        let offset = (address & 0x3) as usize;
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        self.write_u32(aligned, u32::from_le_bytes(bytes));
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::Rp2040;
    use crate::peripherals::{atomic_update, Peripheral};
    use crate::utils::assembler::{opcode_bx, opcode_movs, opcode_nop, opcode_pop, opcode_push};
    use crate::utils::logging::Logger;

    const VTOR: u32 = 0xe000ed08;
    const NVIC_ISER: u32 = 0xe000e100;
    const NVIC_ICER: u32 = 0xe000e180;
    const NVIC_ISPR: u32 = 0xe000e200;
    const NVIC_ICPR: u32 = 0xe000e280;

    struct NullLogger;
    impl Logger for NullLogger {
        fn debug(&self, _: &str, _: &str) {}
        fn info(&self, _: &str, _: &str) {}
        fn warn(&self, _: &str, _: &str) {}
        fn error(&self, _: &str, _: &str) {}
    }

    struct RecordingPeripheral {
        read_value: u32,
        writes: RefCell<Vec<(u32, u32)>>,
    }

    impl RecordingPeripheral {
        fn new(read_value: u32) -> Self {
            Self {
                read_value,
                writes: RefCell::new(Vec::new()),
            }
        }
    }

    impl Peripheral for RecordingPeripheral {
        fn name(&self) -> &str {
            "Recording"
        }
        fn read_u32(&self, _offset: u32) -> u32 {
            self.read_value
        }
        fn write_u32_atomic(&self, offset: u32, value: u32, atomic_type: u32) {
            let value = if atomic_type == 0 {
                value
            } else {
                atomic_update(self.read_value, atomic_type, value)
            };
            self.writes.borrow_mut().push((offset, value));
        }
    }

    fn rp2040() -> Rp2040 {
        let rp = Rp2040::new();
        rp.set_logger(Box::new(NullLogger));
        rp
    }

    #[test]
    fn should_initialize_pc_and_sp_according_to_bootrom_vector_table() {
        let rp = rp2040();
        rp.load_bootrom(&[0x20041f00, 0xee]);
        assert_eq!(rp.core.sp(), 0x20041f00);
        assert_eq!(rp.core.pc(), 0xee);
    }

    #[test]
    fn should_replicate_8_bit_values_four_times() {
        let rp = rp2040();
        let recorder = Rc::new(RecordingPeripheral::new(0));
        rp.set_peripheral(0x10, recorder.clone());
        rp.write_u8(0x10123, 0x34);
        assert_eq!(*recorder.writes.borrow(), vec![(0x120, 0x3434_3434)]);
    }

    #[test]
    fn should_replicate_16_bit_values_twice() {
        let rp = rp2040();
        let recorder = Rc::new(RecordingPeripheral::new(0));
        rp.set_peripheral(0x10, recorder.clone());
        rp.write_u16(0x10123, 0x5678);
        assert_eq!(*recorder.writes.borrow(), vec![(0x120, 0x5678_5678)]);
    }

    #[test]
    fn should_support_atomic_io_register_write_addresses() {
        let rp = rp2040();
        let recorder = Rc::new(RecordingPeripheral::new(0xff));
        rp.set_peripheral(0x10, recorder.clone());
        rp.write_u32(0x11120, 0x0f);
        assert_eq!(*recorder.writes.borrow(), vec![(0x120, 0xf0)]);
    }

    #[test]
    fn should_split_an_unaligned_halfword_write_across_two_words() {
        let rp = rp2040();
        rp.write_u16(0x1000_0003, 0xbeef);
        assert_eq!(rp.flash.borrow()[3], 0xef);
        assert_eq!(rp.flash.borrow()[4], 0xbe);
    }

    #[test]
    fn should_access_last_word_of_sram() {
        let rp = rp2040();
        let end = 0x2004_2000;
        rp.write_u32(end - 4, 0x1234_5678);
        assert_eq!(rp.read_u32(end - 4), 0x1234_5678);
        assert_eq!(rp.read_u16(end - 2), 0x1234);
    }

    #[test]
    fn should_access_last_word_of_flash() {
        let rp = rp2040();
        let end = 0x1100_0000;
        rp.write_u32(end - 4, 0x1234_5678);
        assert_eq!(rp.read_u32(end - 4), 0x1234_5678);
        assert_eq!(rp.read_u16(end - 2), 0x1234);
    }

    #[test]
    fn should_access_last_word_of_usb_dpram() {
        let rp = rp2040();
        let end = 0x5010_1000;
        rp.write_u32(end - 4, 0x1234_5678);
        assert_eq!(rp.read_u32(end - 4), 0x1234_5678);
        assert_eq!(rp.read_u16(end - 2), 0x1234);
    }

    #[test]
    fn should_not_throw_on_accesses_past_sram_end() {
        let rp = rp2040();
        let end = 0x2004_2000;
        for address in end - 3..end {
            let _ = rp.read_u16(address);
            rp.write_u16(address, 0);
            let _ = rp.read_u32(address);
            rp.write_u32(address, 0);
        }
    }

    #[test]
    fn should_not_throw_on_accesses_past_flash_end() {
        let rp = rp2040();
        let end = 0x1100_0000;
        for address in end - 3..end {
            let _ = rp.read_u16(address);
            rp.write_u16(address, 0);
            let _ = rp.read_u32(address);
            rp.write_u32(address, 0);
        }
    }

    #[test]
    fn should_not_throw_on_accesses_past_dpram_end() {
        let rp = rp2040();
        let end = 0x5010_1000;
        for address in end - 3..end {
            let _ = rp.read_u16(address);
            rp.write_u16(address, 0);
            let _ = rp.read_u32(address);
            rp.write_u32(address, 0);
        }
    }

    #[test]
    fn should_execute_exception_handler_and_return() {
        const INT1: u32 = 1 << 1;
        const HANDLER: u32 = 0x1000_0100;
        const EXC_INT1: u32 = 16 + 1;
        let rp = rp2040();
        rp.core.set_sp(0x2000_4000);
        rp.core.set_pc(0x1000_4001);
        rp.core.set_reg(0, 0x44);
        rp.core.set_pending_interrupts(INT1);
        rp.core.set_enabled_interrupts(INT1);
        rp.core.set_interrupts_updated(true);
        rp.write_u32(VTOR, 0x1000_0000);
        rp.write_u32(0x1000_0000 + EXC_INT1 * 4, HANDLER);
        rp.write_u16(HANDLER, opcode_movs(0, 0x55));
        rp.write_u16(HANDLER + 2, opcode_bx(14));
        rp.step();
        assert_eq!(rp.core.ipsr(), EXC_INT1);
        assert_eq!(rp.core.pc(), HANDLER + 2);
        assert_eq!(rp.core.reg(0), 0x55);
        rp.step();
        assert_eq!(rp.core.pc(), 0x1000_4000);
        assert_eq!(rp.core.reg(0), 0x44);
        assert_eq!(rp.core.ipsr(), 0);
    }

    #[test]
    fn should_return_from_exception_with_pop_pc() {
        const INT1: u32 = 1 << 1;
        const HANDLER: u32 = 0x1000_0100;
        const EXC_INT1: u32 = 16 + 1;
        let rp = rp2040();
        rp.core.set_sp(0x2000_4000);
        rp.core.set_pc(0x1000_4001);
        rp.core.set_reg(4, 105);
        rp.core.set_pending_interrupts(INT1);
        rp.core.set_enabled_interrupts(INT1);
        rp.core.set_interrupts_updated(true);
        rp.write_u32(VTOR, 0x1000_0000);
        rp.write_u32(0x1000_0000 + EXC_INT1 * 4, HANDLER);
        rp.write_u16(HANDLER, opcode_push(true, 0b0111_0000));
        rp.write_u16(HANDLER + 2, opcode_movs(4, 42));
        rp.write_u16(HANDLER + 4, opcode_pop(true, 0b0111_0000));
        rp.step();
        assert_eq!(rp.core.ipsr(), EXC_INT1);
        assert_eq!(rp.core.pc(), HANDLER + 2);
        rp.step();
        assert_eq!(rp.core.reg(4), 42);
        rp.step();
        assert_eq!(rp.core.pc(), 0x1000_4000);
        assert_eq!(rp.core.reg(4), 105);
        assert_eq!(rp.core.ipsr(), 0);
    }

    #[test]
    fn should_clear_pending_user_irq_during_exception_entry() {
        const INT31: u32 = 1 << 31;
        const HANDLER: u32 = 0x1000_3100;
        const EXC_INT31: u32 = 16 + 31;
        let rp = rp2040();
        rp.core.set_sp(0x2000_4000);
        rp.core.set_pc(0x1000_4001);
        rp.write_u32(NVIC_ISPR, INT31);
        rp.core.set_enabled_interrupts(INT31);
        rp.core.set_interrupts_updated(true);
        rp.write_u32(VTOR, 0x1000_0000);
        rp.write_u32(0x1000_0000 + EXC_INT31 * 4, HANDLER);
        rp.write_u16(HANDLER, opcode_nop());
        assert_eq!(rp.core.pending_interrupts(), INT31);
        rp.step();
        assert_eq!(rp.core.pending_interrupts(), 0);
        assert_eq!(rp.read_u32(NVIC_ISPR), 0);
    }

    #[test]
    fn should_set_pending_bits_on_nvic_isp_write() {
        let rp = rp2040();
        rp.core.set_pending_interrupts(0x1);
        rp.write_u32(NVIC_ISPR, 0x10);
        assert_eq!(rp.core.pending_interrupts(), 0x11);
    }

    #[test]
    fn should_clear_pending_bits_on_nvic_icpr_write() {
        let rp = rp2040();
        rp.core.set_pending_interrupts(0xff00_000f);
        rp.write_u32(NVIC_ICPR, 0x1000_000f);
        assert_eq!(rp.read_u32(NVIC_ISPR), 0xef00_000f);
    }

    #[test]
    fn should_set_enabled_bits_on_nvic_iser_write() {
        let rp = rp2040();
        rp.core.set_enabled_interrupts(0x1);
        rp.write_u32(NVIC_ISER, 0x10);
        assert_eq!(rp.core.enabled_interrupts(), 0x11);
    }

    #[test]
    fn should_clear_enabled_bits_on_nvic_icer_write() {
        let rp = rp2040();
        rp.core.set_enabled_interrupts(0xff);
        rp.write_u32(NVIC_ICER, 0x10);
        assert_eq!(rp.core.enabled_interrupts(), 0xef);
    }

    #[test]
    fn should_read_nvic_enabled_interrupts() {
        let rp = rp2040();
        rp.core.set_enabled_interrupts(0x1);
        assert_eq!(rp.read_u32(NVIC_ISER), 0x1);
        assert_eq!(rp.read_u32(NVIC_ICER), 0x1);
    }

    #[test]
    fn should_read_nvic_pending_interrupts() {
        let rp = rp2040();
        rp.core.set_pending_interrupts(0x2);
        assert_eq!(rp.read_u32(NVIC_ISPR), 0x2);
        assert_eq!(rp.read_u32(NVIC_ICPR), 0x2);
    }

    #[test]
    fn should_update_interrupt_levels_on_ipr3_write() {
        let rp = rp2040();
        rp.write_u32(0xe000e40c, 0x0080_0000);
        let priorities = rp.core.interrupt_priorities();
        assert_eq!(priorities[0], !(1 << 14));
        assert_eq!(priorities[1], 0);
        assert_eq!(priorities[2], 1 << 14);
        assert_eq!(priorities[3], 0);
        assert_eq!(rp.read_u32(0xe000e40c), 0x0080_0000);
    }

    #[test]
    fn should_read_interrupt_priorities_from_ipr5() {
        let rp = rp2040();
        rp.core
            .set_interrupt_priorities([0, 0x001f_ffff, 0x0020_0000, 0xffc0_0000]);
        assert_eq!(rp.read_u32(0xe000e414), 0xc0c0_8040);
    }
}
