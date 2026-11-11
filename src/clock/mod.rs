mod mock;
mod simulation;

pub use mock::MockClock;
pub use simulation::SimulationClock;

use std::cell::RefCell;
use std::rc::Weak;

pub type AlarmCallback = Box<dyn FnMut()>;

pub trait Clock: 'static {
    fn nanos(&self) -> f64;
    fn create_alarm(&self, callback: AlarmCallback) -> Alarm;
}

#[derive(Clone)]
pub struct Alarm {
    pub(crate) clock: Weak<RefCell<simulation::ClockState>>,
    pub(crate) id: u64,
}

impl Alarm {
    pub fn schedule(&mut self, delta_nanos: f64) {
        if let Some(clock) = self.clock.upgrade() {
            simulation::schedule_alarm(&clock, self.id, delta_nanos);
        }
    }

    pub fn cancel(&mut self) {
        if let Some(clock) = self.clock.upgrade() {
            simulation::cancel_alarm(&clock, self.id);
        }
    }
}
