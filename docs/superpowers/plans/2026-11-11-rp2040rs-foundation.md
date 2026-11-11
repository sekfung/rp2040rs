# rp2040rs Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the foundation of the rp2040js port: vendored reference, safe crate scaffold, logging/bit/time/FIFO utilities, clock alarm engine, Cortex-M0+ opcode encoders, and PIO opcode encoders, with 91 ported unit tests green.

**Architecture:** A single safe Rust library crate with focused utility modules and a `Clock` trait backed by a `Rc<RefCell<...>>` simulation clock. Opcode modules are exact one-to-one ports of upstream `utils/assembler.ts` and `utils/pio-assembler.ts`; tests assert upstream 16- and 32-bit encodings.

**Tech Stack:** Rust 2021, standard library only, `cargo test`, `cargo clippy`, `cargo build --release`.

**Spec:** `docs/superpowers/specs/2026-11-11-rp2040rs-design.md`

## Global Constraints

- Upstream pin: `wokwi/rp2040js` v1.4.1, commit `8693315df527adce8bf2bc315aeb4a1ca61c02a3`.
- Upstream license is MIT (Uri Shaked and contributors); keep `LICENSE` and `NOTICE`.
- Rust edition 2021; `#![forbid(unsafe_code)]`; no runtime crates.
- Final acceptance is `cargo test --lib` reporting exactly 322 upstream-ported tests; this plan delivers the first 91.
- Foundation unit-test counts: `utils/assembler.rs` 78, `utils/pio_assembler.rs` 9, `utils/fifo.rs` 3, `utils/time.rs` 1.
- Additional tests for non-upstream utilities belong in `tests/` so they do not change the 322-case unit count.
- The emulator is single-threaded; use `Rc`/`Weak`/`RefCell`, not `Send`/`Sync` state.
- Use explicit 32-bit wrapping arithmetic wherever the upstream JavaScript uses 32-bit shifts or bitwise operators.
- Do not modify vendored upstream sources; they are the behavioural reference.

## Scope Decomposition

The spec is split into four implementation plans, each producing a green, independently testable stage:

1. **Foundation (this plan):** utilities, clock, opcode encoders, 91 unit tests.
2. **CPU and bus:** Cortex-M0+ core, memory map, PPB/NVIC, 148 unit tests (cumulative 239).
3. **Core peripherals:** CLOCKS, DMA, PIO, PWM, TIMER, UART, SIO, USB CDC, 83 unit tests (cumulative 322).
4. **Host and remaining peripherals:** ADC/I2C/SPI/SSI/RTC/Watchdog/etc., GDB, Simulator, CLI, integration smoke tests.

Write plan 2 only after plan 1 is green, plan 3 after plan 2, and plan 4 after plan 3. This keeps every plan reviewable and executable on its own.

## File Structure

```text
Cargo.toml                         package metadata, lib and binary targets
LICENSE                            upstream MIT text
NOTICE                             attribution to rp2040js
README.md                          crate overview and foundation usage
src/lib.rs                         module declarations and VERSION
src/bin/rp2040rs/main.rs           temporary version-printing binary
src/clock/mod.rs                   Clock trait, Alarm handle, re-exports
src/clock/simulation.rs            SimulationClock and alarm queue
src/clock/mock.rs                  MockClock
src/utils/mod.rs                   utility re-exports
src/utils/bit.rs                   bit, s32, u32 helpers
src/utils/logging.rs               Logger trait, LogLevel, ConsoleLogger
src/utils/fifo.rs                  FIFO and 3 ported tests
src/utils/time.rs                  TimeParts, format_time, get_current_microseconds
src/utils/assembler.rs             Cortex-M0+ opcode encoders and 78 tests
src/utils/pio_assembler.rs         PIO opcode encoders/constants and 9 tests
vendor/rp2040js/                   unmodified upstream source at the pinned commit
tests/smoke.rs                     crate scaffold smoke test
tests/utils.rs                     logger/bit integration tests
tests/clock.rs                     clock alarm integration tests
docs/parity.md                     initial test/API parity map
```

---

### Task 1: Vendor the reference and create the crate scaffold

**Files:**
- Create: `Cargo.toml`
- Create: `LICENSE`
- Create: `NOTICE`
- Create: `README.md`
- Create: `src/lib.rs`
- Create: `src/bin/rp2040rs/main.rs`
- Create: `tests/smoke.rs`
- Create: `.gitignore`
- Create: `vendor/rp2040js/` from the pinned upstream tree

**Interfaces:**
- Consumes: nothing.
- Produces: package `rp2040rs`, `rp2040rs::VERSION`, binary target `rp2040rs`, vendored reference tree.

- [ ] **Step 1: Copy the pinned upstream reference**

If `/tmp/rp2040js-upstream` exists, run:

```bash
mkdir -p vendor/rp2040js
tar -C /tmp/rp2040js-upstream \
  --exclude=node_modules --exclude=node_modules.partial --exclude=.git \
  -cf - . | tar -C vendor/rp2040js -xf -
test -f vendor/rp2040js/src/rp2040.spec.ts
test -f vendor/rp2040js/LICENSE
```

If it does not exist, download the pinned archive first:

```bash
curl -sSL -o /tmp/rp2040js-src.tar.gz \
  https://codeload.github.com/wokwi/rp2040js/tar.gz/8693315df527adce8bf2bc315aeb4a1ca61c02a3
mkdir -p /tmp/rp2040js-upstream
tar -xzf /tmp/rp2040js-src.tar.gz -C /tmp/rp2040js-upstream --strip-components=1
```

Then repeat the copy commands above.

- [ ] **Step 2: Write the failing scaffold test**

Create `tests/smoke.rs`:

```rust
#[test]
fn crate_version_is_set() {
    assert_eq!(rp2040rs::VERSION, env!("CARGO_PKG_VERSION"));
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test --test smoke`

