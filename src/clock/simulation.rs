use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

use super::{Alarm, AlarmCallback, Clock};

type SharedCallback = Rc<RefCell<AlarmCallback>>;

pub(crate) struct ClockState {
    nanos: f64,
    next_order: u64,
    alarms: Vec<AlarmEntry>,
    // The next alarm is last, so firing it is a pop that retains capacity.
    scheduled: Vec<Deadline>,
}

struct AlarmEntry {
    deadline: Option<Deadline>,
    callback: SharedCallback,
}

#[derive(Clone, Copy)]
struct Deadline {
    due: f64,
    order: u64,
    id: usize,
}

impl Deadline {
    fn cmp(&self, other: &Self) -> Ordering {
        // NaN deadlines never enter the queue. Equal deadlines fire newest first.
        other
            .due
            .partial_cmp(&self.due)
            .unwrap()
            .then_with(|| self.order.cmp(&other.order))
            .then_with(|| other.id.cmp(&self.id))
    }
}

impl ClockState {
    fn deadline_index(&self, deadline: &Deadline) -> Result<usize, usize> {
        self.scheduled.binary_search_by(|entry| entry.cmp(deadline))
    }

    fn pop_due_alarm(&mut self, target: f64) -> Option<SharedCallback> {
        let next = self.scheduled.last()?;
        if next.due <= target {
            let deadline = self.scheduled.pop().unwrap();
            self.nanos = deadline.due;
            let alarm = &mut self.alarms[deadline.id];
            alarm.deadline = None;
            Some(alarm.callback.clone())
        } else {
            None
        }
    }

    fn cancel_alarm(&mut self, id: usize) {
        if let Some(deadline) = self.alarms[id].deadline.take() {
            let index = self.deadline_index(&deadline).unwrap();
            self.scheduled.remove(index);
        }
    }
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
                next_order: 0,
                alarms: Vec::new(),
                scheduled: Vec::new(),
            })),
        }
    }

    pub fn tick(&self, delta_nanos: f64) {
        let mut state = self.state.borrow_mut();
        let target = state.nanos + delta_nanos;
        while let Some(callback) = state.pop_due_alarm(target) {
            drop(state);
            (callback.borrow_mut())();
            state = self.state.borrow_mut();
        }
        state.nanos = target;
    }

    pub fn nanos_to_next_alarm(&self) -> Option<f64> {
        let state = self.state.borrow();
        let next = state.scheduled.last()?.due - state.nanos;
        next.is_finite().then_some(next)
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
        let id = state.alarms.len();
        state.alarms.push(AlarmEntry {
            deadline: None,
            callback: Rc::new(RefCell::new(callback)),
        });
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

pub(crate) fn schedule_alarm(clock: &Rc<RefCell<ClockState>>, id: usize, delta_nanos: f64) {
    let mut state = clock.borrow_mut();
    state.cancel_alarm(id);
    let due = state.nanos + delta_nanos;
    if !due.is_nan() {
        let deadline = Deadline {
            due,
            order: state.next_order,
            id,
        };
        state.next_order += 1;
        let index = state.deadline_index(&deadline).unwrap_err();
        state.scheduled.insert(index, deadline);
        state.alarms[id].deadline = Some(deadline);
    }
}

pub(crate) fn cancel_alarm(clock: &Rc<RefCell<ClockState>>, id: usize) {
    clock.borrow_mut().cancel_alarm(id);
}
