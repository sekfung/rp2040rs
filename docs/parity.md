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

## Foundation status

`cargo test --lib` reports exactly 91 passing tests:

- `utils::assembler` — 78
- `utils::pio_assembler` — 9
- `utils::fifo` — 3
- `utils::time` — 1

`cargo test` additionally passes the integration tests in `tests/smoke.rs`,
`tests/utils.rs`, and `tests/clock.rs`. Non-upstream coverage lives in those
files so the upstream-ported unit count stays at 322.

## Count verification note

The 322-case baseline comes from a `vitest run src` execution of the pinned
upstream commit, so it is the reference figure for this map.

While writing this map, a static count of the vendored pinned tree was also
performed for the two files defined in plan 2:

- `instructions.spec.ts` declares 126 `it(...)` cases.
- `rp2040.spec.ts` declares 16 `it(...)` entries, two of which are
  `it.each(regions)` over a three-element table, i.e. 20 expanded cases
  statically.

That static reading gives 320 rather than 322. The difference is confined to
`rp2040.spec.ts` and must be re-confirmed with a real `vitest run src` when the
upstream toolchain is available; plan 2 should adopt whichever per-file number
that run reports.

## Documented shape differences

- Rust names are idiomatic (`Rp2040`, `read_u32`, `clk_sys()`).
- Callbacks that can re-enter the simulator receive an `Rp2040` handle.
- Clock/GPIO listeners are removed by stable IDs.
- GDB and Simulator are single-threaded.
- Emulator state is `Rc`-based and not `Send`/`Sync`.
- No JavaScript object identity or prototype semantics.