Expected: FAIL because `src/lib.rs` or `rp2040rs::VERSION` does not exist yet.

- [ ] **Step 4: Create the package metadata**

Create `Cargo.toml`:

```toml
[package]
name = "rp2040rs"
version = "0.1.0"
edition = "2021"
license = "MIT"
description = "Rust port of the rp2040js Raspberry Pi Pico emulator"
repository = "https://github.com/wokwi/rp2040js"

[lib]
path = "src/lib.rs"

[[bin]]
name = "rp2040rs"
path = "src/bin/rp2040rs/main.rs"

[dependencies]
```

Create `LICENSE` by copying `vendor/rp2040js/LICENSE` verbatim. Create `NOTICE`:

```text
rp2040rs is a Rust port of rp2040js.

Upstream: https://github.com/wokwi/rp2040js
Version: 1.4.1
Commit: 8693315df527adce8bf2bc315aeb4a1ca61c02a3
Copyright (c) 2021-2023, Uri Shaked and contributors.
Released under the MIT license. See LICENSE.
```

Create `.gitignore`:

```text
/target
*.hex
*.uf2
```

Create `README.md`:

```markdown
# rp2040rs

Safe Rust port of the [rp2040js](https://github.com/wokwi/rp2040js)
Raspberry Pi Pico emulator.

Upstream pin: 1.4.1, commit
`8693315df527adce8bf2bc315aeb4a1ca61c02a3`.

This repository is being built in four green stages. The current stage is the
foundation: utilities, clock, and opcode encoders.

## License

MIT. See `LICENSE` and `NOTICE`.
```

Create `src/lib.rs`:

```rust
#![forbid(unsafe_code)]

//! Safe Rust port of the rp2040js Raspberry Pi Pico emulator.

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
```

Create `src/bin/rp2040rs/main.rs`:

```rust
fn main() {
    println!("rp2040rs {}", rp2040rs::VERSION);
}
```

- [ ] **Step 5: Run the scaffold test and release build**

Run: `cargo test --test smoke`

Expected: PASS.

Run: `cargo build --release`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock LICENSE NOTICE README.md .gitignore \
  src/lib.rs src/bin/rp2040rs/main.rs tests/smoke.rs vendor/rp2040js
git commit -m "chore: scaffold rp2040rs and vendor upstream reference"
```

---

### Task 2: Add logging and 32-bit helpers

**Files:**
- Create: `src/utils/mod.rs`
- Create: `src/utils/bit.rs`
- Create: `src/utils/logging.rs`
- Modify: `src/lib.rs`
- Create: `tests/utils.rs`

**Interfaces:**
- Consumes: `rp2040rs::VERSION` scaffold.
- Produces: `utils::{bit, s32, u32}`, `Logger`, `LogLevel`, `ConsoleLogger`.

- [ ] **Step 1: Write the failing integration tests**

Create `tests/utils.rs`:

```rust
use std::sync::{Arc, Mutex};

use rp2040rs::utils::logging::{Logger, LogLevel, ConsoleLogger};
use rp2040rs::utils::{bit, s32, u32};

#[test]
fn bit_helpers_use_32_bit_semantics() {
    assert_eq!(bit(0), 1);
    assert_eq!(bit(31), 0x8000_0000);
    assert_eq!(s32(0xffff_ffff), -1);
    assert_eq!(u32(-1), 0xffff_ffff);
}

#[test]
fn console_logger_throws_on_error_when_enabled() {
    let logger = ConsoleLogger::new(LogLevel::Debug, true);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        logger.error("UART", "boom");
    }));
    assert!(result.is_err());
}

#[derive(Default)]
struct RecordingLogger {
    messages: Mutex<Vec<(LogLevel, String, String)>>,
}

impl Logger for RecordingLogger {
    fn debug(&self, component: &str, message: &str) {
        self.messages
            .lock()
            .unwrap()
            .push((LogLevel::Debug, component.into(), message.into()));
    }
    fn info(&self, component: &str, message: &str) {
        self.messages
            .lock()
            .unwrap()
            .push((LogLevel::Info, component.into(), message.into()));
    }
    fn warn(&self, component: &str, message: &str) {
        self.messages
            .lock()
            .unwrap()
            .push((LogLevel::Warn, component.into(), message.into()));
    }
    fn error(&self, component: &str, message: &str) {
        self.messages
            .lock()
            .unwrap()
            .push((LogLevel::Error, component.into(), message.into()));
    }
}

#[test]
fn logger_trait_accepts_a_recorder() {
    let logger = Arc::new(RecordingLogger::default());
    logger.warn("PIO", "stalled");
    let messages = logger.messages.lock().unwrap();
    assert_eq!(messages[0].0, LogLevel::Warn);
    assert_eq!(messages[0].1, "PIO");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --test utils`

Expected: FAIL because `utils` is not a module yet.

- [ ] **Step 3: Implement `bit.rs`**

Create `src/utils/bit.rs`:

```rust
pub fn bit(n: u32) -> u32 {
    1u32.wrapping_shl(n)
}

pub fn s32(n: u32) -> i32 {
    n as i32
}

pub fn u32(n: i32) -> u32 {
    n as u32
}
```

- [ ] **Step 4: Implement `logging.rs`**

Create `src/utils/logging.rs`:

```rust
use std::fmt;

use crate::utils::time::format_time_now;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

pub trait Logger: 'static {
    fn debug(&self, component: &str, message: &str);
    fn info(&self, component: &str, message: &str);
    fn warn(&self, component: &str, message: &str);
    fn error(&self, component: &str, message: &str);
}

pub struct ConsoleLogger {
    current_level: LogLevel,
    throw_on_error: bool,
}

impl ConsoleLogger {
    pub fn new(current_level: LogLevel, throw_on_error: bool) -> Self {
        Self {
            current_level,
            throw_on_error,
        }
    }

