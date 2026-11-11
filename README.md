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
opcode tests: 91 unit tests. See `docs/parity.md` for the mapping to the
upstream `vitest` suite and the remaining stages.

## License

MIT. See `LICENSE` and `NOTICE`.
