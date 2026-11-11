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

The library ports the complete upstream unit-test suite: 322 tests
(utilities/opcodes 91, Cortex-M0+ 126, RP2040 bus/PPB 22, core peripherals
83). It also includes the remaining untested peripherals, ADC/I2C/SPI/SSI,
RTC/watchdog, GDB server, Simulator, Intel HEX/UF2 loaders, embedded bootrom,
and the `rp2040rs` CLI. See `docs/parity.md` for the per-file mapping and
documented source-level deviations; `tests/exports.rs` is a compile-time audit
of every `src/index.ts` export.

## CLI

```bash
cargo run --release -- --image firmware.hex
cargo run --release -- --image firmware.uf2 --gdb --port 3333
cargo run --release -- --image firmware.hex --expect-text "hello"
```

The CLI loads the embedded B1 bootrom, flashes the image at `0x10000000`, sets
PC, forwards UART0 bytes to stdout, and exposes an optional blocking GDB server.

## Minimal library usage

```rust
use rp2040rs::{Rp2040};

let rp = Rp2040::new();
rp.uart[0].set_on_byte(Box::new(|byte| print!("{}", byte as char)));
rp.write_u32(0x4001_40cc, 5); // IO_BANK0 GPIO25_CTRL: FUNCSEL = SIO
rp.write_u32(0xd000_0024, 1 << 25); // SIO GPIO_OE_SET
rp.write_u32(0xd000_0014, 1 << 25); // SIO GPIO_OUT_SET
```

See `docs/parity.md` for the complete API/export map and documented deviations.

## License

MIT. See `LICENSE` and `NOTICE`.