    fn enabled(&self, level: LogLevel) -> bool {
        level >= self.current_level
    }

    fn format_message(&self, component: &str, message: &str) -> String {
        format!("{} [{}] {}", format_time_now(), component, message)
    }
}

impl Logger for ConsoleLogger {
    fn debug(&self, component: &str, message: &str) {
        if self.enabled(LogLevel::Debug) {
            eprintln!("{}", self.format_message(component, message));
        }
    }

    fn info(&self, component: &str, message: &str) {
        if self.enabled(LogLevel::Info) {
            eprintln!("{}", self.format_message(component, message));
        }
    }

    fn warn(&self, component: &str, message: &str) {
        if self.enabled(LogLevel::Warn) {
            eprintln!("{}", self.format_message(component, message));
        }
    }

    fn error(&self, component: &str, message: &str) {
        if self.enabled(LogLevel::Error) {
            eprintln!("{}", self.format_message(component, message));
            if self.throw_on_error {
                panic!("[{}] {}", component, message);
            }
        }
    }
}

impl fmt::Debug for ConsoleLogger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConsoleLogger")
            .field("current_level", &self.current_level)
            .field("throw_on_error", &self.throw_on_error)
            .finish()
    }
}
```

Create `src/utils/mod.rs`:

```rust
pub mod bit;
pub mod fifo;
pub mod logging;
pub mod time;

pub use bit::{bit, s32, u32};
pub use fifo::Fifo;
pub use logging::{ConsoleLogger, LogLevel, Logger};
pub use time::{format_time, get_current_microseconds, TimeParts};
```

`fifo` and `time` are added in later tasks; declare them only when those files exist. For this task use:

```rust
pub mod bit;
pub mod logging;
pub mod time;

pub use bit::{bit, s32, u32};
pub use logging::{ConsoleLogger, LogLevel, Logger};
pub use time::format_time_now;
```

- [ ] **Step 5: Add a temporary time formatter**

`logging.rs` needs `format_time_now()` before Task 4 implements the full time module. Create `src/utils/time.rs`:

```rust
use std::time::{SystemTime, UNIX_EPOCH};

pub fn format_time_now() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{:02}:{:02}:{:02}.{:03} ", 0, 0, 0, millis % 1000)
}
```

Task 4 replaces this function with the full `format_time` implementation and keeps `format_time_now` as a wrapper.

- [ ] **Step 6: Declare modules in `lib.rs`**

Update `src/lib.rs`:

```rust
#![forbid(unsafe_code)]

//! Safe Rust port of the rp2040js Raspberry Pi Pico emulator.

