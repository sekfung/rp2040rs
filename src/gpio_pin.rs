use std::cell::Cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpioPinState {
    Low,
    High,
    Input,
    InputPullUp,
    InputPullDown,
    InputBusKeeper,
}

/// Minimal GPIO pin state used by SIO and, later, PIO/PWM.
///
/// The full pad/function/IRQ model is added with the remaining peripherals;
/// this type intentionally keeps only the input value and pin index for now.
#[derive(Clone, Debug)]
pub struct GpioPin {
    index: usize,
    input_value: Cell<bool>,
}

impl GpioPin {
    pub fn new(index: usize) -> Self {
        Self {
            index,
            input_value: Cell::new(false),
        }
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn input_value(&self) -> bool {
        self.input_value.get()
    }

    pub fn set_input_value(&self, value: bool) {
        self.input_value.set(value);
    }
}
