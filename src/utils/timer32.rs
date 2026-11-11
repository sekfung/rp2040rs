use std::cell::Cell;
use std::rc::Rc;

/// Minimal timer-frequency handle used by PPB/PWM until the full timer
/// implementation lands in the timer task.
#[derive(Clone)]
pub struct Timer32 {
    frequency: Rc<Cell<f64>>,
}

impl Timer32 {
    pub fn new(frequency: f64) -> Self {
        Self {
            frequency: Rc::new(Cell::new(frequency)),
        }
    }

    pub fn frequency(&self) -> f64 {
        self.frequency.get()
    }

    pub fn set_frequency(&self, frequency: f64) {
        self.frequency.set(frequency);
    }
}
