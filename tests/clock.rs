use std::cell::{Cell, RefCell};
use std::rc::Rc;

use rp2040rs::clock::{Clock, MockClock, SimulationClock};
use rp2040rs::rp2040::Rp2040;

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

    alarm_b.schedule(10.0);
    alarm_a.schedule(10.0);
    clock.tick(10.0);

    assert_eq!(*fired.borrow(), vec!["a", "b"]);
}

#[test]
fn rescheduling_to_equal_due_moves_alarm_before_existing() {
    let clock = SimulationClock::new();
    let fired = Rc::new(RefCell::new(Vec::new()));

    let first = fired.clone();
    let mut alarm_a = clock.create_alarm(Box::new(move || first.borrow_mut().push("a")));
    let second = fired.clone();
    let mut alarm_b = clock.create_alarm(Box::new(move || second.borrow_mut().push("b")));

    alarm_a.schedule(10.0);
    alarm_b.schedule(10.0);
    alarm_a.schedule(10.0);
    clock.tick(10.0);

    assert_eq!(*fired.borrow(), vec!["a", "b"]);
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

#[test]
fn clock_listener_can_unsubscribe_during_notification() {
    const CLK_SYS_CTRL: u32 = 0x4000_803c;

    let rp2040 = Rp2040::new();
    let listener_id = Rc::new(Cell::new(None));
    let calls = Rc::new(Cell::new(0));

    let callback_rp2040 = rp2040.clone();
    let callback_id = listener_id.clone();
    let callback_calls = calls.clone();
    let id = rp2040.add_clock_listener(Box::new(move |_, _| {
        callback_calls.set(callback_calls.get() + 1);
        if let Some(id) = callback_id.get() {
            callback_rp2040.remove_clock_listener(id);
        }
    }));
    listener_id.set(Some(id));

    // clk_sys = xosc (12 MHz), which changes it from the 125 MHz default.
    rp2040.write_u32(CLK_SYS_CTRL, 0x1 | (0x3 << 5));
    assert_eq!(calls.get(), 1);

    // A second change must not fire the removed listener again.
    rp2040.set_xosc_freq(24_000_000.0);
    assert_eq!(calls.get(), 1);
}

#[test]
fn panicking_clock_listener_does_not_drop_the_registry() {
    const CLK_SYS_CTRL: u32 = 0x4000_803c;

    let rp2040 = Rp2040::new();
    let id = rp2040.add_clock_listener(Box::new(|_, _| panic!("listener failed")));

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rp2040.write_u32(CLK_SYS_CTRL, 0x1 | (0x3 << 5));
    }));
    assert!(result.is_err());
    assert!(rp2040.remove_clock_listener(id));
}
