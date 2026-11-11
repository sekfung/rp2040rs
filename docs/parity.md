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
| `instructions.spec.ts` | `cortex_m0_core.rs` | 126 | core |
| `rp2040.spec.ts` | `rp2040.rs` | 22 | core |
| `peripherals/clocks.spec.ts` | `peripherals/clocks.rs` | 16 | implemented |
| `peripherals/dma.spec.ts` | `peripherals/dma.rs` | 2 | implemented |
| `peripherals/pio.spec.ts` | `peripherals/pio.rs` | 35 | implemented |
| `peripherals/pwm.spec.ts` | `peripherals/pwm.rs` | 8 | implemented |
| `peripherals/timer.spec.ts` | `peripherals/timer.rs` | 4 | implemented |
| `peripherals/uart.spec.ts` | `peripherals/uart.rs` | 2 | implemented |
| `sio.spec.ts` | `sio.rs` | 11 | implemented |
| `usb/cdc.spec.ts` | `usb/cdc.rs` | 5 | implemented |
| **Total** | | **322** | |

## Foundation-stage checkpoint

At the foundation checkpoint, `cargo test --lib` reported exactly 91 tests:

- `utils::assembler` — 78
- `utils::pio_assembler` — 9
- `utils::fifo` — 3
- `utils::time` — 1

`cargo test` additionally passes the integration tests in `tests/smoke.rs`,
`tests/utils.rs`, and `tests/clock.rs`. Non-upstream coverage lives in those
files so the upstream-ported unit count stays at 322.

## Core-stage checkpoint

At the core checkpoint, `cargo test --lib` reported exactly 239 passing tests:

- foundation utilities and opcode encoders — 91
- `rp2040::tests` — 22
- `cortex_m0_core::tests` — 126

`cargo test` also passes the integration suites in `tests/smoke.rs`,
`tests/utils.rs`, `tests/clock.rs`, `tests/core_state.rs`,
`tests/core_cycles.rs`, and `tests/core_shifts.rs`. APB peripherals other
than PPB/NVIC were still stubs at that checkpoint; the peripheral stage
replaces them.

## Peripheral status

`cargo test --lib` reports exactly 322 passing tests, matching the upstream
vitest baseline:

| Area | Cases |
|---|---:|
| foundation utilities/opcodes | 91 |
| RP2040 bus/PPB | 22 |
| Cortex-M0+ instructions | 126 |
| clocks | 16 |
| DMA | 2 |
| PIO | 35 |
| PWM | 8 |
| TIMER | 4 |
| UART | 2 |
| SIO | 11 |
| USB CDC | 5 |
| **Total** | **322** |

`cargo test` also passes the added integration suites under `tests/` for
clock listeners, core state/cycles/shifts, DMA, PWM/timer regressions, PIO
scheduling, USB controller paths, and CLI-free library smoke coverage.

## Count verification note

The 322-case baseline comes from a `vitest run src` execution of the pinned
upstream commit, so it is the reference figure for this map.

The per-file figures above are expanded vitest counts, not counts of textual
`it(...)` declarations. `instructions.spec.ts` contributes 126 cases, and
`rp2040.spec.ts` contributes 22: 16 plain `it()` cases plus 6 cases from two
`it.each(regions)` tables over three-element lists. Across all 14 spec files,
316 plain cases plus 6 `it.each` expansions give the 322-case total.

## Documented shape differences

- Rust names are idiomatic (`Rp2040`, `read_u32`, `clk_sys()`).
- Callbacks that can re-enter the simulator receive an `Rp2040` handle.
- Clock/GPIO listeners are removed by stable IDs.
- GDB and Simulator are single-threaded.
- Emulator state is `Rc`-based and not `Send`/`Sync`.
- No JavaScript object identity or prototype semantics.
- SBCS overflow flag for `0x80000000 - 0x7fffffff - 1` with carry-in 0:
  the Rust port computes the hardware-correct `V=true`, while upstream
  `substractUpdateFlags` computes `V=false`. No ported test observes this;
  documented as a source-level deviation.
- PIO `FJOIN_RX/TX` semantics and `SM_CLKDIV`/`CLKDIV_RESTART` pacing are not
  implemented; upstream has the same gaps and no ported test asserts them.
- PIO DMA DREQ signaling and the full GPIO pad/function model are deferred to
  Plan 4; PIO-driven input values are propagated.
- USB host-mode SOF/host-transaction alarms, host SIE_STATUS mapping, and host
  device-connected events are deferred to Plan 4; device-mode CDC is covered.
- PWM `CHn_DIV` timing, CTR free-running, TOP/CC double buffering, and wrap
  DREQ are deferred; the ported phase/wrap cases are covered.
- Full UART interrupt/line-buffer registers and callbacks are deferred; word
  length, baud, and `clk_peri` retuning are implemented.
- TIMER ALARM writes use raw values rather than atomic aliases; normal writes
  are implemented.
- SIO initial divisor is 0 rather than upstream's 1, and INTERP0/INTERP1 are
  Plan 4 stubs; the 11 SIO tests cover divider/spinlock behavior.
- TREQ_TIMER3 uses hardware-correct bits 16-31; upstream's 36-bit shift is a
  source-level bug and is not copied.
- `set_logger` called from inside a logger callback is unsupported.
