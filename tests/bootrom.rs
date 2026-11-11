use rp2040rs::bootrom::BOOTROM_B1;
use rp2040rs::rp2040::Rp2040;

#[test]
fn bootrom_has_full_size_and_expected_vectors() {
    assert_eq!(BOOTROM_B1.len(), 4096);
    assert_eq!(BOOTROM_B1[0], 0x2004_1f00);
    assert_eq!(BOOTROM_B1[1], 0x0000_00ef);
}

#[test]
fn loading_bootrom_sets_sp_and_pc() {
    let rp = Rp2040::new();
    rp.load_bootrom(&BOOTROM_B1);
    assert_eq!(rp.core.sp(), 0x2004_1f00);
    assert_eq!(rp.core.pc(), 0xee);
}

#[test]
fn loading_bootrom_copies_every_word_into_16_kib_memory() {
    let rp = Rp2040::new();
    rp.load_bootrom(&BOOTROM_B1);
    assert_eq!(rp.bootrom.len(), 16 * 1024);
    let last_offset = (BOOTROM_B1.len() * 4 - 4) as u32;
    assert_eq!(rp.read_u32(last_offset), *BOOTROM_B1.last().unwrap());
}
