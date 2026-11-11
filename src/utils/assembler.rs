//! Cortex-M0+ opcode encoders, ported from rp2040js `utils/assembler.ts`.

pub fn opcode_adcs(rdn: u32, rm: u32) -> u16 {
    ((0b0100000101 << 6) | ((rm & 7) << 3) | (rdn & 7)) as u16
}

pub fn opcode_adds1(rd: u32, rn: u32, imm3: u32) -> u16 {
    ((0b0001110 << 9) | ((imm3 & 0x7) << 6) | ((rn & 7) << 3) | (rd & 7)) as u16
}

pub fn opcode_adds2(rdn: u32, imm8: u32) -> u16 {
    ((0b00110 << 11) | ((rdn & 7) << 8) | (imm8 & 0xff)) as u16
}

pub fn opcode_add_sp_plus_imm(rd: u32, imm8: u32) -> u16 {
    ((0b10101 << 11) | ((rd & 7) << 8) | ((imm8 >> 2) & 0xff)) as u16
}

pub fn opcode_add_sp2(imm: u32) -> u16 {
    ((0b101100000 << 7) | ((imm >> 2) & 0x7f)) as u16
}

pub fn opcode_adds_reg(rd: u32, rn: u32, rm: u32) -> u16 {
    ((0b0001100 << 9) | ((rm & 0x7) << 6) | ((rn & 7) << 3) | (rd & 7)) as u16
}

pub fn opcode_add_reg(rdn: u32, rm: u32) -> u16 {
    ((0b01000100 << 8) | ((rdn & 0x8) << 4) | ((rm & 0xf) << 3) | (rdn & 0x7)) as u16
}

pub fn opcode_adr(rd: u32, imm8: u32) -> u16 {
    ((0b10100 << 11) | ((rd & 7) << 8) | ((imm8 >> 2) & 0xff)) as u16
}

pub fn opcode_ands(rn: u32, rm: u32) -> u16 {
    ((0b0100000000 << 6) | ((rm & 7) << 3) | (rn & 0x7)) as u16
}

pub fn opcode_asrs(rd: u32, rm: u32, imm5: u32) -> u16 {
    ((0b00010 << 11) | ((imm5 & 0x1f) << 6) | ((rm & 0x7) << 3) | (rd & 0x7)) as u16
}

pub fn opcode_asrs_reg(rdn: u32, rm: u32) -> u16 {
    ((0b0100000100 << 6) | ((rm & 0x7) << 3) | ((rm & 0x7) << 3) | (rdn & 0x7)) as u16
}

pub fn opcode_bt1(cond: u32, imm8: u32) -> u16 {
    ((0b1101 << 12) | ((cond & 0xf) << 8) | ((imm8 >> 1) & 0x1ff)) as u16
}

pub fn opcode_bt2(imm11: u32) -> u16 {
    ((0b11100 << 11) | ((imm11 >> 1) & 0x7ff)) as u16
}

pub fn opcode_bics(rdn: u32, rm: u32) -> u16 {
    ((0b0100001110 << 6) | ((rm & 7) << 3) | (rdn & 7)) as u16
}

pub fn opcode_bl(imm: i32) -> u32 {
    let imm11 = (imm >> 1) & 0x7ff;
    let imm10 = (imm >> 12) & 0x3ff;
    let s = if imm < 0 { 1_u32 } else { 0 };
    let j2 = 1 - ((((imm >> 22) as u32) & 1) ^ s);
    let j1 = 1 - ((((imm >> 23) as u32) & 1) ^ s);
    (0b1101_u32 << 28)
        | (j1 << 29)
        | (j2 << 27)
        | ((imm11 as u32) << 16)
        | (0b11110_u32 << 11)
        | (s << 10)
        | (imm10 as u32)
}

pub fn opcode_blx(rm: u32) -> u16 {
    ((0b010001111 << 7) | (rm << 3)) as u16
}

pub fn opcode_bx(rm: u32) -> u16 {
    ((0b010001110 << 7) | (rm << 3)) as u16
}

