use super::{Alarm, AlarmCallback, Clock, SimulationClock};

pub struct MockClock {
    inner: SimulationClock,
}

impl MockClock {
    pub fn new() -> Self {
        Self {
            inner: SimulationClock::new(),
        }
    }

    pub fn advance(&self, delta_micros: u64) {
        self.inner.tick(delta_micros as f64 * 1000.0);
    }
}

impl Default for MockClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for MockClock {
    fn nanos(&self) -> f64 {
        self.inner.nanos()
    }

    fn create_alarm(&self, callback: AlarmCallback) -> Alarm {
        self.inner.create_alarm(callback)
    }
}
