use std::cell::Cell;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpioPinState {
    Low,
    High,
    Input,
    InputPullUp,
    InputPullDown,
    InputBusKeeper,
}

const IRQ_EDGE_HIGH: u32 = 1 << 3;
const IRQ_EDGE_LOW: u32 = 1 << 2;
const IRQ_LEVEL_HIGH: u32 = 1 << 1;
const IRQ_LEVEL_LOW: u32 = 1 << 0;

fn apply_override(value: bool, override_type: u32) -> bool {
    match override_type {
        0 => value,
        1 => !value,
        2 => false,
        3 => true,
        _ => value,
    }
}

/// GPIO pad/function state shared by the SIO, IO_BANK0, PADS and PIO blocks.
#[derive(Clone, Debug)]
pub struct GpioPin {
    index: usize,
    raw_input_value: Rc<Cell<bool>>,
    pad_value: Rc<Cell<u32>>,
    ctrl: Rc<Cell<u32>>,
    irq_enable_mask: Rc<Cell<u32>>,
    irq_force_mask: Rc<Cell<u32>>,
    irq_status: Rc<Cell<u32>>,
    last_state: Rc<Cell<GpioPinState>>,
}

impl GpioPin {
    pub fn new(index: usize) -> Self {
        Self {
            index,
            raw_input_value: Rc::new(Cell::new(false)),
            pad_value: Rc::new(Cell::new(0b0110110)),
            ctrl: Rc::new(Cell::new(0x1f)),
            irq_enable_mask: Rc::new(Cell::new(0)),
            irq_force_mask: Rc::new(Cell::new(0)),
            irq_status: Rc::new(Cell::new(0)),
            last_state: Rc::new(Cell::new(GpioPinState::Input)),
        }
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn pad_value(&self) -> u32 {
        self.pad_value.get()
    }

    pub fn set_pad_value(&self, value: u32) {
        self.pad_value.set(value);
    }

    pub fn ctrl(&self) -> u32 {
        self.ctrl.get()
    }

    pub fn set_ctrl(&self, value: u32) {
        self.ctrl.set(value);
    }

    pub fn function_select(&self) -> u32 {
        self.ctrl.get() & 0x1f
    }

    pub fn input_enable(&self) -> bool {
        self.pad_value.get() & 0x40 != 0
    }

    pub fn pullup_enabled(&self) -> bool {
        self.pad_value.get() & 0x8 != 0
    }

    pub fn pulldown_enabled(&self) -> bool {
        self.pad_value.get() & 0x4 != 0
    }

    pub fn raw_input_value(&self) -> bool {
        self.raw_input_value.get()
    }

    pub fn input_value(&self) -> bool {
        let value = self.raw_input_value.get() && self.input_enable();
        apply_override(value, (self.ctrl.get() >> 16) & 0x3)
    }

    pub fn irq_enable_mask(&self) -> u32 {
        self.irq_enable_mask.get()
    }

    pub fn set_irq_enable_mask(&self, value: u32) {
        self.irq_enable_mask.set(value);
    }

    pub fn irq_force_mask(&self) -> u32 {
        self.irq_force_mask.get()
    }

    pub fn set_irq_force_mask(&self, value: u32) {
        self.irq_force_mask.set(value);
    }

    pub fn irq_status(&self) -> u32 {
        self.irq_status.get()
    }

    pub fn irq_value(&self) -> bool {
        let value =
            (self.irq_status.get() & self.irq_enable_mask.get()) | self.irq_force_mask.get();
        apply_override(value != 0, (self.ctrl.get() >> 28) & 0x3)
    }

    pub fn status(&self) -> u32 {
        let mut status = 0;
        if self.irq_value() {
            status |= 1 << 26;
        }
        if (self.irq_status.get() & self.irq_enable_mask.get()) | self.irq_force_mask.get() != 0 {
            status |= 1 << 24;
        }
        if self.input_value() {
            status |= 1 << 19;
        }
        if self.raw_input_value.get() {
            status |= 1 << 17;
        }
        status
    }

    pub fn state(&self) -> GpioPinState {
        if self.pulldown_enabled() && self.pullup_enabled() {
            GpioPinState::InputBusKeeper
        } else if self.pulldown_enabled() {
            GpioPinState::InputPullDown
        } else if self.pullup_enabled() {
            GpioPinState::InputPullUp
        } else {
            GpioPinState::Input
        }
    }

    pub fn set_input_value(&self, value: bool) {
        self.raw_input_value.set(value);
        if value && self.input_enable() {
            self.irq_status
                .set((self.irq_status.get() | IRQ_EDGE_HIGH | IRQ_LEVEL_HIGH) & !IRQ_LEVEL_LOW);
        } else {
            self.irq_status
                .set((self.irq_status.get() | IRQ_EDGE_LOW | IRQ_LEVEL_LOW) & !IRQ_LEVEL_HIGH);
        }
    }

    pub fn update_irq_value(&self, value: u32) {
        let mut status = self.irq_status.get();
        if value & IRQ_EDGE_LOW != 0 {
            status &= !IRQ_EDGE_LOW;
        }
        if value & IRQ_EDGE_HIGH != 0 {
            status &= !IRQ_EDGE_HIGH;
        }
        self.irq_status.set(status);
    }

    pub fn check_changed(&self) -> Option<(GpioPinState, GpioPinState)> {
        let state = self.state();
        let old = self.last_state.get();
        if state != old {
            self.last_state.set(state);
            Some((state, old))
        } else {
            None
        }
    }
}