pub fn opcode_cmn(rn: u32, rm: u32) -> u16 {
    ((0b0100001011 << 6) | ((rm & 0x7) << 3) | (rn & 0x7)) as u16
}

pub fn opcode_cmp_imm(rn: u32, imm8: u32) -> u16 {
    ((0b00101 << 11) | ((rn & 0x7) << 8) | (imm8 & 0xff)) as u16
}

pub fn opcode_cmp_reg_t1(rn: u32, rm: u32) -> u16 {
    ((0b0100001010 << 6) | ((rm & 0x7) << 3) | (rn & 0x7)) as u16
}

pub fn opcode_cmp_reg_t2(rn: u32, rm: u32) -> u16 {
    ((0b01000101 << 8) | (((rn >> 3) & 0x1) << 7) | ((rm & 0xf) << 3) | (rn & 0x7)) as u16
}

pub fn opcode_dmbsy() -> u32 {
    0x8f50f3bf
}

pub fn opcode_dsbsy() -> u32 {
    0x8f4ff3bf
}

pub fn opcode_eors(rdn: u32, rm: u32) -> u16 {
    ((0b0100000001 << 6) | ((rm & 0x7) << 3) | (rdn & 0x7)) as u16
}

pub fn opcode_isbsy() -> u32 {
    0x8f6ff3bf
}

pub fn opcode_ldmia(rn: u32, registers: u32) -> u16 {
    ((0b11001 << 11) | ((rn & 0x7) << 8) | (registers & 0xff)) as u16
}

pub fn opcode_ldr_reg(rt: u32, rn: u32, rm: u32) -> u16 {
    ((0b0101100 << 9) | ((rm & 0x7) << 6) | ((rn & 0x7) << 3) | (rt & 0x7)) as u16
}

pub fn opcode_ldr_imm(rt: u32, rn: u32, imm5: u32) -> u16 {
    ((0b01101 << 11) | (((imm5 >> 2) & 0x1f) << 6) | ((rn & 0x7) << 3) | (rt & 0x7)) as u16
}

pub fn opcode_ldr_lit(rt: u32, imm8: u32) -> u16 {
    ((0b01001 << 11) | ((imm8 >> 2) & 0xff) | ((rt & 0x7) << 8)) as u16
}

pub fn opcode_ldrb(rt: u32, rn: u32, imm5: u32) -> u16 {
    ((0b01111 << 11) | ((imm5 & 0x1f) << 6) | ((rn & 0x7) << 3) | (rt & 0x7)) as u16
}

pub fn opcode_ldr_sp(rt: u32, imm8: u32) -> u16 {
    ((0b10011 << 11) | ((rt & 7) << 8) | ((imm8 >> 2) & 0xff)) as u16
}

pub fn opcode_ldrb_reg(rt: u32, rn: u32, rm: u32) -> u16 {
    ((0b0101110 << 9) | ((rm & 0x7) << 6) | ((rn & 0x7) << 3) | (rt & 0x7)) as u16
}

pub fn opcode_ldrh(rt: u32, rn: u32, imm5: u32) -> u16 {
    ((0b10001 << 11) | (((imm5 >> 1) & 0xf) << 6) | ((rn & 0x7) << 3) | (rt & 0x7)) as u16
}

pub fn opcode_ldrh_reg(rt: u32, rn: u32, rm: u32) -> u16 {
    ((0b0101101 << 9) | ((rm & 0x7) << 6) | ((rn & 0x7) << 3) | (rt & 0x7)) as u16
}

pub fn opcode_ldrsb(rt: u32, rn: u32, rm: u32) -> u16 {
    ((0b0101011 << 9) | ((rm & 0x7) << 6) | ((rn & 0x7) << 3) | (rt & 0x7)) as u16
}

pub fn opcode_ldrsh(rt: u32, rn: u32, rm: u32) -> u16 {
    ((0b0101111 << 9) | ((rm & 0x7) << 6) | ((rn & 0x7) << 3) | (rt & 0x7)) as u16
}

