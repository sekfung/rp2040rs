use std::cell::Cell;
use std::rc::Rc;

pub const FUNCTION_PWM: u32 = 4;
pub const FUNCTION_SIO: u32 = 5;
pub const FUNCTION_PIO0: u32 = 6;
pub const FUNCTION_PIO1: u32 = 7;

pub const SOURCE_PWM: usize = 0;
pub const SOURCE_SIO: usize = 1;
pub const SOURCE_PIO0: usize = 2;
pub const SOURCE_PIO1: usize = 3;

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
    source_output_value: Rc<[Cell<u32>; 4]>,
    source_output_enable: Rc<[Cell<u32>; 4]>,
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
            source_output_value: Rc::new(std::array::from_fn(|_| Cell::new(0))),
            source_output_enable: Rc::new(std::array::from_fn(|_| Cell::new(0))),
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

    #[inline]
    pub(crate) fn pio_input_value(&self) -> bool {
        let ctrl = self.ctrl.get();
        if let Some(source) = Self::source_for_function(ctrl & 0x1f) {
            let bit = 1 << self.index;
            if self.source_output_enable[source].get() & bit != 0 {
                return self.source_output_value[source].get() & bit != 0;
            }
        }
        let value = self.raw_input_value.get() && self.input_enable();
        apply_override(value, (ctrl >> 16) & 0x3)
    }

    fn active_source(&self) -> Option<usize> {
        Self::source_for_function(self.function_select())
    }

    fn source_for_function(function: u32) -> Option<usize> {
        match function {
            FUNCTION_PWM => Some(SOURCE_PWM),
            FUNCTION_SIO => Some(SOURCE_SIO),
            FUNCTION_PIO0 => Some(SOURCE_PIO0),
            FUNCTION_PIO1 => Some(SOURCE_PIO1),
            _ => None,
        }
    }

    /// Update one peripheral's contribution to this pin. The effective pad
    /// output is selected by IO_BANK0 FUNCSEL, matching upstream gpio-pin.ts.
    pub fn set_source_output(&self, source: usize, value: bool, enable: bool) {
        let bit = 1u32 << self.index;
        let value_cell = &self.source_output_value[source];
        let enable_cell = &self.source_output_enable[source];
        if value {
            value_cell.set(value_cell.get() | bit);
        } else {
            value_cell.set(value_cell.get() & !bit);
        }
        if enable {
            enable_cell.set(enable_cell.get() | bit);
            self.raw_input_value.set(value);
        } else {
            enable_cell.set(enable_cell.get() & !bit);
        }
    }

    pub fn clear_source_output(&self, source: usize) {
        let bit = 1u32 << self.index;
        let value_cell = &self.source_output_value[source];
        let enable_cell = &self.source_output_enable[source];
        value_cell.set(value_cell.get() & !bit);
        enable_cell.set(enable_cell.get() & !bit);
    }

    /// Legacy alias used by the SIO path before function selection was added.
    pub fn set_output(&self, value: bool, enable: bool) {
        self.set_source_output(SOURCE_SIO, value, enable);
    }

    pub fn raw_output_value(&self) -> bool {
        self.active_source()
            .map(|source| self.source_output_value[source].get() & (1u32 << self.index) != 0)
            .unwrap_or(false)
    }

    pub fn raw_output_enable(&self) -> bool {
        self.active_source()
            .map(|source| self.source_output_enable[source].get() & (1u32 << self.index) != 0)
            .unwrap_or(false)
    }

    pub fn output_value(&self) -> bool {
        apply_override(self.raw_output_value(), (self.ctrl.get() >> 8) & 0x3)
    }

    pub fn output_enable(&self) -> bool {
        apply_override(self.raw_output_enable(), (self.ctrl.get() >> 12) & 0x3)
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
        if self.output_enable() {
            status |= 1 << 13;
        }
        if self.raw_output_enable() {
            status |= 1 << 12;
        }
        if self.output_value() {
            status |= 1 << 9;
        }
        if self.raw_output_value() {
            status |= 1 << 8;
        }
        status
    }

    pub fn state(&self) -> GpioPinState {
        if self.output_enable() {
            if self.output_value() {
                GpioPinState::High
            } else {
                GpioPinState::Low
            }
        } else if self.pulldown_enabled() && self.pullup_enabled() {
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
