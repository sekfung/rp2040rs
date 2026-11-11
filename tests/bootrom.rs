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