pub fn opcode_lsls_reg(rdn: u32, rm: u32) -> u16 {
    ((0b0100000010 << 6) | ((rm & 0x7) << 3) | (rdn & 0x7)) as u16
}

pub fn opcode_lsls_imm(rd: u32, rm: u32, imm5: u32) -> u16 {
    (((imm5 & 0x1f) << 6) | ((rm & 0x7) << 3) | (rd & 0x7)) as u16
}

pub fn opcode_lsrs(rd: u32, rm: u32, imm5: u32) -> u16 {
    ((0b00001 << 11) | ((imm5 & 0x1f) << 6) | ((rm & 0x7) << 3) | (rd & 0x7)) as u16
}

pub fn opcode_lsrs_reg(rdn: u32, rm: u32) -> u16 {
    ((0b0100000011 << 6) | ((rm & 0x7) << 3) | (rdn & 0x7)) as u16
}

pub fn opcode_mov(rd: u32, rm: u32) -> u16 {
    ((0b01000110 << 8) | ((rd & 0x8 != 0) as u32) << 7 | (rm << 3) | (rd & 0x7)) as u16
}

pub fn opcode_movs(rd: u32, imm8: u32) -> u16 {
    ((0b00100 << 11) | ((rd & 0x7) << 8) | (imm8 & 0xff)) as u16
}

pub fn opcode_movs_reg(rd: u32, rm: u32) -> u16 {
    (((rm & 0x7) << 3) | (rd & 0x7)) as u16
}

pub fn opcode_mrs(rd: u32, spec_reg: u32) -> u32 {
    (0b1000_u32 << 28) | ((rd & 0xf) << 24) | ((spec_reg & 0xff) << 16) | 0b1111_0011_1110_1111
}

pub fn opcode_msr(spec_reg: u32, rn: u32) -> u32 {
    (0b10001000_u32 << 24) | ((spec_reg & 0xff) << 16) | (0b1111_0011_1000 << 4) | (rn & 0xf)
}

pub fn opcode_muls(rn: u32, rdm: u32) -> u16 {
    ((0b0100001101 << 6) | ((rn & 7) << 3) | (rdm & 7)) as u16
}

pub fn opcode_mvns(rd: u32, rm: u32) -> u16 {
    ((0b0100001111 << 6) | ((rm & 7) << 3) | (rd & 7)) as u16
}

pub fn opcode_nop() -> u16 {
    0b1011_1111_0000_0000
}

pub fn opcode_orrs(rn: u32, rm: u32) -> u16 {
    ((0b0100001100 << 6) | ((rm & 0x7) << 3) | (rn & 0x7)) as u16
}

pub fn opcode_pop(p: bool, register_list: u32) -> u16 {
    ((0b1011110 << 9) | ((p as u32) << 8) | register_list) as u16
}

pub fn opcode_push(m: bool, register_list: u32) -> u16 {
    ((0b1011010 << 9) | ((m as u32) << 8) | register_list) as u16
}

pub fn opcode_rev(rd: u32, rn: u32) -> u16 {
    ((0b1011101000 << 6) | ((rn & 0x7) << 3) | (rd & 0x7)) as u16
}

pub fn opcode_rev16(rd: u32, rn: u32) -> u16 {
    ((0b1011101001 << 6) | ((rn & 0x7) << 3) | (rd & 0x7)) as u16
}

pub fn opcode_revsh(rd: u32, rn: u32) -> u16 {
    ((0b1011101011 << 6) | ((rn & 0x7) << 3) | (rd & 0x7)) as u16
}

pub fn opcode_ror(rdn: u32, rm: u32) -> u16 {
    ((0b0100000111 << 6) | ((rm & 0x7) << 3) | (rdn & 0x7)) as u16
}

pub fn opcode_rsbs(rd: u32, rn: u32) -> u16 {
    ((0b0100001001 << 6) | ((rn & 0x7) << 3) | (rd & 0x7)) as u16
}

pub fn opcode_sbcs(rn: u32, rm: u32) -> u16 {
    ((0b0100000110 << 6) | ((rm & 0x7) << 3) | (rn & 0x7)) as u16
}