pub mod utils;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
```

- [ ] **Step 7: Run the integration tests**

Run: `cargo test --test utils`

Expected: 3 tests PASS.

- [ ] **Step 8: Commit**

```bash
git add src/lib.rs src/utils tests/utils.rs
git commit -m "feat: add logging and 32-bit helpers"
```

---

### Task 3: Port FIFO and its 3 upstream tests

**Files:**
- Create: `src/utils/fifo.rs`
- Modify: `src/utils/mod.rs`

**Interfaces:**
- Consumes: nothing beyond the crate scaffold.
- Produces: `Fifo::new(size)`, `size()`, `item_count()`, `push(u32)`, `pull() -> u32`, `peek() -> u32`, `reset()`, `empty()`, `full()`, `items() -> Vec<u32>`.

- [ ] **Step 1: Write the failing upstream tests**

Create `src/utils/fifo.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::Fifo;

    #[test]
    fn should_successfully_push_and_pull_4_items() {
        let mut fifo = Fifo::new(3);
        assert!(fifo.empty());
        fifo.push(1);
        assert!(!fifo.empty());
        fifo.push(2);
        assert_eq!(fifo.item_count(), 2);
        assert!(!fifo.full());
        fifo.push(3);
        assert!(fifo.full());
        assert_eq!(fifo.pull(), 1);
        assert!(!fifo.full());
        fifo.push(4);
        assert!(fifo.full());
        assert_eq!(fifo.pull(), 2);
        assert_eq!(fifo.pull(), 3);
        assert!(!fifo.empty());
        assert_eq!(fifo.item_count(), 1);
        assert_eq!(fifo.pull(), 4);
        assert!(!fifo.full());
        assert!(fifo.empty());
    }

    #[test]
    fn peek_should_not_change_fifo_contents() {
        let mut fifo = Fifo::new(3);
        assert!(fifo.empty());
        fifo.push(10);
        assert!(!fifo.empty());
        fifo.push(20);
        assert_eq!(fifo.peek(), 10);
        assert_eq!(fifo.item_count(), 2);
        assert_eq!(fifo.pull(), 10);
    }

    #[test]
    fn items_should_return_all_fifo_contents_in_order() {
        let mut fifo = Fifo::new(3);
        assert!(fifo.empty());
        fifo.push(10);
        fifo.push(20);
        fifo.push(30);
        fifo.pull();
        assert_eq!(fifo.items(), vec![20, 30]);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib utils::fifo`

Expected: FAIL because `Fifo` is not defined.

- [ ] **Step 3: Implement `Fifo` exactly as upstream**

Add above the test module:

```rust
#[derive(Debug, Clone)]
pub struct Fifo {
    buffer: Vec<u32>,
    start: usize,
    used: usize,
}

impl Fifo {
    pub fn new(size: usize) -> Self {
        Self {
            buffer: vec![0; size],
            start: 0,
            used: 0,
        }
    }

    pub fn size(&self) -> usize {
        self.buffer.len()
    }

    pub fn item_count(&self) -> usize {
        self.used
    }

    pub fn push(&mut self, value: u32) {
        let length = self.buffer.len();
        if self.used < length {
            self.buffer[(self.start + self.used) % length] = value;
            self.used += 1;
        }
    }

    pub fn pull(&mut self) -> u32 {
        if self.used == 0 {
            return 0;
        }
        let value = self.buffer[self.start];
        self.start = (self.start + 1) % self.buffer.len();
        self.used -= 1;
        value
    }

    pub fn peek(&self) -> u32 {
        if self.used == 0 {
            0
        } else {
            self.buffer[self.start]
        }
    }

    pub fn reset(&mut self) {
        self.used = 0;
    }

    pub fn empty(&self) -> bool {
        self.used == 0
    }

    pub fn full(&self) -> bool {
        self.used == self.buffer.len()
    }

    pub fn items(&self) -> Vec<u32> {
        (0..self.used)
            .map(|i| self.buffer[(self.start + i) % self.buffer.len()])
            .collect()
    }
}
```

- [ ] **Step 4: Export `Fifo` and run the tests**

Update `src/utils/mod.rs`:

```rust
pub mod bit;
pub mod fifo;
pub mod logging;
pub mod time;

pub use bit::{bit, s32, u32};
pub use fifo::Fifo;
pub use logging::{ConsoleLogger, LogLevel, Logger};
pub use time::format_time_now;
```

Run: `cargo test --lib utils::fifo`

Expected: 3 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/utils/fifo.rs src/utils/mod.rs
git commit -m "feat: port FIFO with upstream tests"
```

---

### Task 4: Port time formatting and its upstream test

**Files:**
- Modify: `src/utils/time.rs`
- Modify: `src/utils/mod.rs`

**Interfaces:**
- Consumes: the temporary `format_time_now` from Task 2.
- Produces: `TimeParts`, `format_time(TimeParts) -> String`, `format_time_now() -> String`, `get_current_microseconds() -> u64`.

- [ ] **Step 1: Write the failing upstream test**

Replace `src/utils/time.rs` with:

```rust
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeParts {
    pub hours: u32,
    pub minutes: u32,
    pub seconds: u32,
    pub milliseconds: u32,
}

#[cfg(test)]
mod tests {
    use super::{format_time, TimeParts};

    #[test]
    fn should_correctly_format_a_timestamp_with_microseconds_padding_with_spaces() {
        let time = TimeParts {
            hours: 4,
            minutes: 55,
            seconds: 2,
            milliseconds: 12,
        };
        assert_eq!(format_time(time), "04:55:02.12 ");
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --lib utils::time`

Expected: FAIL because `format_time` is not defined.

- [ ] **Step 3: Implement formatting and current-time helpers**

Add above the test module:

```rust
pub fn format_time(time: TimeParts) -> String {
    format!(
        "{:02}:{:02}:{:02}.{:<3}",
        time.hours, time.minutes, time.seconds, time.milliseconds
    )
}

pub fn format_time_now() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = duration.as_secs();
    let millis = duration.subsec_millis();
    format!(
        "{:02}:{:02}:{:02}.{:<3}",
        (seconds / 3600) % 24,
        (seconds / 60) % 60,
        seconds % 60,
        millis
    )
}

pub fn get_current_microseconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}
```

The upstream `formatTime` pads milliseconds with **spaces** to width 3, so the
expected string ends with `"12 "`. `{:<3}` reproduces that padding.

- [ ] **Step 4: Export time helpers and run the test**

Update `src/utils/mod.rs`:

```rust
pub use time::{format_time, format_time_now, get_current_microseconds, TimeParts};
```

Run: `cargo test --lib utils::time`

Expected: 1 test PASS.

- [ ] **Step 5: Commit**

```bash
git add src/utils/time.rs src/utils/mod.rs
git commit -m "feat: port time formatting with upstream test"
```

---

### Task 5: Implement the clock and alarm engine

**Files:**
- Create: `src/clock/mod.rs`
- Create: `src/clock/simulation.rs`
- Create: `src/clock/mock.rs`
- Modify: `src/lib.rs`
- Create: `tests/clock.rs`

**Interfaces:**
- Consumes: standard library only.
- Produces: `Clock` trait, `Alarm` handle, `SimulationClock`, `MockClock`.

- [ ] **Step 1: Write the failing clock integration tests**

Create `tests/clock.rs`:

```rust
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test clock`

Expected: FAIL because `rp2040rs::clock` does not exist.

- [ ] **Step 3: Implement the clock trait and alarm handles**

Create `src/clock/mod.rs`:

```rust
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
```

Create `src/clock/simulation.rs`:

```rust
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use super::{Alarm, AlarmCallback, Clock};

pub(crate) struct ClockState {
    pub(crate) nanos: f64,
    pub(crate) next_id: u64,
    pub(crate) alarms: BTreeMap<u64, AlarmEntry>,
    pub(crate) callbacks: BTreeMap<u64, Rc<RefCell<AlarmCallback>>>,
}

pub(crate) struct AlarmEntry {
    pub(crate) due: f64,
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

    pub fn nanos_to_next_alarm(&self) -> f64 {
        let state = self.state.borrow();
        let next = state
            .alarms
            .values()
            .filter(|entry| entry.scheduled)
            .map(|entry| entry.due - state.nanos)
            .fold(f64::INFINITY, f64::min);
        if next.is_finite() {
            next.max(0.0)
        } else {
            0.0
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
                scheduled: false,
            },
        );
        state.callbacks.insert(id, Rc::new(RefCell::new(callback)));
        Alarm {
            clock: Rc::downgrade(&self.state),
            id,
        }
    }
}

pub(crate) fn schedule_alarm(
    clock: &Rc<RefCell<ClockState>>,
    id: u64,
    delta_nanos: f64,
) {
    let mut state = clock.borrow_mut();
    let nanos = state.nanos;
    if let Some(entry) = state.alarms.get_mut(&id) {
        entry.due = nanos + delta_nanos;
        entry.scheduled = true;
    }
}

pub(crate) fn cancel_alarm(clock: &Rc<RefCell<ClockState>>, id: u64) {
    if let Some(entry) = clock.borrow_mut().alarms.get_mut(&id) {
        entry.scheduled = false;
    }
}
```

Create `src/clock/mock.rs`:

```rust
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
```

- [ ] **Step 4: Declare the clock module and run the tests**

Update `src/lib.rs`:

```rust
pub mod clock;
pub mod utils;
```

Run: `cargo test --test clock`

Expected: 4 tests PASS.

- [ ] **Step 5: Run the full suite and commit**

Run: `cargo test`

Expected: 1 smoke test, 3 utility integration tests, 4 clock integration
tests, and the 4 FIFO/time unit tests all pass.

```bash
git add src/clock src/lib.rs tests/clock.rs
git commit -m "feat: add simulation clock and alarm engine"
```

---

### Task 6: Port Cortex-M0+ opcode encoders and their 78 upstream tests

**Files:**
- Create: `src/utils/assembler.rs`
- Modify: `src/utils/mod.rs`

**Interfaces:**
- Consumes: `vendor/rp2040js/src/utils/assembler.ts`.
- Produces: every upstream `opcodeXXX` function as Rust `opcode_xxx`, with the same parameter order and return width. 16-bit results are `u16`; 32-bit results (`BL`, `MRS`, `MSR`, `DMBSY`, `DSBSY`, `ISBSY`, `UDF2`) are `u32`.

- [ ] **Step 1: Write the failing 78-test module**

Append this test module to `src/utils/assembler.rs` (create the file with only test code first):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! encode {
        ($name:ident, $call:expr, $expected:expr) => {
            #[test]
            fn $name() {
                assert_eq!($call, $expected);
            }
        };
    }

    encode!(adcs_r3_r0, opcode_adcs(3, 0), 0x4143);
    encode!(add_r1_sp_4, opcode_add_sp_plus_imm(1, 4), 0xa901);
    encode!(add_sp_12, opcode_add_sp2(12), 0xb003);
    encode!(adds_r0_r3_0, opcode_adds1(0, 3, 0), 0x1c18);
    encode!(adds_r1_r1_r3, opcode_adds_reg(1, 1, 3), 0x18c9);
    encode!(add_r1_ip, opcode_add_reg(1, 12), 0x4461);
    encode!(adds_r1_1, opcode_adds2(1, 1), 0x3101);
    encode!(ands_r5_r0, opcode_ands(5, 0), 0x4005);
    encode!(adr_r4_52, opcode_adr(4, 52), 0xa40d);
    encode!(asrs_r3_r2_31, opcode_asrs(3, 2, 31), 0x17d3);
    encode!(asrs_r3_r4, opcode_asrs_reg(3, 4), 0x4123);
    encode!(b_n_20_t1, opcode_bt1(1, 0x1f8), 0xd1fc);
    encode!(b_n_20_t2, opcode_bt2(0xfec), 0xe7f6);
    encode!(bics_r0_r3, opcode_bics(0, 3), 0x4398);
    encode!(bl_minus_198, opcode_bl(-198), 0xff9df7ff);
    encode!(bl_plus_10, opcode_bl(10), 0xf805f000);
    encode!(bl_minus_3242, opcode_bl(-3242), 0xf9abf7ff);
    encode!(blx_r1, opcode_blx(1), 0x4788);
    encode!(bx_lr, opcode_bx(14), 0x4770);
    encode!(cmn_r5_r6, opcode_cmn(5, 6), 0x42f5);
    encode!(cmp_r5_66, opcode_cmp_imm(5, 66), 0x2d42);
    encode!(cmp_r5_r0, opcode_cmp_reg_t1(5, 0), 0x4285);
    encode!(cmp_ip_r6, opcode_cmp_reg_t2(12, 6), 0x45b4);
    encode!(eors_r1_r3, opcode_eors(1, 3), 0x4059);
    encode!(dmbsy, opcode_dmbsy(), 0x8f50f3bf);
    encode!(dsbsy, opcode_dsbsy(), 0x8f4ff3bf);
    encode!(ldmia_r0_r1_r2, opcode_ldmia(0, (1 << 1) | (1 << 2)), 0xc806);
    encode!(isbsy, opcode_isbsy(), 0x8f6ff3bf);
    encode!(lsls_reg_r5_r0, opcode_lsls_reg(5, 0), 0x4085);
    encode!(lsrs_r1_r1_1, opcode_lsrs(1, 1, 1), 0x0849);
    encode!(lsrs_reg_r0_r4, opcode_lsrs_reg(0, 4), 0x40e0);
    encode!(ldr_imm_r3_r2_24, opcode_ldr_imm(3, 2, 24), 0x6993);
    encode!(ldr_lit_r0_148, opcode_ldr_lit(0, 148), 0x4825);
    encode!(ldr_reg_r3_r3_r4, opcode_ldr_reg(3, 3, 4), 0x591b);
    encode!(ldr_sp_r3_12, opcode_ldr_sp(3, 12), 0x9b03);
    encode!(ldrb_r0_r1_0, opcode_ldrb(0, 1, 0), 0x7808);
    encode!(ldrb_reg_r2_r5_r4, opcode_ldrb_reg(2, 5, 4), 0x5d2a);
    encode!(ldrh_r3_r0_2, opcode_ldrh(3, 0, 2), 0x8843);
    encode!(ldrh_reg_r4_r0_r1, opcode_ldrh_reg(4, 0, 1), 0x5a44);
    encode!(ldrsb_r3_r2_r3, opcode_ldrsb(3, 2, 3), 0x56d3);
    encode!(ldrsh_r5_r3_r5, opcode_ldrsh(5, 3, 5), 0x5f5d);
    encode!(lsls_imm_r5_r5_18, opcode_lsls_imm(5, 5, 18), 0x04ad);
    encode!(mov_r3_r8, opcode_mov(3, 8), 0x4643);
    encode!(movs_r5_128, opcode_movs(5, 128), 0x2580);
    encode!(movs_reg_r6_r5, opcode_movs_reg(6, 5), 0x002e);
    encode!(mrs_r6_primask, opcode_mrs(6, 16), 0x8610f3ef);
    encode!(msr_primask_r6, opcode_msr(16, 6), 0x8810f386);
    encode!(muls_r2_r0, opcode_muls(2, 0), 0x4350);
    encode!(mvns_r3_r3, opcode_mvns(3, 3), 0x43db);
    encode!(orrs_r3_r0, opcode_orrs(3, 0), 0x4303);
    encode!(pop_pc_r0_r1, opcode_pop(true, (1 << 0) | (1 << 1)), 0xbd03);
    encode!(push_lr_r4_r5_r6, opcode_push(true, (1 << 4) | (1 << 5) | (1 << 6)), 0xb570);
    encode!(rev_r3_r1, opcode_rev(3, 1), 0xba0b);
    encode!(rev16_r2_r7, opcode_rev16(2, 7), 0xba7a);
    encode!(revsh_r1_r5, opcode_revsh(1, 5), 0xbae9);
    encode!(ror_r6_r0, opcode_ror(6, 0), 0x41c6);
    encode!(rsbs_r0_r3, opcode_rsbs(0, 3), 0x4258);
    encode!(sbcs_r0_r3, opcode_sbcs(0, 3), 0x4198);
    encode!(stmia_r2_r0, opcode_stmia(2, 1 << 0), 0xc201);
    encode!(str_r6_r4_20, opcode_str(6, 4, 20), 0x6166);
    encode!(str_sp_r1_4, opcode_str_sp(1, 4), 0x9101);
    encode!(str_reg_r2_r1_r4, opcode_str_reg(2, 1, 4), 0x510a);
    encode!(strb_r3_r2_0, opcode_strb(3, 2, 0), 0x7013);
    encode!(strb_reg_r3_r2_r5, opcode_strb_reg(3, 2, 5), 0x5553);
    encode!(strh_r1_r3_4, opcode_strh(1, 3, 4), 0x8099);
    encode!(strh_reg_r1_r3_r2, opcode_strh_reg(1, 3, 2), 0x5299);
    encode!(sub_sp_12, opcode_sub_sp(12), 0xb083);
    encode!(subs_r3_r0_1, opcode_subs1(3, 0, 1), 0x1e43);
    encode!(subs_reg_r1_r1_r0, opcode_subs_reg(1, 1, 0), 0x1a09);
    encode!(subs_r3_13, opcode_subs2(3, 13), 0x3b0d);
    encode!(svc_0, opcode_svc(0), 0xdf00);
    encode!(sxtb_r2_r2, opcode_sxtb(2, 2), 0xb252);
    encode!(sxth_r6_r3, opcode_sxth(6, 3), 0xb21e);
    encode!(tst_r1_r3, opcode_tst(1, 3), 0x4219);
    encode!(udf_1, opcode_udf(1), 0xde01);
    encode!(udf2_0, opcode_udf2(0), 0xa000f7f0);
    encode!(uxtb_r3_r3, opcode_uxtb(3, 3), 0xb2db);
    encode!(uxth_r3_r0, opcode_uxth(3, 0), 0xb283);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib utils::assembler`

Expected: FAIL because the `opcode_*` functions do not exist.

- [ ] **Step 3: Port every opcode function one-to-one**

Open `vendor/rp2040js/src/utils/assembler.ts` and translate each exported
`opcodeXXX` function into the Rust name used by the tests. Keep the exact
parameter order and bit masks. The file is the authoritative bit layout; do
not invent new encodings.

Required translation rules:

- JavaScript `>>>` becomes `u32` arithmetic followed by `as u16` for 16-bit
  results, or remains `u32` for 32-bit results.
- JavaScript `&`, `|`, `^`, `<<` use `u32`/`i32` with explicit masks.
- `opcodeBL` returns `u32`; `opcodeMRS`, `opcodeMSR`, `opcodeDMBSY`,
  `opcodeDSBSY`, `opcodeISBSY`, and `opcodeUDF2` return `u32`.
- All other encoders return `u16`.
- `opcodeNOP`, `opcodeWFI`, and `opcodeYIELD` are not in the 78-test table but
  are required by instruction tests; port them too.

For example, these three functions must be exactly:

```rust
pub fn opcode_adcs(rd: u32, rm: u32) -> u16 {
    (0b0100000101 << 6 | (rm & 0x7) << 3 | (rd & 0x7)) as u16
}

pub fn opcode_add_sp_plus_imm(rd: u32, imm8: u32) -> u16 {
    (0b10101 << 11 | (rd & 0x7) << 8 | ((imm8 >> 2) & 0xff)) as u16
}

pub fn opcode_udf2(imm16: u32) -> u32 {
    let imm12 = imm16 & 0xfff;
    let imm4 = (imm16 >> 12) & 0xf;
    ((0b1111_0111_1111u32 << 4) | imm4 | (0b1010u32 << 28) | (imm12 << 16)) & 0xffff_ffff
}
```

If a translated value differs, compare the Rust expression with the upstream
TypeScript expression and the test's expected word; fix the mask, not the
test.

- [ ] **Step 4: Export the module and run the 78 tests**

Add `pub mod assembler;` to `src/utils/mod.rs`.

Run: `cargo test --lib utils::assembler`

Expected: 78 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/utils/assembler.rs src/utils/mod.rs
git commit -m "feat: port Cortex-M0+ opcode encoders"
```

---

### Task 7: Port PIO assembler constants/functions and their 9 upstream tests

**Files:**
- Create: `src/utils/pio_assembler.rs`
- Modify: `src/utils/mod.rs`

**Interfaces:**
- Consumes: `vendor/rp2040js/src/utils/pio-assembler.ts`.
- Produces: all upstream `PIO_*` constants plus `pio_jmp`, `pio_wait`, `pio_in`,
  `pio_out`, `pio_push`, `pio_pull`, `pio_mov`, `pio_irq`, `pio_set` returning
  `u16`.

- [ ] **Step 1: Write the failing 9-test module**

Create `src/utils/pio_assembler.rs` with this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_correctly_encode_a_jmp_pin_5_pio_instruction() {
        assert_eq!(pio_jmp(PIO_COND_PIN, 5, 0), 0xc5);
    }

    #[test]
    fn should_correctly_encode_a_wait_1_gpio_12_pio_instruction() {
        assert_eq!(pio_wait(true, PIO_WAIT_SRC_GPIO, 12, 0), 0x208c);
    }

    #[test]
    fn should_correctly_encode_an_in_x_12_pio_instruction() {
        assert_eq!(pio_in(PIO_SRC_X, 12, 0), 0x402c);
    }

    #[test]
    fn should_correctly_encode_an_out_y_30_pio_instruction() {
        assert_eq!(pio_out(PIO_DEST_Y, 30, 0), 0x605e);
    }

    #[test]
    fn should_correctly_encode_a_push_iffull_noblock_pio_instruction() {
        assert_eq!(pio_push(true, true, 12), 0x8c60);
    }

    #[test]
    fn should_correctly_encode_a_pull_block_pio_instruction() {
        assert_eq!(pio_pull(true, false, 0), 0x80c0);
    }

    #[test]
    fn should_correctly_encode_a_mov_x_not_status_pio_instruction() {
        assert_eq!(pio_mov(PIO_MOV_DEST_X, PIO_OP_INVERT, PIO_SRC_STATUS, 0), 0xa02d);
    }

    #[test]
    fn should_correctly_encode_an_irq_set_4_pio_instruction() {
        assert_eq!(pio_irq(false, false, 4, 0), 0xc004);
    }

    #[test]
    fn should_correctly_encode_a_set_x_12_pio_instruction() {
        assert_eq!(pio_set(PIO_MOV_DEST_X, 12, 0), 0xe02c);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib utils::pio_assembler`

Expected: FAIL because the functions/constants do not exist.

- [ ] **Step 3: Implement the constants and encoders**

Add above the test module:

```rust
pub const PIO_SRC_PINS: u32 = 0;
pub const PIO_SRC_X: u32 = 1;
pub const PIO_SRC_Y: u32 = 2;
pub const PIO_SRC_NULL: u32 = 3;
pub const PIO_SRC_STATUS: u32 = 5;
pub const PIO_SRC_ISR: u32 = 6;
pub const PIO_SRC_OSR: u32 = 7;

pub const PIO_DEST_PINS: u32 = 0;
pub const PIO_DEST_X: u32 = 1;
pub const PIO_DEST_Y: u32 = 2;
pub const PIO_DEST_NULL: u32 = 3;
pub const PIO_DEST_PINDIRS: u32 = 4;
pub const PIO_DEST_PC: u32 = 5;
pub const PIO_DEST_ISR: u32 = 6;
pub const PIO_DEST_EXEC: u32 = 7;

pub const PIO_MOV_DEST_PINS: u32 = 0;
pub const PIO_MOV_DEST_X: u32 = 1;
pub const PIO_MOV_DEST_Y: u32 = 2;
pub const PIO_MOV_DEST_EXEC: u32 = 4;
pub const PIO_MOV_DEST_PC: u32 = 5;
pub const PIO_MOV_DEST_ISR: u32 = 6;
pub const PIO_MOV_DEST_OSR: u32 = 7;

pub const PIO_OP_NONE: u32 = 0;
pub const PIO_OP_INVERT: u32 = 1;
pub const PIO_OP_BITREV: u32 = 2;

pub const PIO_WAIT_SRC_GPIO: u32 = 0;
pub const PIO_WAIT_SRC_PIN: u32 = 1;
pub const PIO_WAIT_SRC_IRQ: u32 = 2;

pub const PIO_COND_ALWAYS: u32 = 0;
pub const PIO_COND_NOTX: u32 = 1;
pub const PIO_COND_XDEC: u32 = 2;
pub const PIO_COND_NOTY: u32 = 3;
pub const PIO_COND_YDEC: u32 = 4;
pub const PIO_COND_XNEY: u32 = 5;
pub const PIO_COND_PIN: u32 = 6;
pub const PIO_COND_NOTEMPTYOSR: u32 = 7;

pub fn pio_jmp(cond: u32, address: u32, delay: u32) -> u16 {
    (((delay & 0x1f) << 8) | ((cond & 0x7) << 5) | (address & 0x1f)) as u16
}

pub fn pio_wait(polarity: bool, src: u32, index: u32, delay: u32) -> u16 {
    ((1 << 13)
        | ((delay & 0x1f) << 8)
        | ((polarity as u32) << 7)
        | ((src & 0x3) << 5)
        | (index & 0x1f)) as u16
}

pub fn pio_in(src: u32, bit_count: u32, delay: u32) -> u16 {
    ((2 << 13) | ((delay & 0x1f) << 8) | ((src & 0x7) << 5) | (bit_count & 0x1f))
        as u16
}

pub fn pio_out(dest: u32, bit_count: u32, delay: u32) -> u16 {
    ((3 << 13) | ((delay & 0x1f) << 8) | ((dest & 0x7) << 5) | (bit_count & 0x1f))
        as u16
}

pub fn pio_push(if_full: bool, no_block: bool, delay: u32) -> u16 {
    ((4 << 13)
        | ((delay & 0x1f) << 8)
        | ((if_full as u32) << 6)
        | ((no_block as u32) << 5)) as u16
}

pub fn pio_pull(if_empty: bool, no_block: bool, delay: u32) -> u16 {
    ((4 << 13)
        | ((delay & 0x1f) << 8)
        | (1 << 7)
        | ((if_empty as u32) << 6)
        | ((no_block as u32) << 5)) as u16
}

pub fn pio_mov(dest: u32, op: u32, src: u32, delay: u32) -> u16 {
    ((5 << 13)
        | ((delay & 0x1f) << 8)
        | ((dest & 0x7) << 5)
        | ((op & 0x3) << 3)
        | (src & 0x7)) as u16
}

pub fn pio_irq(clear: bool, wait: bool, index: u32, delay: u32) -> u16 {
    ((6 << 13)
        | ((delay & 0x1f) << 8)
        | ((clear as u32) << 6)
        | ((wait as u32) << 5)
        | (index & 0x1f)) as u16
}

pub fn pio_set(dest: u32, data: u32, delay: u32) -> u16 {
    ((7 << 13) | ((delay & 0x1f) << 8) | ((dest & 0x7) << 5) | (data & 0x1f)) as u16
}
```

- [ ] **Step 4: Export the module and run the tests**

Add `pub mod pio_assembler;` to `src/utils/mod.rs`.

Run: `cargo test --lib utils::pio_assembler`

Expected: 9 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/utils/pio_assembler.rs src/utils/mod.rs
git commit -m "feat: port PIO opcode encoders"
```

---

### Task 8: Foundation verification, parity map, and README

**Files:**
- Create: `docs/parity.md`
- Modify: `README.md`
- Modify: `src/lib.rs` if any module declarations are missing

**Interfaces:**
- Consumes: all foundation modules.
- Produces: documented 91-test baseline and a green foundation build.

- [ ] **Step 1: Create the initial parity map**

Create `docs/parity.md`:

```markdown
# rp2040js parity map

Upstream: `wokwi/rp2040js` 1.4.1, commit
`8693315df527adce8bf2bc315aeb4a1ca61c02a3`.
Upstream baseline: `vitest run src` reports 14 files and 322 passing tests.

## Unit-test mapping

| Upstream spec | Rust module | Cases | Status |
|---|---|---:|---|
| `utils/assembler.spec.ts` | `utils/assembler.rs` | 78 | foundation |
| `utils/pio-assembler.spec.ts` | `utils/pio_assembler.rs` | 9 | foundation |
| `utils/fifo.spec.ts` | `utils/fifo.rs` | 3 | foundation |
| `utils/time.spec.ts` | `utils/time.rs` | 1 | foundation |
| `instructions.spec.ts` | `cortex_m0_core.rs` | 126 | plan 2 |
| `rp2040.spec.ts` | `rp2040.rs` | 22 | plan 2 |
| `peripherals/clocks.spec.ts` | `peripherals/clocks.rs` | 16 | plan 3 |
| `peripherals/dma.spec.ts` | `peripherals/dma.rs` | 2 | plan 3 |
| `peripherals/pio.spec.ts` | `peripherals/pio.rs` | 35 | plan 3 |
| `peripherals/pwm.spec.ts` | `peripherals/pwm.rs` | 8 | plan 3 |
| `peripherals/timer.spec.ts` | `peripherals/timer.rs` | 4 | plan 3 |
| `peripherals/uart.spec.ts` | `peripherals/uart.rs` | 2 | plan 3 |
| `sio.spec.ts` | `sio.rs` | 11 | plan 3 |
| `usb/cdc.spec.ts` | `usb/cdc.rs` | 5 | plan 3 |
| **Total** | | **322** | |

## Documented shape differences

- Rust names are idiomatic (`Rp2040`, `read_u32`, `clk_sys()`).
- Callbacks that can re-enter the simulator receive an `Rp2040` handle.
- Clock/GPIO listeners are removed by stable IDs.
- GDB and Simulator are single-threaded.
- Emulator state is `Rc`-based and not `Send`/`Sync`.
- No JavaScript object identity or prototype semantics.
```

- [ ] **Step 2: Add a README foundation example**

Create or update `README.md`:

```markdown
# rp2040rs

Safe Rust port of the [rp2040js](https://github.com/wokwi/rp2040js)
Raspberry Pi Pico emulator.

Upstream pin: 1.4.1, commit
`8693315df527adce8bf2bc315aeb4a1ca61c02a3`.

## Build and test

```bash
cargo test --lib
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
```

The foundation stage ports the upstream FIFO, time, Cortex-M0+ opcode, and PIO
opcode tests: 91 unit tests.

## License

MIT. See `LICENSE` and `NOTICE`.
```

- [ ] **Step 3: Run the exact acceptance commands**

Run: `cargo test --lib`

Expected: exactly 91 tests pass.

Run: `cargo test`

Expected: all unit and integration tests pass.

Run: `cargo clippy --all-targets -- -D warnings`

Expected: no warnings.

Run: `cargo build --release`

Expected: success.

- [ ] **Step 4: Verify no `unsafe` and no placeholder text**

Run: `grep -rn "unsafe" src || true`

Expected: only `#![forbid(unsafe_code)]` appears in `src/lib.rs`.

Run: `grep -rnE "TBD|TODO|FIXME" src docs/parity.md README.md || true`

Expected: no output.

- [ ] **Step 5: Commit**

```bash
git add README.md docs/parity.md
git commit -m "docs: record foundation parity and usage"
```

## Foundation Acceptance

1. `cargo test --lib` reports exactly 91 passing tests:
   `utils::assembler` 78, `utils::pio_assembler` 9, `utils::fifo` 3,
   `utils::time` 1.
2. `cargo test` passes the smoke, utility, and clock integration tests.
3. `cargo clippy --all-targets -- -D warnings` passes.
4. `cargo build --release` passes.
5. The vendored upstream tree is unmodified and documented.
6. Plan 2 can start from this green foundation.
