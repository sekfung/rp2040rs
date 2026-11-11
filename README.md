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

The library now ports the complete upstream unit-test suite: 322 tests
(utilities/opcodes 91, Cortex-M0+ 126, RP2040 bus/PPB 22, core peripherals
83). See `docs/parity.md` for the per-file mapping and documented source-level
deviations.

Plan 4 still adds the GDB server, Simulator, CLI, the upstream peripherals
that have no unit tests (ADC, I2C, SPI, SSI, RTC, Watchdog, RESETS, PSM, PADS,
IO, BUSCTRL, SYSINFO, SYSCFG, TBMAN, Interpolator), host-mode USB, PWM
output/direction propagation to GPIO, and the full UART register set
(`UARTDR`/`FR`/`CR`/`IMSC`/`RIS`/`MIS`/`ICR` plus identity registers).

## License

MIT. See `LICENSE` and `NOTICE`.
