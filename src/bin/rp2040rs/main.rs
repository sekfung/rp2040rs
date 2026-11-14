use std::cell::Cell;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process;
use std::rc::Rc;

use rp2040rs::{load_hex, load_uf2, GdbServer, GdbTcpServer, Rp2040, Simulator, BOOTROM_B1};

const FLASH_START: u32 = 0x1000_0000;
const EXECUTION_CHUNK: usize = 100_000;
const MAX_CHUNKS_WITH_EXPECT: usize = 50;

struct Options {
    image: PathBuf,
    gdb: bool,
    port: u16,
    expect_text: Option<String>,
}

fn usage() -> ! {
    eprintln!("usage: rp2040rs --image <hex|uf2> [--gdb] [--port <port>] [--expect-text <text>]");
    process::exit(2);
}

fn parse_args() -> Options {
    let mut args = env::args().skip(1);
    let mut image = None;
    let mut gdb = false;
    let mut port = 3333u16;
    let mut expect_text = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--image" => {
                image = Some(PathBuf::from(args.next().unwrap_or_else(|| usage())));
            }
            "--gdb" => gdb = true,
            "--port" => {
                let value = args.next().unwrap_or_else(|| usage());
                port = value.parse().unwrap_or_else(|_| usage());
            }
            "--expect-text" => {
                expect_text = Some(args.next().unwrap_or_else(|| usage()));
            }
            "-h" | "--help" => {
                eprintln!(
                    "usage: rp2040rs --image <hex|uf2> [--gdb] [--port <port>] [--expect-text <text>]"
                );
                process::exit(0);
            }
            other => {
                eprintln!("unknown argument: {other}");
                usage();
            }
        }
    }

    let image = image.unwrap_or_else(|| usage());
    Options {
        image,
        gdb,
        port,
        expect_text,
    }
}

fn load_image(rp2040: &Rp2040, path: &PathBuf) -> Result<usize, String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "hex" => {
            let source = fs::read_to_string(path)
                .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
            load_hex(&source, &rp2040.flash).map_err(|error| error.to_string())
        }
        "uf2" => {
            let source = fs::read(path)
                .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
            load_uf2(&source, &rp2040.flash).map_err(|error| error.to_string())
        }
        other => Err(format!(
            "unsupported image type '{other}' (expected .hex or .uf2)"
        )),
    }
}

struct ExpectedText {
    bytes: Vec<u8>,
    prefix_lengths: Vec<usize>,
    matched: usize,
}

impl ExpectedText {
    fn new(text: String) -> Self {
        let mut pattern = Self {
            prefix_lengths: vec![0; text.len()],
            bytes: text.into_bytes(),
            matched: 0,
        };
        for index in 1..pattern.bytes.len() {
            let prefix = pattern.prefix_lengths[index - 1];
            pattern.prefix_lengths[index] = pattern.advance(prefix, pattern.bytes[index]);
        }
        pattern
    }

    // Keep the longest prefix that is also a suffix of the bytes seen so far.
    fn advance(&self, mut prefix: usize, byte: u8) -> usize {
        while prefix > 0 && self.bytes[prefix] != byte {
            prefix = self.prefix_lengths[prefix - 1];
        }
        prefix + usize::from(self.bytes[prefix] == byte)
    }

    fn push(&mut self, byte: u8) -> bool {
        if self.bytes.is_empty() {
            return false;
        }
        if self.matched < self.bytes.len() {
            self.matched = self.advance(self.matched, byte);
        }
        self.matched == self.bytes.len()
    }
}

fn main() {
    let options = parse_args();
    let mut simulator = Simulator::new();

    simulator.rp2040.load_bootrom(&BOOTROM_B1);
    if let Err(error) = load_image(&simulator.rp2040, &options.image) {
        eprintln!("rp2040rs: {error}");
        process::exit(1);
    }
    simulator.rp2040.core.set_pc(FLASH_START);

    let expects_text = options.expect_text.is_some();
    let mut expected = options.expect_text.map(ExpectedText::new);
    let found = Rc::new(Cell::new(false));
    let found_for_uart = found.clone();
    simulator.rp2040.uart[0].set_on_byte(Box::new(move |byte| {
        let mut stdout = io::stdout().lock();
        let _ = stdout.write_all(&[byte]);
        let _ = stdout.flush();
        if !found_for_uart.get() && expected.as_mut().is_some_and(|pattern| pattern.push(byte)) {
            found_for_uart.set(true);
        }
    }));

    if options.gdb {
        if expects_text {
            eprintln!("rp2040rs: --expect-text cannot be combined with --gdb");
            process::exit(2);
        }
        let server = match GdbTcpServer::bind(options.port) {
            Ok(server) => server,
            Err(error) => {
                eprintln!("rp2040rs: cannot bind GDB port {}: {error}", options.port);
                process::exit(1);
            }
        };
        let bound_port = server
            .local_addr()
            .map(|address| address.port())
            .unwrap_or(options.port);
        eprintln!("rp2040rs GDB server listening on port {bound_port}");
        let mut gdb_server = GdbServer::new(Box::new(simulator));
        if let Err(error) = server.serve(&mut gdb_server) {
            eprintln!("rp2040rs: GDB server error: {error}");
            process::exit(1);
        }
        return;
    }

    if !expects_text {
        loop {
            simulator.execute_once();
        }
    }

    for _ in 0..MAX_CHUNKS_WITH_EXPECT {
        simulator.execute_steps(EXECUTION_CHUNK);
        if found.get() {
            process::exit(0);
        }
    }
    eprintln!("rp2040rs: expected text was not observed");
    process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::ExpectedText;

    #[test]
    fn uart_match_keeps_overlapping_prefixes_across_bytes() {
        let mut pattern = ExpectedText::new("ababac".into());
        for byte in b"abababa" {
            assert!(!pattern.push(*byte));
        }
        assert!(pattern.push(b'c'));
        assert!(pattern.push(b'x'));
    }

    #[test]
    fn empty_expected_text_never_matches() {
        let mut pattern = ExpectedText::new(String::new());
        assert!(!pattern.push(b'a'));
    }
}
