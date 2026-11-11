use std::cell::{Cell, RefCell};
use std::rc::Weak;

use crate::cortex_m0_core::Core;
use crate::gpio_pin::GpioPin;
use crate::irq::Irq;
use crate::peripherals::peripheral::{BasePeripheral, Peripheral};
use crate::rp2040::BoardShared;

const GPIO_CTRL_LAST: u32 = 0x0ec;
const INTR0: u32 = 0x0f0;
const PROC0_INTE0: u32 = 0x100;
const PROC0_INTF0: u32 = 0x110;
const PROC0_INTS0: u32 = 0x120;
const PROC0_INTS3: u32 = 0x12c;

#[derive(Clone)]
pub struct Io {
    base: BasePeripheral,
    pins: Vec<GpioPin>,
    core: Core,
    raw_write_value: Cell<u32>,
}

impl Io {
    pub fn new(
        name: impl Into<String>,
        pins: Vec<GpioPin>,
        core: Core,
        board: Weak<RefCell<BoardShared>>,
    ) -> Self {
        Self {
            base: BasePeripheral::with_board(name, board),
            pins,
            core,
            raw_write_value: Cell::new(0),
        }
    }

    fn update_io_interrupt(&self) {
        self.core.set_interrupt(
            Irq::IoBank0 as u32,
            self.pins.iter().any(GpioPin::irq_value),
        );
    }
}

impl Peripheral for Io {
    fn name(&self) -> &str {
        self.base.name()
    }

    fn read_u32(&self, offset: u32) -> u32 {
        if offset <= GPIO_CTRL_LAST {
            let index = (offset >> 3) as usize;
            let pin = match self.pins.get(index) {
                Some(pin) => pin,
                None => return 0,
            };
            return if offset & 0x4 != 0 {
                pin.ctrl()
            } else {
                pin.status()
            };
        }
        if (INTR0..=PROC0_INTS3).contains(&offset) {
            let start = ((offset & 0xf) * 2) as usize;
            let register = offset & !0xf;
            let mut result = 0u32;
            for index in (0..8).rev() {
                let Some(pin) = self.pins.get(start + index) else {
                    continue;
                };
                result <<= 4;
                result |= match register {
                    INTR0 => pin.irq_status(),
                    PROC0_INTE0 => pin.irq_enable_mask(),
                    PROC0_INTF0 => pin.irq_force_mask(),
                    PROC0_INTS0 => {
                        (pin.irq_status() & pin.irq_enable_mask()) | pin.irq_force_mask()
                    }
                    _ => 0,
                };
            }
            return result;
        }
        self.base.read_u32(offset)
    }

    fn write_u32_atomic(&self, offset: u32, value: u32, atomic_type: u32) {
        self.raw_write_value.set(value);
        if offset <= GPIO_CTRL_LAST {
            let index = (offset >> 3) as usize;
            if offset & 0x4 != 0 {
                if let Some(pin) = self.pins.get(index) {
                    pin.set_ctrl(value);
                }
            }
            return;
        }
        if (INTR0..=PROC0_INTS3).contains(&offset) {
            let start = ((offset & 0xf) * 2) as usize;
            let register = offset & !0xf;
            for index in 0..8 {
                let Some(pin) = self.pins.get(start + index) else {
                    continue;
                };
                let pin_value = (value >> (index * 4)) & 0xf;
                let raw_value = (self.raw_write_value.get() >> (index * 4)) & 0xf;
                match register {
                    INTR0 => pin.update_irq_value(raw_value),
                    PROC0_INTE0 => pin.set_irq_enable_mask(pin_value),
                    PROC0_INTF0 => pin.set_irq_force_mask(pin_value),
                    _ => {}
                }
            }
            self.update_io_interrupt();
            return;
        }
        self.base.write_u32_atomic(offset, value, atomic_type);
    }
}