pub fn opcode_stmia(rn: u32, registers: u32) -> u16 {
    ((0b11000 << 11) | ((rn & 0x7) << 8) | (registers & 0xff)) as u16
}

pub fn opcode_str(rt: u32, rm: u32, imm5: u32) -> u16 {
    ((0b01100 << 11) | (((imm5 >> 2) & 0x1f) << 6) | ((rm & 0x7) << 3) | (rt & 0x7)) as u16
}

pub fn opcode_str_sp(rt: u32, imm8: u32) -> u16 {
    ((0b10010 << 11) | ((rt & 7) << 8) | ((imm8 >> 2) & 0xff)) as u16
}

pub fn opcode_str_reg(rt: u32, rn: u32, rm: u32) -> u16 {
    ((0b0101000 << 9) | ((rm & 0x7) << 6) | ((rn & 0x7) << 3) | (rt & 0x7)) as u16
}

pub fn opcode_strb(rt: u32, rm: u32, imm5: u32) -> u16 {
    ((0b01110 << 11) | ((imm5 & 0x1f) << 6) | ((rm & 0x7) << 3) | (rt & 0x7)) as u16
}

pub fn opcode_strb_reg(rt: u32, rn: u32, rm: u32) -> u16 {
    ((0b0101010 << 9) | ((rm & 0x7) << 6) | ((rn & 0x7) << 3) | (rt & 0x7)) as u16
}

pub fn opcode_strh(rt: u32, rm: u32, imm5: u32) -> u16 {
    ((0b10000 << 11) | (((imm5 >> 1) & 0x1f) << 6) | ((rm & 0x7) << 3) | (rt & 0x7)) as u16
}

pub fn opcode_strh_reg(rt: u32, rn: u32, rm: u32) -> u16 {
    ((0b0101001 << 9) | ((rm & 0x7) << 6) | ((rn & 0x7) << 3) | (rt & 0x7)) as u16
}

pub fn opcode_subs1(rd: u32, rn: u32, imm3: u32) -> u16 {
    ((0b0001111 << 9) | ((imm3 & 0x7) << 6) | ((rn & 7) << 3) | (rd & 7)) as u16
}

pub fn opcode_subs2(rdn: u32, imm8: u32) -> u16 {
    ((0b00111 << 11) | ((rdn & 7) << 8) | (imm8 & 0xff)) as u16
}

pub fn opcode_subs_reg(rd: u32, rn: u32, rm: u32) -> u16 {
    ((0b0001101 << 9) | ((rm & 0x7) << 6) | ((rn & 7) << 3) | (rd & 7)) as u16
}

pub fn opcode_sub_sp(imm: u32) -> u16 {
    ((0b101100001 << 7) | ((imm >> 2) & 0x7f)) as u16
}

pub fn opcode_svc(imm8: u32) -> u16 {
    ((0b11011111 << 8) | (imm8 & 0xff)) as u16
}

pub fn opcode_sxtb(rd: u32, rm: u32) -> u16 {
    ((0b1011001001 << 6) | ((rm & 7) << 3) | (rd & 7)) as u16
}

pub fn opcode_sxth(rd: u32, rm: u32) -> u16 {
    ((0b1011001000 << 6) | ((rm & 7) << 3) | (rd & 7)) as u16
}

pub fn opcode_tst(rm: u32, rn: u32) -> u16 {
    ((0b0100001000 << 6) | ((rn & 7) << 3) | (rm & 7)) as u16
}

pub fn opcode_uxtb(rd: u32, rm: u32) -> u16 {
    ((0b1011001011 << 6) | ((rm & 7) << 3) | (rd & 7)) as u16
}

pub fn opcode_udf(imm8: u32) -> u16 {
    ((0b11011110 << 8) | (imm8 & 0xff)) as u16
}

pub fn opcode_udf2(imm16: u32) -> u32 {
    let imm12 = imm16 & 0xfff;
    let imm4 = (imm16 >> 12) & 0xf;
    (0b1111_0111_1111_u32 << 4) | imm4 | (0b1010_u32 << 28) | (imm12 << 16)
}

