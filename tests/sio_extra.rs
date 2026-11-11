use rp2040rs::Rp2040;

const SIO_BASE: u32 = 0xd000_0000;
const DIV_UDIVISOR: u32 = SIO_BASE + 0x064;
const DIV_SDIVIDEND: u32 = SIO_BASE + 0x068;
const DIV_SDIVISOR: u32 = SIO_BASE + 0x06c;
const DIV_QUOTIENT: u32 = SIO_BASE + 0x070;
const DIV_REMAINDER: u32 = SIO_BASE + 0x074;

#[test]
fn signed_divider_overflow_wraps_instead_of_panicking() {
    let rp = Rp2040::new();
    rp.write_u32(DIV_SDIVIDEND, 0x8000_0000);
    rp.write_u32(DIV_SDIVISOR, 0xffff_ffff);

    assert_eq!(rp.read_u32(DIV_QUOTIENT), 0x8000_0000);
    assert_eq!(rp.read_u32(DIV_REMAINDER), 0);
}

#[test]
fn sio_divisor_starts_at_one_like_upstream() {
    let rp = Rp2040::new();
    assert_eq!(rp.read_u32(DIV_UDIVISOR), 1);
}
