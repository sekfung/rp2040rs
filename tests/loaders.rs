use rp2040rs::loaders::{load_hex, load_uf2, LoadError};
use rp2040rs::rp2040::{Memory, Rp2040, FLASH_START_ADDRESS};

fn hex_record(address: u16, record_type: u8, data: &[u8]) -> String {
    let mut bytes = vec![
        data.len() as u8,
        (address >> 8) as u8,
        address as u8,
        record_type,
    ];
    bytes.extend_from_slice(data);
    let checksum = (!bytes.iter().fold(0u8, |sum, b| sum.wrapping_add(*b))).wrapping_add(1);
    bytes.push(checksum);
    let mut line = String::from(":");
    for byte in bytes {
        line.push_str(&format!("{byte:02X}"));
    }
    line
}

#[test]
fn intel_hex_loads_extended_linear_address_into_flash() {
    let rp = Rp2040::new();
    let source = [
        hex_record(0, 0x04, &[0x10, 0x00]),
        hex_record(0, 0x00, &[1, 2, 3, 4]),
        hex_record(0, 0x01, &[]),
    ]
    .join("\n");
    assert_eq!(load_hex(&source, &rp.flash), Ok(4));
    assert_eq!(rp.flash.borrow()[0..4], [1, 2, 3, 4]);
}

#[test]
fn intel_hex_rejects_bad_checksum() {
    let rp = Rp2040::new();
    let mut line = hex_record(0, 0x00, &[1, 2, 3, 4]);
    line.pop();
    line.push('0');
    assert!(matches!(
        load_hex(&line, &rp.flash),
        Err(LoadError::InvalidHex(_))
    ));
}

#[test]
fn uf2_loads_payload_into_flash() {
    let rp = Rp2040::new();
    let mut block = vec![0u8; 512];
    block[0..4].copy_from_slice(&0x0A32_4655u32.to_le_bytes());
    block[4..8].copy_from_slice(&0x9E5D_5157u32.to_le_bytes());
    block[8..12].copy_from_slice(&0x0AB1_6F30u32.to_le_bytes());
    block[12..16].copy_from_slice(&(FLASH_START_ADDRESS + 0x20).to_le_bytes());
    block[16..20].copy_from_slice(&4u32.to_le_bytes());
    block[32..36].copy_from_slice(&[9, 8, 7, 6]);
    assert_eq!(load_uf2(&block, &rp.flash), Ok(4));
    assert_eq!(rp.flash.borrow()[0x20..0x24], [9, 8, 7, 6]);
}

#[test]
fn loaders_reject_out_of_range_flash_addresses() {
    let flash = Memory::new(16);
    assert!(matches!(
        load_uf2(&[0u8; 512], &flash),
        Err(LoadError::AddressOutOfRange(_)) | Err(LoadError::InvalidUf2(_))
    ));
}
