use rp2040rs::rp2040::Rp2040;
use rp2040rs::utils::assembler::opcode_lsls_reg;

#[test]
fn lsls_register_shift_above_32_does_not_panic() {
    let rp = Rp2040::new();
    rp.write_u16(0x2000_0000, opcode_lsls_reg(3, 4));
    rp.core.set_pc(0x2000_0000);
    rp.core.set_reg(3, 0x8000_0000);
    rp.core.set_reg(4, 33);

    rp.core.execute_instruction();

    assert_eq!(rp.core.reg(3), 0);
    assert!(rp.core.c());
}

#[test]
fn lsls_register_large_count_uses_masked_carry_bit() {
    let rp = Rp2040::new();
    rp.write_u16(0x2000_0000, opcode_lsls_reg(3, 4));
    rp.core.set_pc(0x2000_0000);
    rp.core.set_reg(3, 0x4000_0002);
    rp.core.set_reg(4, 255);
    rp.core.set_c(true);

    rp.core.execute_instruction();

    assert_eq!(rp.core.reg(3), 0);
    assert!(rp.core.c());
}
