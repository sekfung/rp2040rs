# rp2040rs Remaining Parity Gaps Implementation Plan

**Goal:** Close the documented peripheral parity gaps that are not covered by the 322 upstream unit tests: TIMER alarm atomic aliases, PWM polarity/DIVMODE/phase-correct behavior, and USB host-mode SOF/host-transaction/SIE mapping.

**Architecture:** Extend existing peripheral state machines without changing the public API or the 322-case lib-test baseline. Add integration regressions only.

**Spec:** `docs/superpowers/specs/2026-11-11-rp2040rs-design.md`

## Global Constraints

- Rust 2021; `#![forbid(unsafe_code)]`; std only; vendored upstream unchanged.
- `cargo test --lib` stays exactly 322; new coverage in `tests/`.
- Single-threaded `Rc`/`RefCell`; atomic aliases before normal writes.
- Keep all existing integration suites green.

### Task 1: TIMER alarm aliases + PWM polarity/DIVMODE/phase-correct

**Files:** `src/peripherals/timer.rs`, `src/peripherals/pwm.rs`, `tests/peripherals_extra.rs`, `tests/pwm_gpio.rs`.

- [ ] TIMER ALARM writes apply `atomic_update` before arming/disarming; add SET/CLR/XOR regression.
- [ ] PWM CSR polarity bits 2/3 invert A/B output; DIVMODE gating controls GPIO output; phase-correct/ZigZag counter uses Timer32 mode; add integration tests for polarity and phase-correct direction/wrap.
- [ ] Keep lib 322; commit `feat: complete PWM and timer parity`.

### Task 2: USB host mode

**Files:** `src/peripherals/usb.rs`, `tests/usb_extra.rs`.

- [ ] Implement host-mode SOF frame/timer, host transaction/SIE_STATUS mapping, device-connected event, and host-mode interrupt bits from `vendor/rp2040js/src/peripherals/usb.ts`.
- [ ] Add integration tests for host enable/connection, SOF increment, and host SIE status/interrupt.
- [ ] Keep lib 322; commit `feat: add USB host mode`.

### Task 3: Docs/final verification

- [ ] Update `docs/parity.md` to remove closed gaps; run 322 lib, full suite, clippy, fmt, release.
- [ ] Commit `docs: close peripheral parity gaps`.
