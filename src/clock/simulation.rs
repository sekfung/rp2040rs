use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use super::{Alarm, AlarmCallback, Clock};

pub(crate) struct ClockState {
    pub(crate) nanos: f64,
    pub(crate) next_id: u64,
    pub(crate) next_order: u64,
    pub(crate) alarms: BTreeMap<u64, AlarmEntry>,
    pub(crate) callbacks: BTreeMap<u64, Rc<RefCell<AlarmCallback>>>,
}

pub(crate) struct AlarmEntry {
    pub(crate) due: f64,
    pub(crate) order: u64,
    pub(crate) scheduled: bool,
}

#[derive(Clone)]
pub struct SimulationClock {
    state: Rc<RefCell<ClockState>>,
}

impl SimulationClock {
    pub fn new() -> Self {
        Self {
            state: Rc::new(RefCell::new(ClockState {
                nanos: 0.0,
                next_id: 0,
                next_order: 0,
                alarms: BTreeMap::new(),
                callbacks: BTreeMap::new(),
            })),
        }
    }

    pub fn tick(&self, delta_nanos: f64) {
        let target = self.state.borrow().nanos + delta_nanos;
        loop {
            let due_id = {
                let state = self.state.borrow();
                state
                    .alarms
                    .iter()
                    .filter(|(_, entry)| entry.scheduled && entry.due <= target)
                    .min_by(|(_, a), (_, b)| {
                        a.due
                            .partial_cmp(&b.due)
                            .unwrap_or(std::cmp::Ordering::Equal)
                            .then_with(|| b.order.cmp(&a.order))
                    })
                    .map(|(id, _)| *id)
            };
            let Some(id) = due_id else {
                break;
            };
            let callback = {
                let mut state = self.state.borrow_mut();
                if let Some(entry) = state.alarms.get_mut(&id) {
                    entry.scheduled = false;
                    state.nanos = entry.due;
                }
                state.callbacks.get(&id).cloned()
            };
            if let Some(callback) = callback {
                (callback.borrow_mut())();
            }
        }
        self.state.borrow_mut().nanos = target;
    }

    pub fn nanos_to_next_alarm(&self) -> Option<f64> {
        let state = self.state.borrow();
        let next = state
            .alarms
            .values()
            .filter(|entry| entry.scheduled)
            .map(|entry| entry.due - state.nanos)
            .fold(f64::INFINITY, f64::min);
        if next.is_finite() {
            Some(next)
        } else {
            None
        }
    }
}

impl Default for SimulationClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SimulationClock {
    fn nanos(&self) -> f64 {
        self.state.borrow().nanos
    }

    fn create_alarm(&self, callback: AlarmCallback) -> Alarm {
        let mut state = self.state.borrow_mut();
        let id = state.next_id;
        state.next_id += 1;
        state.alarms.insert(
            id,
            AlarmEntry {
                due: 0.0,
                order: 0,
                scheduled: false,
            },
        );
        state.callbacks.insert(id, Rc::new(RefCell::new(callback)));
        Alarm {
            clock: Rc::downgrade(&self.state),
            id,
        }
    }

    fn tick(&self, delta_nanos: f64) {
        SimulationClock::tick(self, delta_nanos);
    }

    fn nanos_to_next_alarm(&self) -> Option<f64> {
        SimulationClock::nanos_to_next_alarm(self)
    }
}

pub(crate) fn schedule_alarm(clock: &Rc<RefCell<ClockState>>, id: u64, delta_nanos: f64) {
    let mut state = clock.borrow_mut();
    let nanos = state.nanos;
    let order = state.next_order;
    state.next_order += 1;
    if let Some(entry) = state.alarms.get_mut(&id) {
        entry.due = nanos + delta_nanos;
        entry.order = order;
        entry.scheduled = true;
    }
}

pub(crate) fn cancel_alarm(clock: &Rc<RefCell<ClockState>>, id: u64) {
    if let Some(entry) = clock.borrow_mut().alarms.get_mut(&id) {
        entry.scheduled = false;
    }
}