pub fn opcode_uxth(rd: u32, rm: u32) -> u16 {
    ((0b1011001010 << 6) | ((rm & 7) << 3) | (rd & 7)) as u16
}

pub fn opcode_wfi() -> u16 {
    0b1011_1111_0011_0000
}

pub fn opcode_yield() -> u16 {
    0b1011_1111_0001_0000
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! encode {
        ($name:ident, $call:expr, $expected:expr) => {
            #[test]
            fn $name() {
                assert_eq!($call, $expected);
            }
        };
    }

    encode!(adcs_r3_r0, opcode_adcs(3, 0), 0x4143);
    encode!(add_r1_sp_4, opcode_add_sp_plus_imm(1, 4), 0xa901);
    encode!(add_sp_12, opcode_add_sp2(12), 0xb003);
    encode!(adds_r0_r3_0, opcode_adds1(0, 3, 0), 0x1c18);
    encode!(adds_r1_r1_r3, opcode_adds_reg(1, 1, 3), 0x18c9);
    encode!(add_r1_ip, opcode_add_reg(1, 12), 0x4461);
    encode!(adds_r1_1, opcode_adds2(1, 1), 0x3101);
    encode!(ands_r5_r0, opcode_ands(5, 0), 0x4005);
    encode!(adr_r4_52, opcode_adr(4, 52), 0xa40d);
    encode!(asrs_r3_r2_31, opcode_asrs(3, 2, 31), 0x17d3);
    encode!(asrs_r3_r4, opcode_asrs_reg(3, 4), 0x4123);
    encode!(b_n_20_t1, opcode_bt1(1, 0x1f8), 0xd1fc);
    encode!(b_n_20_t2, opcode_bt2(0xfec), 0xe7f6);
    encode!(bics_r0_r3, opcode_bics(0, 3), 0x4398);
    encode!(bl_minus_198, opcode_bl(-198), 0xff9df7ff);
    encode!(bl_plus_10, opcode_bl(10), 0xf805f000);
    encode!(bl_minus_3242, opcode_bl(-3242), 0xf9abf7ff);
    encode!(blx_r1, opcode_blx(1), 0x4788);
    encode!(bx_lr, opcode_bx(14), 0x4770);
    encode!(cmn_r5_r6, opcode_cmn(5, 6), 0x42f5);
    encode!(cmp_r5_66, opcode_cmp_imm(5, 66), 0x2d42);
    encode!(cmp_r5_r0, opcode_cmp_reg_t1(5, 0), 0x4285);
    encode!(cmp_ip_r6, opcode_cmp_reg_t2(12, 6), 0x45b4);
    encode!(eors_r1_r3, opcode_eors(1, 3), 0x4059);
    encode!(dmbsy, opcode_dmbsy(), 0x8f50f3bf);
    encode!(dsbsy, opcode_dsbsy(), 0x8f4ff3bf);
    encode!(ldmia_r0_r1_r2, opcode_ldmia(0, (1 << 1) | (1 << 2)), 0xc806);
    encode!(isbsy, opcode_isbsy(), 0x8f6ff3bf);
    encode!(lsls_reg_r5_r0, opcode_lsls_reg(5, 0), 0x4085);
    encode!(lsrs_r1_r1_1, opcode_lsrs(1, 1, 1), 0x0849);
    encode!(lsrs_reg_r0_r4, opcode_lsrs_reg(0, 4), 0x40e0);
    encode!(ldr_imm_r3_r2_24, opcode_ldr_imm(3, 2, 24), 0x6993);
    encode!(ldr_lit_r0_148, opcode_ldr_lit(0, 148), 0x4825);
    encode!(ldr_reg_r3_r3_r4, opcode_ldr_reg(3, 3, 4), 0x591b);
    encode!(ldr_sp_r3_12, opcode_ldr_sp(3, 12), 0x9b03);
    encode!(ldrb_r0_r1_0, opcode_ldrb(0, 1, 0), 0x7808);
    encode!(ldrb_reg_r2_r5_r4, opcode_ldrb_reg(2, 5, 4), 0x5d2a);
    encode!(ldrh_r3_r0_2, opcode_ldrh(3, 0, 2), 0x8843);
    encode!(ldrh_reg_r4_r0_r1, opcode_ldrh_reg(4, 0, 1), 0x5a44);
    encode!(ldrsb_r3_r2_r3, opcode_ldrsb(3, 2, 3), 0x56d3);
    encode!(ldrsh_r5_r3_r5, opcode_ldrsh(5, 3, 5), 0x5f5d);
    encode!(lsls_imm_r5_r5_18, opcode_lsls_imm(5, 5, 18), 0x04ad);
    encode!(mov_r3_r8, opcode_mov(3, 8), 0x4643);
    encode!(movs_r5_128, opcode_movs(5, 128), 0x2580);
    encode!(movs_reg_r6_r5, opcode_movs_reg(6, 5), 0x002e);
    encode!(mrs_r6_primask, opcode_mrs(6, 16), 0x8610f3ef);
    encode!(msr_primask_r6, opcode_msr(16, 6), 0x8810f386);
    encode!(muls_r2_r0, opcode_muls(2, 0), 0x4350);
    encode!(mvns_r3_r3, opcode_mvns(3, 3), 0x43db);
    encode!(orrs_r3_r0, opcode_orrs(3, 0), 0x4303);
    encode!(pop_pc_r0_r1, opcode_pop(true, (1 << 0) | (1 << 1)), 0xbd03);
    encode!(
        push_lr_r4_r5_r6,
        opcode_push(true, (1 << 4) | (1 << 5) | (1 << 6)),
        0xb570
    );
    encode!(rev_r3_r1, opcode_rev(3, 1), 0xba0b);
    encode!(rev16_r2_r7, opcode_rev16(2, 7), 0xba7a);
    encode!(revsh_r1_r5, opcode_revsh(1, 5), 0xbae9);
    encode!(ror_r6_r0, opcode_ror(6, 0), 0x41c6);
    encode!(rsbs_r0_r3, opcode_rsbs(0, 3), 0x4258);
    encode!(sbcs_r0_r3, opcode_sbcs(0, 3), 0x4198);
    encode!(stmia_r2_r0, opcode_stmia(2, 1 << 0), 0xc201);
    encode!(str_r6_r4_20, opcode_str(6, 4, 20), 0x6166);
    encode!(str_sp_r1_4, opcode_str_sp(1, 4), 0x9101);
    encode!(str_reg_r2_r1_r4, opcode_str_reg(2, 1, 4), 0x510a);
    encode!(strb_r3_r2_0, opcode_strb(3, 2, 0), 0x7013);
    encode!(strb_reg_r3_r2_r5, opcode_strb_reg(3, 2, 5), 0x5553);
    encode!(strh_r1_r3_4, opcode_strh(1, 3, 4), 0x8099);
    encode!(strh_reg_r1_r3_r2, opcode_strh_reg(1, 3, 2), 0x5299);
    encode!(sub_sp_12, opcode_sub_sp(12), 0xb083);
    encode!(subs_r3_r0_1, opcode_subs1(3, 0, 1), 0x1e43);
    encode!(subs_reg_r1_r1_r0, opcode_subs_reg(1, 1, 0), 0x1a09);
    encode!(subs_r3_13, opcode_subs2(3, 13), 0x3b0d);
    encode!(svc_0, opcode_svc(0), 0xdf00);
    encode!(sxtb_r2_r2, opcode_sxtb(2, 2), 0xb252);
    encode!(sxth_r6_r3, opcode_sxth(6, 3), 0xb21e);
    encode!(tst_r1_r3, opcode_tst(1, 3), 0x4219);
    encode!(udf_1, opcode_udf(1), 0xde01);
    encode!(udf2_0, opcode_udf2(0), 0xa000f7f0);
    encode!(uxtb_r3_r3, opcode_uxtb(3, 3), 0xb2db);
    encode!(uxth_r3_r0, opcode_uxth(3, 0), 0xb283);
}
