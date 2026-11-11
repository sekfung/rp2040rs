# rp2040rs Core Peripherals Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the RP2040 core peripherals and port the remaining 83 upstream unit tests, raising `cargo test --lib` from 239 to exactly 322 and completing the upstream test parity target.

**Architecture:** Peripheral handles own `Rc<RefCell<State>>` plus a weak `BoardShared` reference. `Rp2040` registers each real peripheral at its upstream APB/DPRAM key and exposes the upstream handles (`clocks`, `uart[2]`, `pwm`, `timer`, `dma`, `pio[2]`, `sio`, `usb_ctrl`, etc.). Clock alarms drive DMA/PIO/PWM/timer/USB timing through the shared `Clock`.

**Tech Stack:** Rust 2021, standard library only, `cargo test`, `cargo clippy`, `cargo build --release`.

**Spec:** `docs/superpowers/specs/2026-11-11-rp2040rs-design.md`

## Global Constraints

- Upstream pin: `wokwi/rp2040js` v1.4.1, commit `8693315df527adce8bf2bc315aeb4a1ca61c02a3`.
- Rust 2021; `#![forbid(unsafe_code)]`; standard library only; MIT attribution.
- Vendored `vendor/rp2040js/` is read-only reference.
- `cargo test --lib` must end at exactly 322: 239 core + 16 clocks + 2 DMA + 35 PIO + 8 PWM + 4 timer + 2 UART + 11 SIO + 5 USB CDC.
- Integration tests stay in `tests/`; no extra lib unit tests.
- Single-threaded `Rc`/`Weak`/`RefCell`; never hold a `BoardShared` borrow across a callback.
- Use explicit 32-bit wrapping arithmetic and upstream masks.

## File Structure

```text
src/gpio_pin.rs                 GPIO pin state/callbacks
src/sio.rs                      SIO divider, spinlocks, GPIO access
src/peripherals/clocks.rs       CLOCKS register tree and listeners
src/peripherals/pll.rs          PLL_SYS/PLL_USB
src/peripherals/xosc.rs         crystal oscillator
src/peripherals/dma.rs          DMA channels, chaining, DREQ/IRQ
src/peripherals/timer.rs        TIMER alarms
src/peripherals/pwm.rs          PWM channels and clock
src/peripherals/uart.rs         UART0/1
src/peripherals/usb.rs          USB controller/endpoints/DPRAM
src/peripherals/pio.rs          PIO blocks/state machines
src/usb/cdc.rs                  CDC descriptor parser/enumeration
src/rp2040.rs                   register real peripheral handles
```

---

### Task 1: Clocks/PLL/XOSC and 16 clocks tests

**Files:** create `src/peripherals/{clocks,pll,xosc}.rs`; modify `src/rp2040.rs`, `src/peripherals/mod.rs`.
**Interfaces:** `Rp2040::{clocks,pll_sys,pll_usb,xosc,clk_sys(),clk_peri(),add_clock_listener,remove_clock_listener}`; CLOCKS/PLL register windows; `update_clocks()`.

- [ ] Port `vendor/rp2040js/src/peripherals/clocks.spec.ts` (16 cases) into `src/peripherals/clocks.rs`.
- [ ] Add minimal UART/PWM retune hooks (`Uart::clk_peri_changed`, PWM channel frequency update) needed by the clock tests.
- [ ] Implement PLL integer/fractional dividers, CLK_REF/CLK_SYS/CLK_PERI AUXSRC, divide-by-2^16, last-known-good frequency, listener notification, SysTick/PWM/UART retune.
- [ ] Run `cargo test --lib peripherals::clocks` (16), `cargo test --lib` (255), full suite, clippy.
- [ ] Commit `feat: add clock tree and PLLs`.

### Task 2: SIO and 11 SIO tests

**Files:** create `src/sio.rs`, `src/gpio_pin.rs`; modify `src/rp2040.rs`, `src/lib.rs`.
**Interfaces:** `Sio` divider registers (`UDIVIDEND/UDIVISOR/SDIVIDEND/SDIVISOR/QUOTIENT/REMAINDER/CSR`), spinlocks, GPIO output/input registers.

