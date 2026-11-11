use rp2040rs::rp2040::Rp2040;
use rp2040rs::utils::assembler::{opcode_asrs_reg, opcode_ldr_imm, opcode_lsrs_reg, opcode_str};

fn instruction_cycles(opcode: u16, address: u32) -> u32 {
    let rp = Rp2040::new();
    rp.write_u16(0x2000_0000, opcode);
    rp.core.set_pc(0x2000_0000);
    rp.core.set_reg(0, 0x1234_5678);
    rp.core.set_reg(1, address);
    rp.core.execute_instruction()
}

#[test]
fn ldr_cycles_match_upstream_by_region() {
    assert_eq!(instruction_cycles(opcode_ldr_imm(0, 1, 0), 0x2000_1000), 2);
    assert_eq!(instruction_cycles(opcode_ldr_imm(0, 1, 0), 0xd000_0000), 1);
    assert_eq!(instruction_cycles(opcode_ldr_imm(0, 1, 0), 0x4003_4000), 4);
    assert_eq!(instruction_cycles(opcode_ldr_imm(0, 1, 0), 0xe000_ed00), 2);
}

#[test]
fn str_cycles_match_upstream_by_region() {
    assert_eq!(instruction_cycles(opcode_str(0, 1, 0), 0x2000_1000), 2);
    assert_eq!(instruction_cycles(opcode_str(0, 1, 0), 0xd000_0000), 1);
    assert_eq!(instruction_cycles(opcode_str(0, 1, 0), 0x4003_4000), 5);
    assert_eq!(instruction_cycles(opcode_str(0, 1, 0), 0xe000_ed00), 2);
}

#[test]
fn register_shift_by_zero_uses_bit31_for_carry() {
    let rp = Rp2040::new();
    rp.write_u16(0x2000_0000, opcode_asrs_reg(0, 1));
    rp.core.set_pc(0x2000_0000);
    rp.core.set_reg(0, 0x8000_0000);
    rp.core.set_reg(1, 0);
    rp.core.set_c(false);
    rp.core.execute_instruction();
    assert!(rp.core.c());

    rp.write_u16(0x2000_0000, opcode_lsrs_reg(0, 1));
    rp.core.set_pc(0x2000_0000);
    rp.core.set_reg(0, 0x8000_0000);
    rp.core.set_reg(1, 0);
    rp.core.set_c(false);
    rp.core.execute_instruction();
    assert!(rp.core.c());
}
