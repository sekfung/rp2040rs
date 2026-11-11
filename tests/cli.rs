use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT_FILE: AtomicU32 = AtomicU32::new(0);

fn temp_path(extension: &str) -> PathBuf {
    let id = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "rp2040rs-cli-{}-{}.{}",
        std::process::id(),
        id,
        extension
    ))
}

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rp2040rs"))
}

/// Thumb program: load UART0 base, write 'H' to UARTDR, then loop forever.
const PROGRAM: [u8; 12] = [
    0x01, 0x48, // ldr r0, [pc, #4] -> literal at offset 8
    0x48, 0x21, // movs r1, #0x48 ('H')
    0x01, 0x70, // strb r1, [r0]
    0xfe, 0xe7, // b .
    0x00, 0x40, 0x03, 0x40, // 0x40034000, UART0 base
];

fn make_hex() -> String {
    // Extended linear address 0x1000 -> 0x10000000, then the 12-byte payload.
    let data = [0x02u8, 0x00, 0x00, 0x04, 0x10, 0x00];
    let sum = data.iter().fold(0u8, |acc, byte| acc.wrapping_add(*byte));
    let upper = format!(":020000041000{:02X}", (!sum).wrapping_add(1));

    let mut record = vec![PROGRAM.len() as u8, 0x00, 0x00, 0x00];
    record.extend_from_slice(&PROGRAM);
    let checksum = (!record.iter().fold(0u8, |acc, byte| acc.wrapping_add(*byte))).wrapping_add(1);
    let payload = format!(
        ":{}{:02X}",
        record
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<String>(),
        checksum
    );
    format!("{upper}\n{payload}\n:00000001FF\n")
}

fn write_le32(buffer: &mut [u8], offset: usize, value: u32) {
    buffer[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn make_uf2() -> Vec<u8> {
    let mut block = vec![0u8; 512];
    write_le32(&mut block, 0, 0x0A32_4655);
    write_le32(&mut block, 4, 0x9E5D_5157);
    write_le32(&mut block, 8, 0x0AB1_6F30);
    write_le32(&mut block, 12, 0x1000_0000);
    write_le32(&mut block, 16, PROGRAM.len() as u32);
    write_le32(&mut block, 20, 0);
    write_le32(&mut block, 24, 1);
    block[32..32 + PROGRAM.len()].copy_from_slice(&PROGRAM);
    block
}

#[test]
fn cli_loads_hex_and_expect_text_succeeds() {
    let path = temp_path("hex");
    let source = make_hex();
    fs::write(&path, source).unwrap();
    let output = Command::new(binary())
        .args(["--image", path.to_str().unwrap(), "--expect-text", "H"])
        .output()
        .unwrap();
    let _ = fs::remove_file(&path);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.contains(&b'H'));
}

#[test]
fn cli_loads_uf2_and_expect_text_succeeds() {
    let path = temp_path("uf2");
    fs::write(&path, make_uf2()).unwrap();
    let output = Command::new(binary())
        .args(["--image", path.to_str().unwrap(), "--expect-text", "H"])
        .output()
        .unwrap();
    let _ = fs::remove_file(&path);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.contains(&b'H'));
}

#[test]
fn cli_expect_text_miss_exits_nonzero() {
    let path = temp_path("hex");
    fs::write(&path, make_hex()).unwrap();
    let output = Command::new(binary())
        .args([
            "--image",
            path.to_str().unwrap(),
            "--expect-text",
            "missing",
        ])
        .output()
        .unwrap();
    let _ = fs::remove_file(&path);
    assert!(!output.status.success());
}

#[test]
fn cli_rejects_unknown_image_type() {
    let path = temp_path("bin");
    fs::write(&path, [0u8; 4]).unwrap();
    let output = Command::new(binary())
        .args(["--image", path.to_str().unwrap(), "--expect-text", "H"])
        .output()
        .unwrap();
    let _ = fs::remove_file(&path);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported image type"));
}
