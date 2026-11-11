use std::cell::RefCell;
use std::rc::Rc;

use rp2040rs::clock::{Clock, MockClock, SimulationClock};

#[test]
fn alarms_fire_in_due_time_order() {
    let clock = SimulationClock::new();
    let fired = Rc::new(RefCell::new(Vec::new()));

    let first = fired.clone();
    let mut alarm_a = clock.create_alarm(Box::new(move || first.borrow_mut().push("a")));
    let second = fired.clone();
    let mut alarm_b = clock.create_alarm(Box::new(move || second.borrow_mut().push("b")));

    alarm_a.schedule(20.0);
    alarm_b.schedule(10.0);
    clock.tick(30.0);

    assert_eq!(*fired.borrow(), vec!["b", "a"]);
    assert_eq!(clock.nanos(), 30.0);
}

#[test]
fn equal_due_alarms_fire_newest_first() {
    let clock = SimulationClock::new();
    let fired = Rc::new(RefCell::new(Vec::new()));

    let first = fired.clone();
    let mut alarm_a = clock.create_alarm(Box::new(move || first.borrow_mut().push("a")));
    let second = fired.clone();
    let mut alarm_b = clock.create_alarm(Box::new(move || second.borrow_mut().push("b")));

    alarm_a.schedule(10.0);
    alarm_b.schedule(10.0);
    clock.tick(10.0);

    assert_eq!(*fired.borrow(), vec!["b", "a"]);
}

#[test]
fn cancelled_alarms_do_not_fire() {
    let clock = SimulationClock::new();
    let fired = Rc::new(RefCell::new(0));
    let counter = fired.clone();
    let mut alarm = clock.create_alarm(Box::new(move || {
        *counter.borrow_mut() += 1;
    }));
    alarm.schedule(5.0);
    alarm.cancel();
    clock.tick(10.0);
    assert_eq!(*fired.borrow(), 0);
}

#[test]
fn rescheduling_moves_an_alarm() {
    let clock = SimulationClock::new();
    let fired = Rc::new(RefCell::new(Vec::new()));
    let first = fired.clone();
    let mut alarm = clock.create_alarm(Box::new(move || first.borrow_mut().push("fired")));
    alarm.schedule(5.0);
    alarm.schedule(15.0);
    clock.tick(10.0);
    assert!(fired.borrow().is_empty());
    clock.tick(20.0);
    assert_eq!(*fired.borrow(), vec!["fired"]);
}

#[test]
fn mock_clock_advances_in_microseconds() {
    let clock = MockClock::new();
    clock.advance(250);
    assert_eq!(clock.nanos(), 250_000.0);
}
