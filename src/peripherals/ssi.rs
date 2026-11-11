use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::peripherals::peripheral::{atomic_update, Peripheral};
use crate::rp2040::BoardShared;

const CTRLR0: u32 = 0x00;
const CTRLR1: u32 = 0x04;
const SSIENR: u32 = 0x08;
const BAUDR: u32 = 0x14;
const TXFLR: u32 = 0x20;
const RXFLR: u32 = 0x24;
const SR: u32 = 0x28;
const SR_TFNF: u32 = 0x02;
const SR_TFE: u32 = 0x04;
const SR_RFNE: u32 = 0x08;
const IDR: u32 = 0x58;
const VERSION_ID: u32 = 0x5c;
const DR0: u32 = 0x60;
const RX_SAMPLE_DLY: u32 = 0xf0;
const SPI_CTRLR0: u32 = 0xf4;
const TXD_DRIVE_EDGE: u32 = 0xf8;
const CMD_READ_STATUS: u32 = 0x05;

#[derive(Clone)]
pub struct Ssi {
    name: String,
    state: Rc<RefCell<SsiState>>,
}

struct SsiState {
    dr0: u32,
    txflr: u32,
    rxflr: u32,
    baudr: u32,
    ctrlr0: u32,
    ctrlr1: u32,
    ssienr: u32,
    spi_ctrlr0: u32,
    rx_sample_dly: u32,
    txd_drive_edge: u32,
}

impl Ssi {
    pub fn new(name: impl Into<String>, _board: Weak<RefCell<BoardShared>>) -> Self {
        Self {
            name: name.into(),
            state: Rc::new(RefCell::new(SsiState {
                dr0: 0,
                txflr: 0,
                rxflr: 0,
                baudr: 0,
                ctrlr0: 0,
                ctrlr1: 0,
                ssienr: 0,
                spi_ctrlr0: 0,
                rx_sample_dly: 0,
                txd_drive_edge: 0,
            })),
        }
    }

    fn read(&self, offset: u32) -> Option<u32> {
        let state = self.state.borrow();
        Some(match offset {
            TXFLR => state.txflr,
            RXFLR => state.rxflr,
            CTRLR0 => state.ctrlr0,
            CTRLR1 => state.ctrlr1,
            SSIENR => state.ssienr,
            BAUDR => state.baudr,
            SR => SR_TFE | SR_RFNE | SR_TFNF,
            IDR => 0x5153_5049,
            VERSION_ID => 0x3430_312a,
            RX_SAMPLE_DLY => state.rx_sample_dly,
            TXD_DRIVE_EDGE => state.txd_drive_edge,
            SPI_CTRLR0 => state.spi_ctrlr0,
            DR0 => state.dr0,
            _ => return None,
        })
    }
}

impl Peripheral for Ssi {
    fn name(&self) -> &str {
        &self.name
    }

    fn read_u32(&self, offset: u32) -> u32 {
        self.read(offset).unwrap_or(0xffff_ffff)
    }

    fn write_u32_atomic(&self, offset: u32, value: u32, atomic_type: u32) {
        let current = self.read(offset).unwrap_or(0);
        let value = if atomic_type == 0 {
            value
        } else {
            atomic_update(current, atomic_type, value)
        };
        let mut state = self.state.borrow_mut();
        match offset {
            TXFLR => state.txflr = value,
            RXFLR => state.rxflr = value,
            CTRLR0 => state.ctrlr0 = value,
            CTRLR1 => state.ctrlr1 = value,
            SSIENR => state.ssienr = value,
            BAUDR => state.baudr = value,
            RX_SAMPLE_DLY => state.rx_sample_dly = value & 0xff,
            TXD_DRIVE_EDGE => state.txd_drive_edge = value & 0xff,
            SPI_CTRLR0 => state.spi_ctrlr0 = value,
            DR0 if value == CMD_READ_STATUS => state.dr0 = 0,
            _ => {}
        }
    }
}