- [ ] Port `vendor/rp2040js/src/sio.spec.ts` (11 cases) into `src/sio.rs`.
- [ ] Implement signed/unsigned divider, divide-by-zero, quotient/remainder restore, CSR ready/dirty bits, spinlock SET/STATUS/CLR.
- [ ] Implement minimal `GpioPin` state and SIO GPIO registers needed by later PIO tests.
- [ ] Run focused 11, lib 266, full suite, clippy; commit `feat: add SIO divider and spinlocks`.

### Task 3: DMA and 2 DMA tests

**Files:** create `src/peripherals/dma.rs`; modify `src/rp2040.rs`.
**Interfaces:** `Dma`, 12 channels, channel registers/aliases, `INTR`, `DREQ` hooks, alarm-driven transfers.

- [ ] Port `vendor/rp2040js/src/peripherals/dma.spec.ts` (2 cases).
- [ ] Implement control/read/write address increments, transfer count, `CHAIN_TO`, busy/IRQ, TREQ permanent, no dispatch past channel 11.
- [ ] Run focused 2, lib 268, full suite, clippy; commit `feat: add DMA channels`.

### Task 4: TIMER, PWM, UART and 14 tests

**Files:** create `src/peripherals/{timer,pwm,uart}.rs`; modify `src/rp2040.rs`, `src/peripherals/clocks.rs`.
**Interfaces:** `Timer` alarms+INTE/INTF/INTS; `Pwm` 8 channels with PH_ADV/PH_RET and wrap IRQ; `Uart` word length/baud/clk_peri hook.

- [ ] Port `timer.spec.ts` (4), `pwm.spec.ts` (8), `uart.spec.ts` (2).
- [ ] Implement alarm arming/disarming/firing, PWM counter advance/retard/wrap/interrupt timing, UART LCR_H/IBRD/FBRD.
- [ ] Run focused 14, lib 282, full suite, clippy; commit `feat: add timer, PWM, and UART`.

### Task 5: USB controller/CDC and 5 CDC tests

**Files:** create `src/peripherals/usb.rs`, `src/usb/{mod,cdc,interfaces,setup,usb_device}.rs`; modify `src/rp2040.rs`.
**Interfaces:** `UsbController` endpoint/DPRAM/IRQ callbacks; `UsbCdc::extract_endpoint_numbers`.

- [ ] Port `vendor/rp2040js/src/usb/cdc.spec.ts` (5 cases) and implement `extract_endpoint_numbers` exactly.
- [ ] Implement USB controller register/DPRAM/endpoint timing and CDC enumeration/TX FIFO.
- [ ] Run focused 5, lib 287, full suite, clippy; commit `feat: add USB controller and CDC`.

### Task 6: PIO and 35 PIO tests

**Files:** create `src/peripherals/pio.rs`; modify `src/rp2040.rs`, `src/gpio_pin.rs`.
**Interfaces:** `Pio` 2 blocks/4 state machines; instruction memory, FIFOs, shift registers, IRQ/FDEBUG, wrap, autopull/autopush.

- [ ] Port `vendor/rp2040js/src/peripherals/pio.spec.ts` (35 cases).
- [ ] Implement every PIO instruction/mode asserted: SET/MOV/OUT/IN/PUSH/PULL/JMP/WAIT/IRQ, OUT EXEC/PC, status select, FIFO joins, stalls, wrap, TXNFULL/RXNEMPTY, FDEBUG TXSTALL.
- [ ] Run focused 35, lib exactly 322, full suite, clippy; commit `feat: add PIO state machines`.

### Task 7: Peripheral-stage verification and parity update

**Files:** modify `docs/parity.md`, `README.md`.

- [ ] Run `cargo test --lib` (exactly 322), `cargo test`, clippy, fmt, release build.
- [ ] Mark all 14 upstream specs implemented in parity.md; README records 322/322.
- [ ] Commit `docs: record full upstream test parity`.

## Peripheral-Stage Acceptance

1. `cargo test --lib` = exactly 322 tests with the per-file counts above.
2. `cargo test`, clippy, fmt, release build pass; no `unsafe`; vendored tree unchanged.
3. Plan 4 can implement GDB/Simulator/CLI and untested peripherals on top of a fully tested core.
