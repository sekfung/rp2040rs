use std::cell::{Cell, RefCell};
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

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
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

    let output = Rc::new(RefCell::new(Vec::<u8>::new()));
    let expected = options.expect_text.clone();
    let found = Rc::new(Cell::new(false));
    let output_for_uart = output.clone();
    let found_for_uart = found.clone();
    let expected_for_uart = expected.clone();
    simulator.rp2040.uart[0].set_on_byte(Box::new(move |byte| {
        let mut stdout = io::stdout().lock();
        let _ = stdout.write_all(&[byte]);
        let _ = stdout.flush();
        let mut collected = output_for_uart.borrow_mut();
        collected.push(byte);
        if let Some(expected) = expected_for_uart.as_deref() {
            if contains_bytes(&collected, expected.as_bytes()) {
                found_for_uart.set(true);
            }
        }
    }));

    if options.gdb {
        if expected.is_some() {
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
        eprintln!("rp2040rs GDB server listening on port {}", options.port);
        let mut gdb_server = GdbServer::new(Box::new(simulator));
        if let Err(error) = server.serve(&mut gdb_server) {
            eprintln!("rp2040rs: GDB server error: {error}");
            process::exit(1);
        }
        return;
    }

    if expected.is_none() {
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
