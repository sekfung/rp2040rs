# rp2040js parity map

Upstream: `wokwi/rp2040js` 1.4.1, commit
`8693315df527adce8bf2bc315aeb4a1ca61c02a3`.
Upstream baseline: `vitest run src` reports 14 files and 322 passing tests.

## Unit-test mapping

| Upstream spec | Rust module | Cases | Status |
|---|---|---:|---|
| `utils/assembler.spec.ts` | `utils/assembler.rs` | 78 | implemented (foundation) |
| `utils/pio-assembler.spec.ts` | `utils/pio_assembler.rs` | 9 | implemented (foundation) |
| `utils/fifo.spec.ts` | `utils/fifo.rs` | 3 | implemented (foundation) |
| `utils/time.spec.ts` | `utils/time.rs` | 1 | implemented (foundation) |
| `instructions.spec.ts` | `cortex_m0_core.rs` | 126 | implemented (core) |
| `rp2040.spec.ts` | `rp2040.rs` | 22 | implemented (core) |
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

`cargo test` also passes the integration suites under `tests/` for clock
listeners, core state/cycles/shifts, DMA, PWM/timer/GPIO, PIO scheduling and
DREQ, USB controller/CDC, GDB framing/registers/stop replies, Simulator,
bootrom/loaders, CLI, and the compile-time `src/index.ts` export audit.

## Host tools and full feature status

Implemented beyond the upstream unit-test surface:

- GDB server (`GdbConnection`, `GdbServer`, `GdbTcpServer`) with RSP framing,
  register/memory access, single-step, continue/stop replies, and target XML.
- `Simulator` batching/stop, embedded B1 bootrom (16 KiB), Intel HEX and UF2
  loaders, and the `rp2040rs` CLI.
- ADC, I2C0/1, SPI0/1, SSI, RTC, watchdog, RESETS, PSM, PADS, IO_BANK0,
  BUSCTRL, SYSINFO, SYSCFG, TBMAN, and INTERP0/1.
- Full UART registers/FIFO/interrupts/line buffering, PWM free-running counter,
  `CHn_DIV`, double-buffered TOP/CC, wrap DREQ, and GPIO A/B output; per-source
  GPIO ownership resolved through IO_BANK0 FUNCSEL.
- PIO clock alarms, TX/RX DREQ, FJOIN_RX read path, SM reset, and side-set.

The crate root re-exports every `src/index.ts` symbol; `tests/exports.rs`
enforces this at compile time.

## Public export map

| `src/index.ts` export | Rust crate-root name |
|---|---|
| `GDBConnection`, `GDBServer` | `GdbConnection`, `GdbServer` |
| `GPIOPin`, `GPIOPinState` | `GpioPin`, `GpioPinState` |
| `I2CMode`, `I2CSpeed`, `RPI2C` | `I2cMode`, `I2cSpeed`, `I2c` |
| `BasePeripheral`, `Peripheral` | `BasePeripheral`, `Peripheral` |
| `RPPIO`, `StateMachine` | `Pio`, `StateMachine` |
| `RPUSBController` | `UsbController` |
| `RP2040`, `ClockListener` | `Rp2040`, `ClockListener` |
| `Simulator` | `Simulator` |
| `USBCDC` | `UsbCdc` |
| USB interface/setup symbols | `DataDirection`, `DescriptorType`, `SetupRecipient`, `SetupRequest`, `SetupType`, `ISetupPacketParams`, `createSetupPacket`, `getDescriptorPacket`, `setDeviceAddressPacket`, `setDeviceConfigurationPacket` |
| USB device symbols | `StandardRequest`, `UsbDescriptorType`, `parseSetupPacket`, `UsbDevice`, `UsbTransferResult`, `UsbTransferStatus` |
| `FIFO`, `ConsoleLogger`, `LogLevel`, `Logger` | `Fifo`, `ConsoleLogger`, `LogLevel`, `Logger` |

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
- GPIO output is tracked per source (PWM/SIO/PIO0/PIO1) and resolved through
  IO_BANK0 FUNCSEL. PIO `FJOIN_TX`, the full eight-entry shared FIFO, and
  per-SM `SM_CLKDIV`/`CLKDIV_RESTART` pacing remain simplified; upstream has
  the same gaps and no ported test asserts them.
- USB host mode implements SOF generation, host transaction ACK/timeout,
  host SIE_STATUS/interrupt mapping and device-connected events. Full USBDevice
  packet-level host transaction modeling and all 12 device SIE mappings remain
  simplified; device-mode CDC/controller paths are implemented.
- PWM polarity inversion, `DIVMODE` gating, phase-correct/ZigZag counting, and
  compare-output edges are implemented; free-running counter, `CHn_DIV`, double
  buffering, wrap DREQ, and GPIO A/B output are implemented.
- UART modem/flow-control, FIFO thresholds, and overrun/error flags remain
  simplified; the full data/status/control/interrupt/identity register set,
  RX FIFO, line buffering, and TX/RX callbacks are implemented.
- TIMER ALARM writes use raw values rather than atomic aliases; normal writes
  are implemented.
- SIO initial divisor matches upstream's 1; INTERP0/INTERP1 are implemented.
  SIO signed division uses hardware wrapping semantics (`i32::MIN / -1`
  wraps, remainder 0), while upstream JavaScript returns the non-wrapped
  numeric result for that overflow pair.
- TREQ_TIMER3 uses hardware-correct bits 16-31; upstream's 36-bit shift is a
  source-level bug and is not copied.
- `set_logger` called from inside a logger callback is unsupported.
- RTC date/time reads use UTC from the simulated wall clock; upstream uses the
  host-local `Date` getters. This affects displayed RTC values only.
- GDB `continue` is synchronous: it executes one bounded 1,000,000-instruction
  batch and returns `OK` when no breakpoint fires, or `S05` when a BKPT stops
  the target. Upstream returns `OK` immediately and continues asynchronously.
