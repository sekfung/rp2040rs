use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::rp2040::{BoardShared, Bus};

pub const SYSM_APSR: u32 = 0;
pub const SYSM_XPSR: u32 = 3;
pub const SYSM_IPSR: u32 = 5;
pub const SYSM_MSP: u32 = 8;
pub const SYSM_PSP: u32 = 9;
pub const SYSM_PRIMASK: u32 = 16;
pub const SYSM_CONTROL: u32 = 20;

const EXC_SVCALL: u32 = 11;
const EXC_PENDSV: u32 = 14;
const EXC_SYSTICK: u32 = 15;
const LOWEST_PRIORITY: u32 = 4;
const XPSR_THUMB: u32 = 1 << 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionMode {
    Thread,
    Handler,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackPointerBank {
    Main,
    Process,
}

pub struct CoreState {
    registers: [u32; 16],
    banked_sp: u32,
    cycles: u64,
    pub(crate) n: bool,
    pub(crate) z: bool,
    pub(crate) c: bool,
    pub(crate) v: bool,
    pub(crate) break_rewind: u32,
    pub(crate) pm: bool,
    pub(crate) spsel: StackPointerBank,
    pub(crate) npriv: bool,
    pub(crate) current_mode: ExecutionMode,
    pub(crate) ipsr: u32,
    pub(crate) pending_interrupts: u32,
    pub(crate) enabled_interrupts: u32,
    pub(crate) interrupt_priorities: [u32; 4],
    pub(crate) interrupts_updated: bool,
    pub(crate) pending_nmi: bool,
    pub(crate) pending_pendsv: bool,
    pub(crate) pending_svc: bool,
    pub(crate) pending_systick: bool,
    pub(crate) vtor: u32,
    pub(crate) shpr2: u32,
    pub(crate) shpr3: u32,
    pub(crate) waiting: bool,
}

impl CoreState {
    fn new() -> Self {
        Self {
            registers: [0; 16],
            banked_sp: 0xffff_fffc,
            cycles: 0,
            n: false,
            z: false,
            c: false,
            v: false,
            break_rewind: 0,
            pm: false,
            spsel: StackPointerBank::Main,
            npriv: false,
            current_mode: ExecutionMode::Thread,
            ipsr: 0,
            pending_interrupts: 0,
            enabled_interrupts: 0,
            interrupt_priorities: [0xffff_ffff, 0, 0, 0],
            interrupts_updated: false,
            pending_nmi: false,
            pending_pendsv: false,
            pending_svc: false,
            pending_systick: false,
            vtor: 0,
            shpr2: 0,
            shpr3: 0,
            waiting: false,
        }
    }
}

#[derive(Clone)]
pub struct Core {
    state: Rc<RefCell<CoreState>>,
    board: Weak<RefCell<BoardShared>>,
}

impl Core {
    pub(crate) fn new(board: Weak<RefCell<BoardShared>>) -> Self {
        let mut state = CoreState::new();
        state.registers[13] = state.banked_sp;
        Self {
            state: Rc::new(RefCell::new(state)),
            board,
        }
    }

    fn bus(&self) -> Option<Bus> {
        self.board.upgrade().map(Bus)
    }

    pub fn reset(&self) {
        let Some(bus) = self.bus() else {
            return;
        };
        let vtor = self.state.borrow().vtor;
        let sp = bus.read_u32(vtor) & !0x3;
        let pc = bus.read_u32(vtor.wrapping_add(4)) & 0xffff_fffe;
        let mut state = self.state.borrow_mut();
        state.registers[13] = sp;
        state.registers[15] = pc;
        state.cycles = 0;
    }

    pub fn pc(&self) -> u32 {
        self.state.borrow().registers[15]
    }

    pub fn set_pc(&self, value: u32) {
        self.state.borrow_mut().registers[15] = value;
    }

    pub fn sp(&self) -> u32 {
        self.state.borrow().registers[13]
    }

    pub fn set_sp(&self, value: u32) {
        self.state.borrow_mut().registers[13] = value & !0x3;
    }

    pub fn msp(&self) -> u32 {
        let state = self.state.borrow();
        match state.spsel {
            StackPointerBank::Main => state.registers[13],
            StackPointerBank::Process => state.banked_sp,
        }
    }

    pub fn psp(&self) -> u32 {
        let state = self.state.borrow();
        match state.spsel {
            StackPointerBank::Process => state.registers[13],
            StackPointerBank::Main => state.banked_sp,
        }
    }

    pub fn set_msp(&self, value: u32) {
        let mut state = self.state.borrow_mut();
        match state.spsel {
            StackPointerBank::Main => state.registers[13] = value & !0x3,
            StackPointerBank::Process => state.banked_sp = value & !0x3,
        }
    }

    pub fn set_psp(&self, value: u32) {
        let mut state = self.state.borrow_mut();
        match state.spsel {
            StackPointerBank::Process => state.registers[13] = value & !0x3,
            StackPointerBank::Main => state.banked_sp = value & !0x3,
        }
    }

    fn switch_stack(&self, stack: StackPointerBank) {
        let mut state = self.state.borrow_mut();
        if state.spsel != stack {
            let temp = state.registers[13];
            state.registers[13] = state.banked_sp;
            state.banked_sp = temp;
            state.spsel = stack;
        }
    }

    pub fn lr(&self) -> u32 {
        self.state.borrow().registers[14]
    }

    pub fn set_lr(&self, value: u32) {
        self.state.borrow_mut().registers[14] = value;
    }

    pub fn reg(&self, index: usize) -> u32 {
        self.state.borrow().registers[index]
    }

    pub fn set_reg(&self, index: usize, value: u32) {
        self.state.borrow_mut().registers[index] = value;
    }

    pub fn set_registers(&self, values: &[u32; 16]) {
        self.state.borrow_mut().registers = *values;
    }

    pub fn registers(&self) -> [u32; 16] {
        self.state.borrow().registers
    }

    pub fn cycles(&self) -> u64 {
        self.state.borrow().cycles
    }

    pub fn set_cycles(&self, value: u64) {
        self.state.borrow_mut().cycles = value;
    }

    pub fn apsr(&self) -> u32 {
        let state = self.state.borrow();
        (u32::from(state.n) << 31)
            | (u32::from(state.z) << 30)
            | (u32::from(state.c) << 29)
            | (u32::from(state.v) << 28)
    }

    pub fn set_apsr(&self, value: u32) {
        let mut state = self.state.borrow_mut();
        state.n = value & 0x8000_0000 != 0;
        state.z = value & 0x4000_0000 != 0;
        state.c = value & 0x2000_0000 != 0;
        state.v = value & 0x1000_0000 != 0;
    }

    pub fn xpsr(&self) -> u32 {
        let state = self.state.borrow();
        self.apsr() | (state.ipsr & 0x3f) | XPSR_THUMB
    }

    pub fn set_xpsr(&self, value: u32) {
        self.set_apsr(value);
        self.state.borrow_mut().ipsr = value & 0x3f;
    }

    pub fn ipsr(&self) -> u32 {
        self.state.borrow().ipsr
    }

    pub fn set_ipsr(&self, value: u32) {
        self.state.borrow_mut().ipsr = value & 0x3f;
    }

    pub fn n(&self) -> bool {
        self.state.borrow().n
    }
    pub fn z(&self) -> bool {
        self.state.borrow().z
    }
    pub fn c(&self) -> bool {
        self.state.borrow().c
    }
    pub fn v(&self) -> bool {
        self.state.borrow().v
    }

    pub fn set_n(&self, value: bool) {
        self.state.borrow_mut().n = value;
    }
    pub fn set_z(&self, value: bool) {
        self.state.borrow_mut().z = value;
    }
    pub fn set_c(&self, value: bool) {
        self.state.borrow_mut().c = value;
    }
    pub fn set_v(&self, value: bool) {
        self.state.borrow_mut().v = value;
    }

    pub fn break_rewind(&self) -> u32 {
        self.state.borrow().break_rewind
    }

    pub fn pending_interrupts(&self) -> u32 {
        self.state.borrow().pending_interrupts
    }

    pub fn set_pending_interrupts(&self, value: u32) {
        self.state.borrow_mut().pending_interrupts = value;
    }

    pub fn enabled_interrupts(&self) -> u32 {
        self.state.borrow().enabled_interrupts
    }

    pub fn set_enabled_interrupts(&self, value: u32) {
        self.state.borrow_mut().enabled_interrupts = value;
    }

    pub fn pending_nmi(&self) -> bool {
        self.state.borrow().pending_nmi
    }

    pub fn set_pending_nmi(&self, value: bool) {
        self.state.borrow_mut().pending_nmi = value;
    }

    pub fn pending_pendsv(&self) -> bool {
        self.state.borrow().pending_pendsv
    }

    pub fn set_pending_pendsv(&self, value: bool) {
        self.state.borrow_mut().pending_pendsv = value;
    }

    pub fn pending_svc(&self) -> bool {
        self.state.borrow().pending_svc
    }

    pub fn set_pending_svc(&self, value: bool) {
        self.state.borrow_mut().pending_svc = value;
    }

    pub fn pending_systick(&self) -> bool {
        self.state.borrow().pending_systick
    }

    pub fn set_pending_systick(&self, value: bool) {
        self.state.borrow_mut().pending_systick = value;
    }

    pub fn pm(&self) -> bool {
        self.state.borrow().pm
    }

    pub fn set_pm(&self, value: bool) {
        self.state.borrow_mut().pm = value;
    }

    pub fn set_spsel(&self, value: StackPointerBank) {
        self.state.borrow_mut().spsel = value;
    }

    pub fn spsel(&self) -> StackPointerBank {
        self.state.borrow().spsel
    }

    pub fn set_npriv(&self, value: bool) {
        self.state.borrow_mut().npriv = value;
    }

    pub fn npriv(&self) -> bool {
        self.state.borrow().npriv
    }

    pub fn set_interrupts_updated(&self, value: bool) {
        self.state.borrow_mut().interrupts_updated = value;
    }

    pub fn interrupt_priorities(&self) -> [u32; 4] {
        self.state.borrow().interrupt_priorities
    }

    pub fn set_interrupt_priorities(&self, value: [u32; 4]) {
        self.state.borrow_mut().interrupt_priorities = value;
    }

    pub fn vtor(&self) -> u32 {
        self.state.borrow().vtor
    }

    pub fn set_vtor(&self, value: u32) {
        self.state.borrow_mut().vtor = value;
    }

    pub fn shpr2(&self) -> u32 {
        self.state.borrow().shpr2
    }
    pub fn set_shpr2(&self, value: u32) {
        self.state.borrow_mut().shpr2 = value;
    }
    pub fn shpr3(&self) -> u32 {
        self.state.borrow().shpr3
    }
    pub fn set_shpr3(&self, value: u32) {
        self.state.borrow_mut().shpr3 = value;
    }

    pub fn waiting(&self) -> bool {
        self.state.borrow().waiting
    }

    pub fn set_interrupt(&self, irq: u32, value: bool) {
        let bit = 1u32.wrapping_shl(irq);
        let should_wake = {
            let mut state = self.state.borrow_mut();
            if value && state.pending_interrupts & bit == 0 {
                state.pending_interrupts |= bit;
                state.interrupts_updated = true;
                state.waiting
            } else {
                if !value {
                    state.pending_interrupts &= !bit;
                }
                false
            }
        };
        if should_wake && self.check_for_interrupts() {
            self.state.borrow_mut().waiting = false;
        }
    }

    pub fn step(&self) {
        let _ = self.execute_instruction();
    }

    pub fn execute_instruction(&self) -> u32 {
        if self.state.borrow().interrupts_updated && self.check_for_interrupts() {
            self.state.borrow_mut().waiting = false;
        }
        let Some(bus) = self.bus() else {
            return 0;
        };
        let opcode_pc = self.pc() & !1;
        let opcode = bus.read_u16(opcode_pc);
        let wide_instruction = opcode >> 12 == 0b1111 || opcode >> 11 == 0b11101;
        let opcode2 = if wide_instruction {
            bus.read_u16(opcode_pc.wrapping_add(2))
        } else {
            0
        };
        self.set_pc(self.pc().wrapping_add(2));
        let mut delta_cycles = 1u32;

        if opcode >> 6 == 0b0100000101 {
            // ADCS Rdn, Rm
            let rdn = (opcode & 0x7) as usize;
            let rm = ((opcode >> 3) & 0x7) as usize;
            let carry = u32::from(self.c());
            let result = self.add_with_carry(self.reg(rdn), self.reg(rm), carry, true);
            self.set_reg(rdn, result);
        } else if opcode >> 11 == 0b10101 {
            // ADD Rd, SP, #imm8
            let rd = ((opcode >> 8) & 0x7) as usize;
            let imm = (opcode & 0xff) << 2;
            let result = self.sp().wrapping_add(u32::from(imm));
            self.set_reg(rd, result);
        } else if opcode >> 7 == 0b1_0110_0000 {
            // ADD SP, #imm7
            let imm = ((opcode & 0x7f) << 2) as u32;
            self.set_sp(self.sp().wrapping_add(imm));
        } else if opcode >> 9 == 0b0001110 {
            // ADDS Rd, Rn, #imm3
            let rd = (opcode & 0x7) as usize;
            let rn = ((opcode >> 3) & 0x7) as usize;
            let imm = ((opcode >> 6) & 0x7) as u32;
            let result = self.add_with_carry(self.reg(rn), imm, 0, true);
            self.set_reg(rd, result);
        } else if opcode >> 11 == 0b00110 {
            // ADDS Rdn, #imm8
            let rdn = ((opcode >> 8) & 0x7) as usize;
            let imm = (opcode & 0xff) as u32;
            let result = self.add_with_carry(self.reg(rdn), imm, 0, true);
            self.set_reg(rdn, result);
        } else if opcode >> 9 == 0b0001100 {
            // ADDS Rd, Rn, Rm
            let rd = (opcode & 0x7) as usize;
            let rn = ((opcode >> 3) & 0x7) as usize;
            let rm = ((opcode >> 6) & 0x7) as usize;
            let result = self.add_with_carry(self.reg(rn), self.reg(rm), 0, true);
            self.set_reg(rd, result);
        } else if opcode >> 8 == 0b01000100 {
            // ADD (high registers), T2
            let rdn = (((opcode >> 4) & 0x8) | (opcode & 0x7)) as usize;
            let rm = ((opcode >> 3) & 0xf) as usize;
            let rdn_value = if rdn == 15 {
                opcode_pc.wrapping_add(4)
            } else {
                self.reg(rdn)
            };
            let rm_value = if rm == 15 {
                opcode_pc.wrapping_add(4)
            } else {
                self.reg(rm)
            };
            let result = rdn_value.wrapping_add(rm_value);
            if rdn == 15 {
                self.set_pc(result & !1);
                delta_cycles += 1;
            } else if rdn == 13 {
                self.set_sp(result);
            } else {
                self.set_reg(rdn, result);
            }
        } else if opcode >> 11 == 0b10100 {
            // ADR Rd, #imm8
            let rd = ((opcode >> 8) & 0x7) as usize;
            let imm = (opcode & 0xff) << 2;
            let base = opcode_pc.wrapping_add(4) & !3;
            self.set_reg(rd, base.wrapping_add(u32::from(imm)));
        } else if opcode >> 6 == 0b0100000000 {
            // ANDS Rdn, Rm
            let rdn = (opcode & 0x7) as usize;
            let rm = ((opcode >> 3) & 0x7) as usize;
            let result = self.reg(rdn) & self.reg(rm);
            self.set_reg(rdn, result);
            self.set_nz(result);
        } else if opcode >> 11 == 0b00010 {
            // ASRS Rd, Rm, #imm5
            let rd = (opcode & 0x7) as usize;
            let rm = ((opcode >> 3) & 0x7) as usize;
            let imm5 = ((opcode >> 6) & 0x1f) as u32;
            let shift = if imm5 == 0 { 32 } else { imm5 };
            let (result, carry) = self.arithmetic_shift_right(self.reg(rm), shift);
            self.set_reg(rd, result);
            self.set_nz(result);
            self.set_c(carry);
        } else if opcode >> 6 == 0b0100000100 {
            // ASRS Rdn, Rm (register)
            let rdn = (opcode & 0x7) as usize;
            let rm = ((opcode >> 3) & 0x7) as usize;
            let shift = self.reg(rm) & 0xff;
            let (result, carry) = self.arithmetic_shift_right(self.reg(rdn), shift);
            self.set_reg(rdn, result);
            self.set_nz(result);
            self.set_c(carry);
        } else if opcode >> 12 == 0b1101 && ((opcode >> 9) & 0x7) != 0b111 {
            // B (conditional)
            let mut imm8 = ((opcode & 0xff) << 1) as i32;
            if imm8 & (1 << 8) != 0 {
                imm8 = (imm8 & 0x1ff) - 0x200;
            }
            let cond = ((opcode >> 8) & 0xf) as u32;
            if self.check_condition(cond) {
                self.set_pc(self.pc().wrapping_add(imm8 as u32).wrapping_add(2));
                delta_cycles += 1;
            }
        } else if opcode >> 11 == 0b11100 {
            // B (unconditional)
            let mut imm11 = ((opcode & 0x7ff) << 1) as i32;
            if imm11 & (1 << 11) != 0 {
                imm11 = (imm11 & 0x7ff) - 0x800;
            }
            self.set_pc(self.pc().wrapping_add(imm11 as u32).wrapping_add(2));
            delta_cycles += 1;
        } else if opcode >> 6 == 0b0100001110 {
            // BICS Rdn, Rm
            let rdn = (opcode & 0x7) as usize;
            let rm = ((opcode >> 3) & 0x7) as usize;
            let result = self.reg(rdn) & !self.reg(rm);
            self.set_reg(rdn, result);
            self.set_nz(result);
        } else if opcode >> 8 == 0b10111110 {
            // BKPT
            self.state.borrow_mut().break_rewind = 2;
            self.fire_break(u32::from(opcode & 0xff));
        } else if opcode >> 11 == 0b11110 && opcode2 >> 14 == 0b11 && ((opcode2 >> 12) & 1) == 1 {
            // BL
            let imm11 = u32::from(opcode2 & 0x7ff);
            let j2 = u32::from((opcode2 >> 11) & 1);
            let j1 = u32::from((opcode2 >> 13) & 1);
            let imm10 = u32::from(opcode & 0x3ff);
            let s = u32::from((opcode >> 10) & 1);
            let i1 = 1 - (s ^ j1);
            let i2 = 1 - (s ^ j2);
            let imm32 = ((s * 0xff) << 24) | (i1 << 23) | (i2 << 22) | (imm10 << 12) | (imm11 << 1);
            self.set_lr(self.pc().wrapping_add(2) | 1);
            self.set_pc(self.pc().wrapping_add(2).wrapping_add(imm32));
            delta_cycles += 2;
        } else if opcode >> 7 == 0b010001111 && opcode & 0x7 == 0 {
            // BLX Rm
            let rm = ((opcode >> 3) & 0xf) as usize;
            self.set_lr(self.pc() | 1);
            self.set_pc(self.reg(rm) & !1);
            delta_cycles += 1;
        } else if opcode >> 7 == 0b010001110 && opcode & 0x7 == 0 {
            // BX Rm
            let rm = ((opcode >> 3) & 0xf) as usize;
            self.bx_write_pc(self.reg(rm));
            delta_cycles += 1;
        } else if opcode >> 6 == 0b0100001011 {
            // CMN (register)
            let rm = ((opcode >> 3) & 0x7) as usize;
            let rn = (opcode & 0x7) as usize;
            self.add_with_carry(self.reg(rn), self.reg(rm), 0, true);
        } else if opcode >> 11 == 0b00101 {
            // CMP immediate
            let rn = ((opcode >> 8) & 0x7) as usize;
            let imm8 = u32::from(opcode & 0xff);
            self.subtract_with_carry(self.reg(rn), imm8, 1, true);
        } else if opcode >> 6 == 0b0100001010 {
            // CMP (register)
            let rm = ((opcode >> 3) & 0x7) as usize;
            let rn = (opcode & 0x7) as usize;
            self.subtract_with_carry(self.reg(rn), self.reg(rm), 1, true);
        } else if opcode >> 8 == 0b01000101 {
            // CMP (register) T2
            let rm = ((opcode >> 3) & 0xf) as usize;
            let rn = (((opcode >> 4) & 0x8) | (opcode & 0x7)) as usize;
            self.subtract_with_carry(self.reg(rn), self.reg(rm), 1, true);
        } else if opcode == 0xb672 {
            // CPSID i
            self.state.borrow_mut().pm = true;
        } else if opcode == 0xb662 {
            // CPSIE i
            self.state.borrow_mut().pm = false;
            self.state.borrow_mut().interrupts_updated = true;
        } else if opcode == 0xf3bf && (opcode2 & 0xfff0) == 0x8f50 {
            // DMB SY
            self.set_pc(self.pc().wrapping_add(2));
            delta_cycles += 2;
        } else if opcode == 0xf3bf && (opcode2 & 0xfff0) == 0x8f40 {
            // DSB SY
            self.set_pc(self.pc().wrapping_add(2));
            delta_cycles += 2;
        } else if opcode >> 6 == 0b0100000001 {
            // EORS
            let rm = ((opcode >> 3) & 0x7) as usize;
            let rdn = (opcode & 0x7) as usize;
            let result = self.reg(rm) ^ self.reg(rdn);
            self.set_reg(rdn, result);
            self.set_nz(result);
        } else if opcode == 0xf3bf && (opcode2 & 0xfff0) == 0x8f60 {
            // ISB SY
            self.set_pc(self.pc().wrapping_add(2));
            delta_cycles += 2;
        } else if opcode >> 11 == 0b11001 {
            // LDMIA
            let rn = ((opcode >> 8) & 0x7) as usize;
            let registers = opcode & 0xff;
            let mut address = self.reg(rn);
            for i in 0..8 {
                if registers & (1 << i) != 0 {
                    self.set_reg(i, bus.read_u32(address));
                    address = address.wrapping_add(4);
                    delta_cycles += 1;
                }
            }
            if registers & (1 << rn) == 0 {
                self.set_reg(rn, address);
            }
        } else if opcode >> 11 == 0b01101 {
            // LDR (immediate)
            let imm5 = u32::from((opcode >> 6) & 0x1f) << 2;
            let rn = ((opcode >> 3) & 0x7) as usize;
            let rt = (opcode & 0x7) as usize;
            let addr = self.reg(rn).wrapping_add(imm5);
            delta_cycles += self.cycles_io(addr);
            self.set_reg(rt, bus.read_u32(addr));
        } else if opcode >> 11 == 0b10011 {
            // LDR (sp + immediate)
            let rt = ((opcode >> 8) & 0x7) as usize;
            let imm8 = u32::from(opcode & 0xff) << 2;
            let addr = self.sp().wrapping_add(imm8);
            delta_cycles += self.cycles_io(addr);
            self.set_reg(rt, bus.read_u32(addr));
        } else if opcode >> 11 == 0b01001 {
            // LDR (literal)
            let imm8 = u32::from(opcode & 0xff) << 2;
            let rt = ((opcode >> 8) & 0x7) as usize;
            let next_pc = self.pc().wrapping_add(2) & !3;
            let addr = next_pc.wrapping_add(imm8);
            delta_cycles += self.cycles_io(addr);
            self.set_reg(rt, bus.read_u32(addr));
        } else if opcode >> 9 == 0b0101100 {
            // LDR (register)
            let rm = ((opcode >> 6) & 0x7) as usize;
            let rn = ((opcode >> 3) & 0x7) as usize;
            let rt = (opcode & 0x7) as usize;
            let addr = self.reg(rm).wrapping_add(self.reg(rn));
            delta_cycles += self.cycles_io(addr);
            self.set_reg(rt, bus.read_u32(addr));
        } else if opcode >> 11 == 0b01111 {
            // LDRB (immediate)
            let imm5 = u32::from((opcode >> 6) & 0x1f);
            let rn = ((opcode >> 3) & 0x7) as usize;
            let rt = (opcode & 0x7) as usize;
            let addr = self.reg(rn).wrapping_add(imm5);
            delta_cycles += self.cycles_io(addr);
            self.set_reg(rt, u32::from(bus.read_u8(addr)));
        } else if opcode >> 9 == 0b0101110 {
            // LDRB (register)
            let rm = ((opcode >> 6) & 0x7) as usize;
            let rn = ((opcode >> 3) & 0x7) as usize;
            let rt = (opcode & 0x7) as usize;
            let addr = self.reg(rm).wrapping_add(self.reg(rn));
            delta_cycles += self.cycles_io(addr);
            self.set_reg(rt, u32::from(bus.read_u8(addr)));
        } else if opcode >> 11 == 0b10001 {
            // LDRH (immediate)
            let imm5 = u32::from((opcode >> 6) & 0x1f) << 1;
            let rn = ((opcode >> 3) & 0x7) as usize;
            let rt = (opcode & 0x7) as usize;
            let addr = self.reg(rn).wrapping_add(imm5);
            delta_cycles += self.cycles_io(addr);
            self.set_reg(rt, u32::from(bus.read_u16(addr)));
        } else if opcode >> 9 == 0b0101101 {
            // LDRH (register)
            let rm = ((opcode >> 6) & 0x7) as usize;
            let rn = ((opcode >> 3) & 0x7) as usize;
            let rt = (opcode & 0x7) as usize;
            let addr = self.reg(rm).wrapping_add(self.reg(rn));
            delta_cycles += self.cycles_io(addr);
            self.set_reg(rt, u32::from(bus.read_u16(addr)));
        } else if opcode >> 9 == 0b0101011 {
            // LDRSB
            let rm = ((opcode >> 6) & 0x7) as usize;
            let rn = ((opcode >> 3) & 0x7) as usize;
            let rt = (opcode & 0x7) as usize;
            let addr = self.reg(rm).wrapping_add(self.reg(rn));
            delta_cycles += self.cycles_io(addr);
            self.set_reg(rt, sign_extend_8(u32::from(bus.read_u8(addr))));
        } else if opcode >> 9 == 0b0101111 {
            // LDRSH
            let rm = ((opcode >> 6) & 0x7) as usize;
            let rn = ((opcode >> 3) & 0x7) as usize;
            let rt = (opcode & 0x7) as usize;
            let addr = self.reg(rm).wrapping_add(self.reg(rn));
            delta_cycles += self.cycles_io(addr);
            self.set_reg(rt, sign_extend_16(u32::from(bus.read_u16(addr))));
        } else if opcode >> 11 == 0b00000 {
            // LSLS (immediate)
            let imm5 = u32::from((opcode >> 6) & 0x1f);
            let rm = ((opcode >> 3) & 0x7) as usize;
            let rd = (opcode & 0x7) as usize;
            let input = self.reg(rm);
            let result = input.wrapping_shl(imm5);
            self.set_reg(rd, result);
            self.set_nz(result);
            self.set_c(if imm5 != 0 {
                input & (1 << (32 - imm5)) != 0
            } else {
                self.c()
            });
        } else if opcode >> 6 == 0b0100000010 {
            // LSLS (register)
            let rm = ((opcode >> 3) & 0x7) as usize;
            let rdn = (opcode & 0x7) as usize;
            let input = self.reg(rdn);
            let shift = self.reg(rm) & 0xff;
            let result = if shift >= 32 {
                0
            } else {
                input.wrapping_shl(shift)
            };
            self.set_reg(rdn, result);
            self.set_nz(result);
            self.set_c(if shift != 0 {
                input & (1u32.wrapping_shl(32 - shift)) != 0
            } else {
                self.c()
            });
        } else if opcode >> 11 == 0b00001 {
            // LSRS (immediate)
            let imm5 = u32::from((opcode >> 6) & 0x1f);
            let rm = ((opcode >> 3) & 0x7) as usize;
            let rd = (opcode & 0x7) as usize;
            let input = self.reg(rm);
            let result = if imm5 != 0 { input >> imm5 } else { 0 };
            self.set_reg(rd, result);
            self.set_nz(result);
            self.set_c(((input >> if imm5 != 0 { imm5 - 1 } else { 31 }) & 1) != 0);
        } else if opcode >> 6 == 0b0100000011 {
            // LSRS (register)
            let rm = ((opcode >> 3) & 0x7) as usize;
            let rdn = (opcode & 0x7) as usize;
            let shift = self.reg(rm) & 0xff;
            let input = self.reg(rdn);
            let result = if shift < 32 { input >> shift } else { 0 };
            self.set_reg(rdn, result);
            self.set_nz(result);
            self.set_c(if shift <= 32 && shift != 0 {
                (input >> (shift - 1)) & 1 != 0
            } else {
                false
            });
        } else if opcode >> 8 == 0b01000110 {
            // MOV (high register)
            let rm = ((opcode >> 3) & 0xf) as usize;
            let rd = (((opcode >> 4) & 0x8) | (opcode & 0x7)) as usize;
            let mut value = if rm == 15 {
                self.pc().wrapping_add(2)
            } else {
                self.reg(rm)
            };
            if rd == 15 {
                delta_cycles += 1;
                value &= !1;
            } else if rd == 13 {
                value &= !3;
            }
            if rd == 15 {
                self.set_pc(value);
            } else if rd == 13 {
                self.set_sp(value);
            } else {
                self.set_reg(rd, value);
            }
        } else if opcode >> 11 == 0b00100 {
            // MOVS Rd, #imm8
            let value = u32::from(opcode & 0xff);
            let rd = ((opcode >> 8) & 0x7) as usize;
            self.state.borrow_mut().registers[rd] = value;
            self.set_n(value & 0x8000_0000 != 0);
            self.set_z(value == 0);
        } else if opcode == 0b1111001111101111 && opcode2 >> 12 == 0b1000 {
            // MRS
            let sysm = u32::from(opcode2 & 0xff);
            let rd = ((opcode2 >> 8) & 0xf) as usize;
            self.set_reg(rd, self.read_special_register(sysm));
            self.set_pc(self.pc().wrapping_add(2));
            delta_cycles += 2;
        } else if opcode >> 4 == 0b111100111000 && opcode2 >> 8 == 0b10001000 {
            // MSR
            let sysm = u32::from(opcode2 & 0xff);
            let rn = (opcode & 0xf) as usize;
            self.write_special_register(sysm, self.reg(rn));
            self.set_pc(self.pc().wrapping_add(2));
            delta_cycles += 2;
        } else if opcode >> 6 == 0b0100001101 {
            // MULS
            let rn = ((opcode >> 3) & 0x7) as usize;
            let rdm = (opcode & 0x7) as usize;
            let result = self.reg(rn).wrapping_mul(self.reg(rdm));
            self.set_reg(rdm, result);
            self.set_nz(result);
        } else if opcode >> 6 == 0b0100001111 {
            // MVNS
            let rm = ((opcode >> 3) & 0x7) as usize;
            let rd = (opcode & 0x7) as usize;
            let result = !self.reg(rm);
            self.set_reg(rd, result);
            self.set_nz(result);
        } else if opcode >> 6 == 0b0100001100 {
            // ORRS
            let rm = ((opcode >> 3) & 0x7) as usize;
            let rdn = (opcode & 0x7) as usize;
            let result = self.reg(rdn) | self.reg(rm);
            self.set_reg(rdn, result);
            self.set_nz(result);
        } else if opcode >> 9 == 0x5a {
            // PUSH
            let mut bit_count = 0u32;
            for i in 0..=8 {
                if opcode & (1 << i) != 0 {
                    bit_count += 1;
                }
            }
            let mut address = self.sp().wrapping_sub(4 * bit_count);
            for i in 0..=7 {
                if opcode & (1 << i) != 0 {
                    bus.write_u32(address, self.reg(i));
                    delta_cycles += 1;
                    address = address.wrapping_add(4);
                }
            }
            if opcode & (1 << 8) != 0 {
                bus.write_u32(address, self.lr());
            }
            self.set_sp(self.sp().wrapping_sub(4 * bit_count));
        } else if opcode >> 9 == 0x5e {
            // POP
            let p = (opcode >> 8) & 1 != 0;
            let mut address = self.sp();
            for i in 0..=7 {
                if opcode & (1 << i) != 0 {
                    self.set_reg(i, bus.read_u32(address));
                    address = address.wrapping_add(4);
                    delta_cycles += 1;
                }
            }
            if p {
                self.set_sp(address.wrapping_add(4));
                self.bx_write_pc(bus.read_u32(address));
                delta_cycles += 2;
            } else {
                self.set_sp(address);
            }
        } else if opcode == 0b1011_1111_0000_0000 {
            // NOP
        } else if opcode >> 6 == 0b0100000111 {
            // ROR
            let rm = ((opcode >> 3) & 0x7) as usize;
            let rdn = (opcode & 0x7) as usize;
            let input = self.reg(rdn);
            let shift = self.reg(rm) & 0xff;
            let result = if shift == 0 {
                input
            } else {
                input.rotate_right(shift)
            };
            self.set_reg(rdn, result);
            self.set_nz(result);
            self.set_c(result & 0x8000_0000 != 0);
        } else if opcode >> 6 == 0b0100001001 {
            // RSBS
            let rn = ((opcode >> 3) & 0x7) as usize;
            let rd = (opcode & 0x7) as usize;
            let result = self.subtract_with_carry(0, self.reg(rn), 1, true);
            self.set_reg(rd, result);
        } else if opcode >> 6 == 0b0100000110 {
            // SBCS
            let rm = ((opcode >> 3) & 0x7) as usize;
            let rdn = (opcode & 0x7) as usize;
            let result =
                self.subtract_with_carry(self.reg(rdn), self.reg(rm), u32::from(self.c()), true);
            self.set_reg(rdn, result);
        } else if opcode == 0b1011111101000000 {
            // SEV
        } else if opcode >> 11 == 0b11000 {
            // STMIA
            let rn = ((opcode >> 8) & 0x7) as usize;
            let registers = opcode & 0xff;
            let mut address = self.reg(rn);
            for i in 0..8 {
                if registers & (1 << i) != 0 {
                    bus.write_u32(address, self.reg(i));
                    address = address.wrapping_add(4);
                    delta_cycles += 1;
                }
            }
            if registers & (1 << rn) == 0 {
                self.set_reg(rn, address);
            }
        } else if opcode >> 11 == 0b01100 {
            // STR (immediate)
            let imm5 = u32::from((opcode >> 6) & 0x1f) << 2;
            let rn = ((opcode >> 3) & 0x7) as usize;
            let rt = (opcode & 0x7) as usize;
            let address = self.reg(rn).wrapping_add(imm5);
            delta_cycles += self.cycles_io(address);
            bus.write_u32(address, self.reg(rt));
        } else if opcode >> 11 == 0b10010 {
            // STR (sp + immediate)
            let rt = ((opcode >> 8) & 0x7) as usize;
            let imm8 = u32::from(opcode & 0xff) << 2;
            let address = self.sp().wrapping_add(imm8);
            delta_cycles += self.cycles_io(address);
            bus.write_u32(address, self.reg(rt));
        } else if opcode >> 9 == 0b0101000 {
            // STR (register)
            let rm = ((opcode >> 6) & 0x7) as usize;
            let rn = ((opcode >> 3) & 0x7) as usize;
            let rt = (opcode & 0x7) as usize;
            let address = self.reg(rm).wrapping_add(self.reg(rn));
            delta_cycles += self.cycles_io(address);
            bus.write_u32(address, self.reg(rt));
        } else if opcode >> 11 == 0b01110 {
            // STRB (immediate)
            let imm5 = u32::from((opcode >> 6) & 0x1f);
            let rn = ((opcode >> 3) & 0x7) as usize;
            let rt = (opcode & 0x7) as usize;
            let address = self.reg(rn).wrapping_add(imm5);
            delta_cycles += self.cycles_io(address);
            bus.write_u8(address, self.reg(rt) as u8);
        } else if opcode >> 9 == 0b0101010 {
            // STRB (register)
            let rm = ((opcode >> 6) & 0x7) as usize;
            let rn = ((opcode >> 3) & 0x7) as usize;
            let rt = (opcode & 0x7) as usize;
            let address = self.reg(rm).wrapping_add(self.reg(rn));
            delta_cycles += self.cycles_io(address);
            bus.write_u8(address, self.reg(rt) as u8);
        } else if opcode >> 11 == 0b10000 {
            // STRH (immediate)
            let imm5 = u32::from((opcode >> 6) & 0x1f) << 1;
            let rn = ((opcode >> 3) & 0x7) as usize;
            let rt = (opcode & 0x7) as usize;
            let address = self.reg(rn).wrapping_add(imm5);
            delta_cycles += self.cycles_io(address);
            bus.write_u16(address, self.reg(rt) as u16);
        } else if opcode >> 9 == 0b0101001 {
            // STRH (register)
            let rm = ((opcode >> 6) & 0x7) as usize;
            let rn = ((opcode >> 3) & 0x7) as usize;
            let rt = (opcode & 0x7) as usize;
            let address = self.reg(rm).wrapping_add(self.reg(rn));
            delta_cycles += self.cycles_io(address);
            bus.write_u16(address, self.reg(rt) as u16);
        } else if opcode >> 7 == 0b101100001 {
            // SUB SP, #imm7
            let imm = u32::from((opcode & 0x7f) << 2);
            self.set_sp(self.sp().wrapping_sub(imm));
        } else if opcode >> 9 == 0b0001111 {
            // SUBS Rd, Rn, #imm3
            let imm3 = u32::from((opcode >> 6) & 0x7);
            let rn = ((opcode >> 3) & 0x7) as usize;
            let rd = (opcode & 0x7) as usize;
            let result = self.subtract_with_carry(self.reg(rn), imm3, 1, true);
            self.set_reg(rd, result);
        } else if opcode >> 11 == 0b00111 {
            // SUBS Rdn, #imm8
            let imm8 = u32::from(opcode & 0xff);
            let rdn = ((opcode >> 8) & 0x7) as usize;
            let result = self.subtract_with_carry(self.reg(rdn), imm8, 1, true);
            self.set_reg(rdn, result);
        } else if opcode >> 9 == 0b0001101 {
            // SUBS Rd, Rn, Rm
            let rm = ((opcode >> 6) & 0x7) as usize;
            let rn = ((opcode >> 3) & 0x7) as usize;
            let rd = (opcode & 0x7) as usize;
            let result = self.subtract_with_carry(self.reg(rn), self.reg(rm), 1, true);
            self.set_reg(rd, result);
        } else if opcode >> 8 == 0b11011111 {
            // SVC
            self.state.borrow_mut().pending_svc = true;
            self.state.borrow_mut().interrupts_updated = true;
        } else if opcode >> 6 == 0b1011001001 {
            // SXTB
            let rm = ((opcode >> 3) & 0x7) as usize;
            let rd = (opcode & 0x7) as usize;
            self.set_reg(rd, sign_extend_8(self.reg(rm)));
        } else if opcode >> 6 == 0b1011001000 {
            // SXTH
            let rm = ((opcode >> 3) & 0x7) as usize;
            let rd = (opcode & 0x7) as usize;
            self.set_reg(rd, sign_extend_16(self.reg(rm)));
        } else if opcode >> 6 == 0b0100001000 {
            // TST
            let rm = ((opcode >> 3) & 0x7) as usize;
            let rn = (opcode & 0x7) as usize;
            let result = self.reg(rn) & self.reg(rm);
            self.set_nz(result);
        } else if opcode >> 8 == 0b11011110 {
            // UDF
            self.state.borrow_mut().break_rewind = 2;
            self.fire_break(u32::from(opcode & 0xff));
        } else if opcode >> 4 == 0b111101111111 && opcode2 >> 12 == 0b1010 {
            // UDF (T2)
            let imm4 = u32::from(opcode & 0xf);
            let imm12 = u32::from(opcode2 & 0xfff);
            self.state.borrow_mut().break_rewind = 4;
            self.fire_break((imm4 << 12) | imm12);
            self.set_pc(self.pc().wrapping_add(2));
        } else if opcode >> 6 == 0b1011001011 {
            // UXTB
            let rm = ((opcode >> 3) & 0x7) as usize;
            let rd = (opcode & 0x7) as usize;
            self.set_reg(rd, self.reg(rm) & 0xff);
        } else if opcode >> 6 == 0b1011001010 {
            // UXTH
            let rm = ((opcode >> 3) & 0x7) as usize;
            let rd = (opcode & 0x7) as usize;
            self.set_reg(rd, self.reg(rm) & 0xffff);
        } else if opcode == 0b1011111100100000 {
            // WFE
            delta_cycles += 1;
            self.state.borrow_mut().waiting = true;
        } else if opcode == 0b1011111100110000 {
            // WFI
            delta_cycles += 1;
            self.state.borrow_mut().waiting = true;
        } else if opcode == 0b1011111100010000 {
            // YIELD
        } else {
            let board = self.board.upgrade();
            if let Some(board) = board {
                board.borrow().logger.warn(
                    "CortexM0Core",
                    &format!("Instruction at {opcode_pc:x} is not implemented yet: {opcode:x}"),
                );
            }
        }
        self.state.borrow_mut().cycles += u64::from(delta_cycles);
        delta_cycles
    }

    fn check_condition(&self, cond: u32) -> bool {
        let result = match cond >> 1 {
            0b000 => self.z(),
            0b001 => self.c(),
            0b010 => self.n(),
            0b011 => self.v(),
            0b100 => self.c() && !self.z(),
            0b101 => self.n() == self.v(),
            0b110 => self.n() == self.v() && !self.z(),
            0b111 => true,
            _ => false,
        };
        if cond & 1 != 0 && cond != 0b1111 {
            !result
        } else {
            result
        }
    }

    fn subtract_with_carry(&self, a: u32, b: u32, carry: u32, set_flags: bool) -> u32 {
        let borrow = 1u64 - u64::from(carry);
        let subtrahend = u64::from(b) + borrow;
        let result = (u64::from(a)).wrapping_sub(subtrahend) as u32;
        if set_flags {
            self.set_n(result & 0x8000_0000 != 0);
            self.set_z(result == 0);
            self.set_c(u64::from(a) >= subtrahend);
            let overflow = ((a ^ b) & (a ^ result) & 0x8000_0000) != 0;
            self.set_v(overflow);
        }
        result
    }

    fn cycles_io(&self, address: u32) -> u32 {
        if (0x2000_0000..0x2004_2000).contains(&address)
            || (0x1000_0000..0x1400_0000).contains(&address)
            || address < 0x1000
        {
            0
        } else {
            1
        }
    }

    fn fire_break(&self, code: u32) {
        if let Some(board) = self.board.upgrade() {
            let mut callback = board.borrow_mut().on_break.take();
            if let Some(callback) = callback.as_mut() {
                callback(code);
            }
            board.borrow_mut().on_break = callback;
        }
    }

    fn set_nz(&self, value: u32) {
        self.set_n(value & 0x8000_0000 != 0);
        self.set_z(value == 0);
    }

    fn add_with_carry(&self, a: u32, b: u32, carry: u32, set_flags: bool) -> u32 {
        let sum = u64::from(a) + u64::from(b) + u64::from(carry);
        let result = sum as u32;
        if set_flags {
            self.set_n(result & 0x8000_0000 != 0);
            self.set_z(result == 0);
            self.set_c(sum > u64::from(u32::MAX));
            let overflow = (!(a ^ b) & (a ^ result) & 0x8000_0000) != 0;
            self.set_v(overflow);
        }
        result
    }

    fn arithmetic_shift_right(&self, value: u32, shift: u32) -> (u32, bool) {
        if shift == 0 {
            return (value, self.c());
        }
        let shift = shift.min(32);
        let carry = if shift >= 32 {
            value & 0x8000_0000 != 0
        } else {
            value & (1 << (shift - 1)) != 0
        };
        let result = if shift >= 32 {
            if value & 0x8000_0000 != 0 {
                0xffff_ffff
            } else {
                0
            }
        } else {
            ((value as i32) >> shift) as u32
        };
        (result, carry)
    }

    fn bx_write_pc(&self, address: u32) {
        let state = self.state.borrow();
        if state.current_mode == ExecutionMode::Handler && address >> 28 == 0b1111 {
            drop(state);
            self.exception_return(address & 0x0fff_ffff);
        } else {
            drop(state);
            self.set_pc(address & !1);
        }
    }

    fn check_for_interrupts(&self) -> bool {
        let (pending, enabled, priorities, ipsr, waiting, pm) = {
            let state = self.state.borrow();
            (
                state.pending_interrupts,
                state.enabled_interrupts,
                state.interrupt_priorities,
                state.ipsr,
                state.waiting,
                state.pm,
            )
        };
        let current_priority = if waiting {
            if pm {
                self.exception_priority(ipsr)
            } else {
                LOWEST_PRIORITY as i32
            }
        } else {
            self.exception_priority(ipsr)
                .min(if pm { 0 } else { LOWEST_PRIORITY as i32 })
        };
        if self.pending_nmi() {
            self.set_pending_nmi(false);
            self.exception_entry(2);
            return true;
        }
        if current_priority <= 0 {
            self.state.borrow_mut().interrupts_updated = false;
            return false;
        }
        let interrupt_set = pending & enabled;
        let svc_priority = self.svc_priority();
        let pendsv_priority = self.pendsv_priority();
        let systick_priority = self.systick_priority();
        for priority in 0..current_priority as u32 {
            if self.pending_svc() && priority == svc_priority as u32 {
                self.set_pending_svc(false);
                self.exception_entry(EXC_SVCALL);
                return true;
            }
            if self.pending_pendsv() && priority == pendsv_priority as u32 {
                self.set_pending_pendsv(false);
                self.exception_entry(EXC_PENDSV);
                return true;
            }
            if self.pending_systick() && priority == systick_priority as u32 {
                self.set_pending_systick(false);
                self.exception_entry(EXC_SYSTICK);
                return true;
            }
            let level = interrupt_set & priorities[priority as usize];
            if level != 0 {
                for n in 0..32 {
                    if level & (1 << n) != 0 {
                        if n > crate::irq::MAX_HARDWARE_IRQ {
                            self.state.borrow_mut().pending_interrupts &= !(1 << n);
                        }
                        self.exception_entry(16 + n);
                        return true;
                    }
                }
            }
        }
        self.state.borrow_mut().interrupts_updated = false;
        false
    }

    fn svc_priority(&self) -> i32 {
        (self.state.borrow().shpr2 >> 30) as i32
    }

    fn pendsv_priority(&self) -> i32 {
        ((self.state.borrow().shpr3 >> 22) & 0x3) as i32
    }

    fn systick_priority(&self) -> i32 {
        (self.state.borrow().shpr3 >> 30) as i32
    }

    pub fn exception_priority(&self, number: u32) -> i32 {
        match number {
            1 => -3,
            2 => -2,
            3 => -1,
            EXC_SVCALL => self.svc_priority(),
            EXC_PENDSV => self.pendsv_priority(),
            EXC_SYSTICK => self.systick_priority(),
            n if n < 16 => LOWEST_PRIORITY as i32,
            n => {
                let int_num = n - 16;
                let priorities = self.state.borrow().interrupt_priorities;
                for (priority, bits) in priorities.iter().enumerate() {
                    if bits & (1 << int_num) != 0 {
                        return priority as i32;
                    }
                }
                LOWEST_PRIORITY as i32
            }
        }
    }

    pub fn vect_pending(&self) -> u32 {
        if self.pending_nmi() {
            return 2;
        }
        let pending = self.state.borrow().pending_interrupts;
        let priorities = self.state.borrow().interrupt_priorities;
        let svc_priority = self.svc_priority();
        let pendsv_priority = self.pendsv_priority();
        let systick_priority = self.systick_priority();
        for priority in 0..LOWEST_PRIORITY {
            if self.pending_svc() && priority == svc_priority as u32 {
                return EXC_SVCALL;
            }
            if self.pending_pendsv() && priority == pendsv_priority as u32 {
                return EXC_PENDSV;
            }
            if self.pending_systick() && priority == systick_priority as u32 {
                return EXC_SYSTICK;
            }
            let level = pending & priorities[priority as usize];
            if level != 0 {
                for n in 0..32 {
                    if level & (1 << n) != 0 {
                        return 16 + n;
                    }
                }
            }
        }
        0
    }

    fn exception_entry(&self, exception_number: u32) {
        let Some(bus) = self.bus() else {
            return;
        };
        let (current_mode, spsel) = {
            let state = self.state.borrow();
            (state.current_mode, state.spsel)
        };
        let use_process =
            current_mode == ExecutionMode::Thread && spsel == StackPointerBank::Process;
        let stack_pointer = if use_process { self.psp() } else { self.msp() };
        let frame_align = u32::from(stack_pointer & 0b100 != 0);
        let frame_ptr = stack_pointer.wrapping_sub(0x20) & !0b100;
        if use_process {
            self.set_psp(frame_ptr);
        } else {
            self.set_msp(frame_ptr);
        }
        let xpsr = self.xpsr();
        let lr = self.lr();
        let pc = self.pc();
        bus.write_u32(frame_ptr, self.reg(0));
        bus.write_u32(frame_ptr.wrapping_add(4), self.reg(1));
        bus.write_u32(frame_ptr.wrapping_add(8), self.reg(2));
        bus.write_u32(frame_ptr.wrapping_add(12), self.reg(3));
        bus.write_u32(frame_ptr.wrapping_add(16), self.reg(12));
        bus.write_u32(frame_ptr.wrapping_add(20), lr);
        bus.write_u32(frame_ptr.wrapping_add(24), pc & !1);
        bus.write_u32(
            frame_ptr.wrapping_add(28),
            (xpsr & !(1 << 9)) | (frame_align << 9),
        );
        let mut state = self.state.borrow_mut();
        state.registers[14] = if current_mode == ExecutionMode::Handler {
            0xffff_fff1
        } else if spsel == StackPointerBank::Main {
            0xffff_fff9
        } else {
            0xffff_fffd
        };
        state.current_mode = ExecutionMode::Handler;
        state.ipsr = exception_number;
        let vector = state.vtor;
        drop(state);
        self.switch_stack(StackPointerBank::Main);
        self.set_pc(bus.read_u32(vector.wrapping_add(4 * exception_number)));
    }

    fn exception_return(&self, exc_return: u32) {
        let Some(bus) = self.bus() else {
            return;
        };
        let frame_ptr = if exc_return & 0xf == 0b1101 {
            self.psp()
        } else {
            self.msp()
        };
        self.set_reg(0, bus.read_u32(frame_ptr));
        self.set_reg(1, bus.read_u32(frame_ptr.wrapping_add(4)));
        self.set_reg(2, bus.read_u32(frame_ptr.wrapping_add(8)));
        self.set_reg(3, bus.read_u32(frame_ptr.wrapping_add(12)));
        self.set_reg(12, bus.read_u32(frame_ptr.wrapping_add(16)));
        self.set_lr(bus.read_u32(frame_ptr.wrapping_add(20)));
        self.set_pc(bus.read_u32(frame_ptr.wrapping_add(24)));
        let psr = bus.read_u32(frame_ptr.wrapping_add(28));
        let frame_align = if psr & (1 << 9) != 0 { 0b100 } else { 0 };
        match exc_return & 0xf {
            0b0001 => {
                self.state.borrow_mut().current_mode = ExecutionMode::Handler;
                self.switch_stack(StackPointerBank::Main);
                self.set_sp((frame_ptr.wrapping_add(0x20)) | frame_align);
            }
            0b1001 => {
                self.state.borrow_mut().current_mode = ExecutionMode::Thread;
                self.switch_stack(StackPointerBank::Main);
                self.set_sp((frame_ptr.wrapping_add(0x20)) | frame_align);
            }
            0b1101 => {
                self.state.borrow_mut().current_mode = ExecutionMode::Thread;
                self.switch_stack(StackPointerBank::Process);
                self.set_sp((frame_ptr.wrapping_add(0x20)) | frame_align);
            }
            _ => {}
        }
        let mut state = self.state.borrow_mut();
        state.n = psr & 0x8000_0000 != 0;
        state.z = psr & 0x4000_0000 != 0;
        state.c = psr & 0x2000_0000 != 0;
        state.v = psr & 0x1000_0000 != 0;
        let force_thread = state.current_mode == ExecutionMode::Thread && state.npriv;
        state.ipsr = if force_thread { 0 } else { psr & 0x3f };
        state.interrupts_updated = true;
    }

    pub fn read_special_register(&self, sysm: u32) -> u32 {
        match sysm {
            SYSM_APSR => self.apsr(),
            SYSM_XPSR => self.xpsr(),
            SYSM_IPSR => self.ipsr(),
            SYSM_MSP => self.msp(),
            SYSM_PSP => self.psp(),
            SYSM_PRIMASK => u32::from(self.pm()),
            SYSM_CONTROL => {
                let state = self.state.borrow();
                (u32::from(state.npriv))
                    | (u32::from(state.spsel == StackPointerBank::Process) << 1)
            }
            _ => 0,
        }
    }

    pub fn write_special_register(&self, sysm: u32, value: u32) {
        match sysm {
            SYSM_APSR => self.set_apsr(value),
            SYSM_XPSR => self.set_xpsr(value),
            SYSM_IPSR => self.set_ipsr(value),
            SYSM_MSP => self.set_msp(value),
            SYSM_PSP => self.set_psp(value),
            SYSM_PRIMASK => {
                self.state.borrow_mut().pm = value & 1 != 0;
                self.state.borrow_mut().interrupts_updated = true;
            }
            SYSM_CONTROL => {
                self.state.borrow_mut().npriv = value & 1 != 0;
                let current_mode = self.state.borrow().current_mode;
                if current_mode == ExecutionMode::Thread {
                    self.switch_stack(if value & 2 != 0 {
                        StackPointerBank::Process
                    } else {
                        StackPointerBank::Main
                    });
                }
            }
            _ => {}
        }
    }
}

pub fn sign_extend_8(value: u32) -> u32 {
    ((value << 24) as i32 >> 24) as u32
}

pub fn sign_extend_16(value: u32) -> u32 {
    ((value << 16) as i32 >> 16) as u32
}

pub fn exception_priority_constants() -> (u32, u32, u32) {
    (EXC_SVCALL, EXC_PENDSV, EXC_SYSTICK)
}

#[cfg(test)]
mod tests {
    use crate::test_support::{Registers, TestDriver};
    use crate::utils::assembler::{
        opcode_adcs, opcode_add_reg, opcode_add_sp2, opcode_add_sp_plus_imm, opcode_adds1,
        opcode_adds2, opcode_adds_reg, opcode_adr, opcode_ands, opcode_asrs, opcode_asrs_reg,
        opcode_bics, opcode_nop,
    };

    fn driver() -> TestDriver {
        let driver = TestDriver::new();
        driver.set_pc(0x2000_0000);
        driver
    }

    #[test]
    fn should_execute_adcs_r5_r4() {
        let cpu = driver();
        cpu.write_u16(0x2000_0000, opcode_adcs(5, 4));
        cpu.set_registers(&[(4, 55), (5, 66)]);
        cpu.set_flags(false, false, true, false);
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r5, 122);
        assert!(!r.n && !r.z && !r.c && !r.v);
    }

    #[test]
    fn should_execute_adcs_r5_r4_and_set_negative_overflow_flags() {
        let cpu = driver();
        cpu.write_u16(0x2000_0000, opcode_adcs(5, 4));
        cpu.set_registers(&[(4, 0x7fff_ffff), (5, 0)]);
        cpu.set_flags(false, false, true, false);
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r5, 0x8000_0000);
        assert!(r.n && !r.z && !r.c && r.v);
    }

    #[test]
    fn should_not_set_overflow_for_adcs_zero_plus_zero_with_carry() {
        let cpu = driver();
        cpu.write_u16(0x2000_0000, opcode_adcs(3, 2));
        cpu.set_registers(&[(2, 0), (3, 0)]);
        cpu.set_flags(false, true, true, false);
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r3, 1);
        assert!(!r.n && !r.z && !r.c && !r.v);
    }

    #[test]
    fn should_set_zero_carry_overflow_for_adcs_0x80000000_twice() {
        let cpu = driver();
        cpu.write_u16(0x2000_0000, opcode_adcs(0, 0));
        cpu.set_registers(&[(0, 0x8000_0000)]);
        cpu.set_flags(false, false, false, false);
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r0, 0);
        assert!(!r.n && r.z && r.c && r.v);
    }

    #[test]
    fn should_execute_add_sp_0x10() {
        let cpu = driver();
        cpu.set_registers(&[(13, 0x1000_0040)]);
        cpu.write_u16(0x2000_0000, opcode_add_sp2(0x10));
        cpu.single_step();
        assert_eq!(cpu.read_registers().sp, 0x1000_0050);
    }

    #[test]
    fn should_execute_add_r1_sp_4() {
        let cpu = driver();
        cpu.set_registers(&[(13, 0x54), (1, 0)]);
        cpu.write_u16(0x2000_0000, opcode_add_sp_plus_imm(1, 0x10));
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.sp, 0x54);
        assert_eq!(r.r1, 0x64);
    }

    #[test]
    fn should_execute_adds_r1_r2_3() {
        let cpu = driver();
        cpu.write_u16(0x2000_0000, opcode_adds1(1, 2, 3));
        cpu.set_registers(&[(2, 2)]);
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r1, 5);
        assert!(!r.n && !r.z && !r.c && !r.v);
    }

    #[test]
    fn should_execute_adds_r1_1() {
        let cpu = driver();
        cpu.write_u16(0x2000_0000, opcode_adds2(1, 1));
        cpu.set_registers(&[(1, 0xffff_ffff)]);
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r1, 0);
        assert!(!r.n && r.z && r.c && !r.v);
    }

    #[test]
    fn should_execute_adds_r1_r2_r7() {
        let cpu = driver();
        cpu.write_u16(0x2000_0000, opcode_adds_reg(1, 2, 7));
        cpu.set_registers(&[(2, 2), (7, 27)]);
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r1, 29);
        assert!(!r.n && !r.z && !r.c && !r.v);
    }

    #[test]
    fn should_execute_adds_r4_r4_r2() {
        let cpu = driver();
        cpu.write_u16(0x2000_0000, opcode_adds_reg(4, 4, 2));
        cpu.set_registers(&[(2, 0x74bc_8000), (4, 0x4374_0000)]);
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r4, 0xb830_8000);
        assert!(r.n && !r.z && !r.c && r.v);
    }

    #[test]
    fn should_execute_adds_r1_r1_r1() {
        let cpu = driver();
        cpu.write_u16(0x2000_0000, opcode_adds_reg(1, 1, 1));
        cpu.set_registers(&[(1, 0xbf8d_1424)]);
        cpu.set_flags(false, false, true, false);
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r1, 0x7f1a_2848);
        assert!(!r.n && !r.z && r.c && r.v);
    }

    #[test]
    fn should_execute_add_r1_ip() {
        let cpu = driver();
        cpu.write_u16(0x2000_0000, opcode_add_reg(1, 12));
        cpu.set_registers(&[(1, 66), (12, 44)]);
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r1, 110);
        assert!(!r.n && !r.z && !r.c && !r.v);
    }

    #[test]
    fn should_not_update_flags_for_add_r3_r12_t2() {
        let cpu = driver();
        cpu.write_u16(0x2000_0000, opcode_add_reg(3, 12));
        cpu.set_registers(&[(3, 0x0000_2000), (12, 0xffff_e000)]);
        cpu.set_flags(true, true, true, true);
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r3, 0);
        assert!(r.n && r.z && r.c && r.v);
    }

    #[test]
    fn should_execute_add_sp_r8_and_not_update_flags() {
        let cpu = driver();
        cpu.write_u16(0x2000_0000, opcode_add_reg(13, 8));
        cpu.set_registers(&[(13, 0x2003_0000), (8, 0x13)]);
        cpu.set_flags(false, true, false, false);
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.sp, 0x2003_0010);
        assert!(r.z);
    }

    #[test]
    fn should_execute_add_pc_r8() {
        let cpu = driver();
        cpu.write_u16(0x2000_0000, opcode_add_reg(15, 8));
        cpu.set_registers(&[(8, 0x11)]);
        cpu.single_step();
        assert_eq!(cpu.read_registers().pc, 0x2000_0014);
    }

    #[test]
    fn should_execute_add_r7_pc_word_aligned() {
        let cpu = driver();
        cpu.write_u16(0x2000_0000, opcode_add_reg(7, 15));
        cpu.set_registers(&[(7, 0)]);
        cpu.single_step();
        assert_eq!(cpu.read_registers().r7, 0x2000_0004);
    }

    #[test]
    fn should_execute_add_r7_pc_halfword_aligned() {
        let cpu = driver();
        cpu.write_u16(0x2000_0000, opcode_nop());
        cpu.write_u16(0x2000_0002, opcode_add_reg(7, 15));
        cpu.set_registers(&[(7, 0)]);
        cpu.single_step();
        cpu.single_step();
        assert_eq!(cpu.read_registers().r7, 0x2000_0006);
    }

    #[test]
    fn should_execute_adr_r4_0x50() {
        let cpu = driver();
        cpu.write_u16(0x2000_0000, opcode_adr(4, 0x50));
        cpu.single_step();
        assert_eq!(cpu.read_registers().r4, 0x2000_0054);
    }

    #[test]
    fn should_execute_ands_r5_r0() {
        let cpu = driver();
        cpu.write_u16(0x2000_0000, opcode_ands(5, 0));
        cpu.set_registers(&[(5, 0xffff_0000), (0, 0xf00f_ffff)]);
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r5, 0xf00f_0000);
        assert!(r.n && !r.z);
    }

    #[test]
    fn should_execute_asrs_r3_r2_31() {
        let cpu = driver();
        cpu.write_u16(0x2000_0000, opcode_asrs(3, 2, 31));
        cpu.set_registers(&[(2, 0x8000_0000)]);
        cpu.set_flags(false, false, true, false);
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r3, 0xffff_ffff);
        assert_eq!(r.pc, 0x2000_0002);
        assert!(r.n && !r.z && !r.c);
    }

    #[test]
    fn should_update_carry_for_asrs_r3_r2_32() {
        let cpu = driver();
        cpu.write_u16(0x2000_0000, opcode_asrs(3, 2, 0));
        cpu.set_registers(&[(2, 0x8000_0000)]);
        cpu.set_flags(false, false, false, false);
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r3, 0xffff_ffff);
        assert_eq!(r.pc, 0x2000_0002);
        assert!(r.n && !r.z && r.c);
    }

    #[test]
    fn should_execute_asrs_r3_r4_shift_7() {
        let cpu = driver();
        cpu.write_u16(0x2000_0000, opcode_asrs_reg(3, 4));
        cpu.set_registers(&[(3, 0x8000_0040), (4, 0xff50_0007)]);
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r3, 0xff00_0000);
        assert_eq!(r.pc, 0x2000_0002);
        assert!(r.n && !r.z && r.c);
    }

    #[test]
    fn should_execute_asrs_r3_r4_shift_50() {
        let cpu = driver();
        cpu.write_u16(0x2000_0000, opcode_asrs_reg(3, 4));
        cpu.set_registers(&[(3, 0x4000_0040), (4, 50)]);
        cpu.set_flags(false, false, true, false);
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r3, 0);
        assert_eq!(r.pc, 0x2000_0002);
        assert!(!r.n && r.z && !r.c);
    }

    #[test]
    fn should_execute_asrs_r3_r4_shift_31() {
        let cpu = driver();
        cpu.write_u16(0x2000_0000, opcode_asrs_reg(3, 4));
        cpu.set_registers(&[(3, 0x4000_0040), (4, 31)]);
        cpu.set_flags(false, false, true, false);
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r3, 0);
        assert_eq!(r.pc, 0x2000_0002);
        assert!(!r.n && r.z && r.c);
    }

    #[test]
    fn should_execute_asrs_r3_r4_negative_shift_50() {
        let cpu = driver();
        cpu.write_u16(0x2000_0000, opcode_asrs_reg(3, 4));
        cpu.set_registers(&[(3, 0x8000_0040), (4, 50)]);
        cpu.set_flags(false, false, true, false);
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r3, 0xffff_ffff);
        assert_eq!(r.pc, 0x2000_0002);
        assert!(r.n && !r.z && r.c);
    }

    #[test]
    fn should_execute_asrs_r3_r4_shift_0() {
        let cpu = driver();
        cpu.write_u16(0x2000_0000, opcode_asrs_reg(3, 4));
        cpu.set_registers(&[(3, 0x8000_0040), (4, 0)]);
        cpu.set_flags(false, false, true, false);
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r3, 0x8000_0040);
        assert_eq!(r.pc, 0x2000_0002);
        assert!(r.n && !r.z && r.c);
    }

    #[test]
    fn should_execute_bics_r0_r3() {
        let cpu = driver();
        cpu.set_registers(&[(0, 0xff), (3, 0x0f)]);
        cpu.write_u16(0x2000_0000, opcode_bics(0, 3));
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r0, 0xf0);
        assert!(!r.n && !r.z);
    }

    #[test]
    fn should_execute_bics_r0_r3_and_set_negative() {
        let cpu = driver();
        cpu.set_registers(&[(0, 0xffff_ffff), (3, 0x0000_ffff)]);
        cpu.write_u16(0x2000_0000, opcode_bics(0, 3));
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r0, 0xffff_0000);
        assert!(r.n && !r.z);
    }

    use crate::utils::assembler::{
        opcode_bl, opcode_blx, opcode_bt1, opcode_bt2, opcode_bx, opcode_cmn, opcode_cmp_imm,
        opcode_cmp_reg_t1, opcode_cmp_reg_t2, opcode_dmbsy, opcode_dsbsy, opcode_eors,
        opcode_isbsy, opcode_ldmia, opcode_ldr_imm, opcode_ldr_lit, opcode_ldr_reg, opcode_ldr_sp,
        opcode_ldrb, opcode_ldrb_reg, opcode_ldrh, opcode_ldrh_reg, opcode_ldrsb, opcode_ldrsh,
        opcode_lsls_imm, opcode_mov, opcode_movs, opcode_mrs, opcode_msr, opcode_muls, opcode_mvns,
        opcode_orrs, opcode_pop, opcode_push, opcode_udf, opcode_udf2,
    };

    fn run16_at(pc: u32, opcode: u16) -> Registers {
        let cpu = TestDriver::new();
        cpu.set_pc(pc);
        cpu.write_u16(pc, opcode);
        cpu.single_step();
        cpu.read_registers()
    }

    fn run32_at(pc: u32, opcode: u32) -> Registers {
        let cpu = TestDriver::new();
        cpu.set_pc(pc);
        cpu.write_u32(pc, opcode);
        cpu.single_step();
        cpu.read_registers()
    }

    #[test]
    fn should_execute_bl_0x34() {
        let r = run32_at(0x2000_0000, opcode_bl(0x34));
        assert_eq!(r.pc, 0x2000_0038);
        assert_eq!(r.lr, 0x2000_0005);
    }

    #[test]
    fn should_execute_bl_minus_0x10() {
        let r = run32_at(0x2000_0000, opcode_bl(-0x10));
        assert_eq!(r.pc, 0x2000_0004u32.wrapping_sub(0x10));
        assert_eq!(r.lr, 0x2000_0005);
    }

    #[test]
    fn should_execute_bl_minus_3242() {
        let r = run32_at(0x2000_0000, opcode_bl(-3242));
        assert_eq!(r.pc, 0x2000_0004u32.wrapping_sub(3242));
        assert_eq!(r.lr, 0x2000_0005);
    }

    #[test]
    fn should_execute_blx_r3() {
        let cpu = driver();
        cpu.set_registers(&[(3, 0x2000_0201)]);
        cpu.write_u32(0x2000_0000, opcode_blx(3) as u32);
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.pc, 0x2000_0200);
        assert_eq!(r.lr, 0x2000_0003);
    }

    #[test]
    fn should_execute_b_n_minus_20() {
        let r = run16_at(0x2000_0012, opcode_bt2(0xfec));
        assert_eq!(r.pc, 0x2000_0002);
    }

    #[test]
    fn should_execute_bne_n_minus_6() {
        let cpu = TestDriver::new();
        cpu.set_pc(0x2000_0012);
        cpu.set_flags(false, false, false, false);
        cpu.write_u16(0x2000_0012, opcode_bt1(1, 0x1f8));
        cpu.single_step();
        assert_eq!(cpu.read_registers().pc, 0x2000_000e);
    }

    #[test]
    fn should_execute_bx_lr() {
        let cpu = driver();
        cpu.set_registers(&[(14, 0x1000_0200)]);
        cpu.write_u16(0x2000_0000, opcode_bx(14));
        cpu.single_step();
        assert_eq!(cpu.read_registers().pc, 0x1000_0200);
    }

    #[test]
    fn should_execute_cmn_r7_r2() {
        let cpu = driver();
        cpu.set_registers(&[(2, 1), (7, (-2i32) as u32)]);
        cpu.write_u16(0x2000_0000, opcode_cmn(7, 2));
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r2, 1);
        assert_eq!(r.r7, (-2i32) as u32);
        assert!(r.n && !r.z && !r.c && !r.v);
    }

    #[test]
    fn should_execute_cmp_r5_66() {
        let cpu = driver();
        cpu.set_registers(&[(5, 60)]);
        cpu.write_u16(0x2000_0000, opcode_cmp_imm(5, 66));
        cpu.single_step();
        let r = cpu.read_registers();
        assert!(r.n && !r.z && !r.c && !r.v);
    }

    #[test]
    fn should_set_carry_for_cmp_r0_zero() {
        let cpu = driver();
        cpu.set_registers(&[(0, 0x8001_0133)]);
        cpu.write_u16(0x2000_0000, opcode_cmp_imm(0, 0));
        cpu.single_step();
        let r = cpu.read_registers();
        assert!(r.n && !r.z && r.c && !r.v);
    }

    #[test]
    fn should_execute_cmp_r5_r0() {
        let cpu = driver();
        cpu.set_registers(&[(5, 60), (0, 56)]);
        cpu.write_u16(0x2000_0000, opcode_cmp_reg_t1(5, 0));
        cpu.single_step();
        let r = cpu.read_registers();
        assert!(!r.n && !r.z && r.c && !r.v);
    }

    #[test]
    fn should_not_set_flags_for_cmp_r2_r0_large_values() {
        let cpu = driver();
        cpu.set_registers(&[(0, 0xb71b_0000), (2, 0x00b7_1b00)]);
        cpu.write_u16(0x2000_0000, opcode_cmp_reg_t1(2, 0));
        cpu.single_step();
        let r = cpu.read_registers();
        assert!(!r.n && !r.z && !r.c && !r.v);
    }

    #[test]
    fn should_set_carry_for_cmp_r11_r3() {
        let cpu = driver();
        cpu.set_registers(&[(3, 8), (11, 0xffff_ffff)]);
        cpu.write_u16(0x2000_0000, opcode_cmp_reg_t2(11, 3));
        cpu.single_step();
        let r = cpu.read_registers();
        assert!(r.n && !r.z && r.c && !r.v);
    }

    #[test]
    fn should_execute_cmp_ip_r6() {
        let cpu = driver();
        cpu.set_registers(&[(6, 56), (12, 60)]);
        cpu.write_u16(0x2000_0000, opcode_cmp_reg_t2(12, 6));
        cpu.single_step();
        let r = cpu.read_registers();
        assert!(!r.n && !r.z && r.c && !r.v);
    }

    #[test]
    fn should_set_nc_for_cmp_r11_r3_zero_and_min() {
        let cpu = driver();
        cpu.set_registers(&[(3, 0), (11, 0x8000_0000)]);
        cpu.write_u16(0x2000_0000, opcode_cmp_reg_t2(11, 3));
        cpu.single_step();
        let r = cpu.read_registers();
        assert!(r.n && !r.z && r.c && !r.v);
    }

    #[test]
    fn should_set_nv_for_cmp_r3_r7() {
        let cpu = driver();
        cpu.set_registers(&[(3, 0), (7, 0x8000_0000)]);
        cpu.write_u16(0x2000_0000, opcode_cmp_reg_t1(3, 7));
        cpu.single_step();
        let r = cpu.read_registers();
        assert!(r.n && !r.z && !r.c && r.v);
    }

    #[test]
    fn should_set_nv_for_cmp_r11_r3_min_and_zero() {
        let cpu = driver();
        cpu.set_registers(&[(3, 0x8000_0000), (11, 0)]);
        cpu.write_u16(0x2000_0000, opcode_cmp_reg_t2(11, 3));
        cpu.single_step();
        let r = cpu.read_registers();
        assert!(r.n && !r.z && !r.c && r.v);
    }

    #[test]
    fn should_set_nc_for_cmp_r3_r7_min_and_zero() {
        let cpu = driver();
        cpu.set_registers(&[(3, 0x8000_0000), (7, 0)]);
        cpu.write_u16(0x2000_0000, opcode_cmp_reg_t1(3, 7));
        cpu.single_step();
        let r = cpu.read_registers();
        assert!(r.n && !r.z && r.c && !r.v);
    }

    #[test]
    fn should_decode_dmb_sy() {
        assert_eq!(run32_at(0x2000_0000, opcode_dmbsy()).pc, 0x2000_0004);
    }

    #[test]
    fn should_decode_dsb_sy() {
        assert_eq!(run32_at(0x2000_0000, opcode_dsbsy()).pc, 0x2000_0004);
    }

    #[test]
    fn should_execute_eors_r1_r3() {
        let cpu = driver();
        cpu.set_registers(&[(1, 0xf0f0_f0f0), (3, 0x08ff_3007)]);
        cpu.write_u16(0x2000_0000, opcode_eors(1, 3));
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r1, 0xf80f_c0f7);
        assert!(r.n && !r.z);
    }

    #[test]
    fn should_decode_isb_sy() {
        assert_eq!(run32_at(0x2000_0000, opcode_isbsy()).pc, 0x2000_0004);
    }

    #[test]
    fn should_execute_mov_r3_r8() {
        let cpu = driver();
        cpu.set_registers(&[(8, 55)]);
        cpu.write_u16(0x2000_0000, opcode_mov(3, 8));
        cpu.single_step();
        assert_eq!(cpu.read_registers().r3, 55);
    }

    #[test]
    fn should_execute_mov_r3_pc() {
        let r = run16_at(0x2000_0000, opcode_mov(3, 15));
        assert_eq!(r.r3, 0x2000_0004);
    }

    #[test]
    fn should_execute_mov_sp_r8_case() {
        let cpu = driver();
        cpu.set_registers(&[(8, 55)]);
        cpu.write_u16(0x2000_0000, opcode_mov(3, 8));
        cpu.single_step();
        assert_eq!(cpu.read_registers().r3, 55);
    }

    #[test]
    fn should_execute_muls_r0_r2() {
        let cpu = driver();
        cpu.set_registers(&[(0, 5), (2, 1_000_000)]);
        cpu.write_u16(0x2000_0000, opcode_muls(0, 2));
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r2, 5_000_000);
        assert!(!r.n && !r.z);
    }

    #[test]
    fn should_execute_muls_large_values() {
        let cpu = driver();
        cpu.set_registers(&[(0, 2_654_435_769), (2, 340_573_321)]);
        cpu.write_u16(0x2000_0000, opcode_muls(0, 2));
        cpu.single_step();
        assert_eq!(cpu.read_registers().r2, 1);
    }

    #[test]
    fn should_set_z_for_muls_zero() {
        let cpu = driver();
        cpu.set_registers(&[(0, 0), (2, 1_000_000)]);
        cpu.write_u16(0x2000_0000, opcode_muls(0, 2));
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r2, 0);
        assert!(!r.n && r.z);
    }

    #[test]
    fn should_set_n_for_muls_negative() {
        let cpu = driver();
        cpu.set_registers(&[(0, (-1i32) as u32), (2, 1_000_000)]);
        cpu.write_u16(0x2000_0000, opcode_muls(0, 2));
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r2, (-1_000_000i32) as u32);
        assert!(r.n && !r.z);
    }

    #[test]
    fn should_execute_mvns_r4_r3() {
        let cpu = driver();
        cpu.set_registers(&[(3, 0x1111_5555)]);
        cpu.write_u16(0x2000_0000, opcode_mvns(4, 3));
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r4, 0xeeee_aaaa);
        assert!(!r.z && r.n);
    }

    #[test]
    fn should_execute_nop() {
        assert_eq!(run16_at(0x2000_0000, 0xbf00).pc, 0x2000_0002);
    }

    #[test]
    fn should_execute_orrs_r5_r0() {
        let cpu = driver();
        cpu.set_registers(&[(5, 0xf00f_0000), (0, 0xf000_ffff)]);
        cpu.write_u16(0x2000_0000, opcode_orrs(5, 0));
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r5, 0xf00f_ffff);
        assert!(r.n && !r.z);
    }

    #[test]
    fn should_execute_pop_pc_r4_r5_r6() {
        let cpu = driver();
        cpu.set_registers(&[(13, 0x2000_00f0)]);
        cpu.write_u16(
            0x2000_0000,
            opcode_pop(true, (1 << 4) | (1 << 5) | (1 << 6)),
        );
        cpu.write_u32(0x2000_00f0, 0x40);
        cpu.write_u32(0x2000_00f4, 0x50);
        cpu.write_u32(0x2000_00f8, 0x60);
        cpu.write_u32(0x2000_00fc, 0x42);
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.sp, 0x2000_0100);
        assert_eq!((r.r4, r.r5, r.r6, r.pc), (0x40, 0x50, 0x60, 0x42));
    }

    #[test]
    fn should_execute_push_r4_r5_r6_lr() {
        let cpu = driver();
        cpu.set_registers(&[
            (13, 0x2000_0100),
            (4, 0x40),
            (5, 0x50),
            (6, 0x60),
            (14, 0x42),
        ]);
        cpu.write_u16(
            0x2000_0000,
            opcode_push(true, (1 << 4) | (1 << 5) | (1 << 6)),
        );
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.sp, 0x2000_00f0);
        assert_eq!(cpu.read_u8(0x2000_00f0), 0x40);
        assert_eq!(cpu.read_u8(0x2000_00f4), 0x50);
        assert_eq!(cpu.read_u8(0x2000_00f8), 0x60);
        assert_eq!(cpu.read_u8(0x2000_00fc), 0x42);
    }

    #[test]
    fn should_execute_mrs_r0_ipsr() {
        let cpu = driver();
        cpu.set_registers(&[(0, 55)]);
        cpu.write_u32(0x2000_0000, opcode_mrs(0, 5));
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r0, 0);
        assert_eq!(r.pc, 0x2000_0004);
    }

    #[test]
    fn should_execute_msr_msp_r0() {
        let cpu = driver();
        cpu.set_registers(&[(0, 0x1234)]);
        cpu.write_u32(0x2000_0000, opcode_msr(8, 0));
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.sp, 0x1234);
        assert_eq!(r.pc, 0x2000_0004);
    }

    #[test]
    fn should_execute_movs_r5_128() {
        let r = run16_at(0x2000_0000, opcode_movs(5, 128));
        assert_eq!((r.r5, r.pc), (128, 0x2000_0002));
    }

    #[test]
    fn should_execute_ldmia_r0_r1_r2() {
        let cpu = driver();
        cpu.set_registers(&[(0, 0x2000_0010)]);
        cpu.write_u16(0x2000_0000, opcode_ldmia(0, (1 << 1) | (1 << 2)));
        cpu.write_u32(0x2000_0010, 0xf00d_f00d);
        cpu.write_u32(0x2000_0014, 0x4242);
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(
            (r.pc, r.r0, r.r1, r.r2),
            (0x2000_0002, 0x2000_0018, 0xf00d_f00d, 0x4242)
        );
    }

    #[test]
    fn should_execute_ldmia_r5_without_writeback() {
        let cpu = driver();
        cpu.set_registers(&[(5, 0x2000_0010)]);
        cpu.write_u16(0x2000_0000, opcode_ldmia(5, 1 << 5));
        cpu.write_u32(0x2000_0010, 0xf00d_f00d);
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!((r.pc, r.r5), (0x2000_0002, 0xf00d_f00d));
    }

    #[test]
    fn should_execute_ldr_literal() {
        let cpu = driver();
        cpu.write_u16(0x2000_0000, opcode_ldr_lit(0, 148));
        cpu.write_u32(0x2000_0098, 0x42);
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!((r.r0, r.pc), (0x42, 0x2000_0002));
    }

    #[test]
    fn should_execute_ldr_immediate() {
        let cpu = driver();
        cpu.set_registers(&[(2, 0x2000_0000)]);
        cpu.write_u16(0x2000_0000, opcode_ldr_imm(3, 2, 24));
        cpu.write_u32(0x2000_0018, 0x55);
        cpu.single_step();
        assert_eq!(cpu.read_registers().r3, 0x55);
    }

    #[test]
    fn should_execute_ldr_sp_immediate() {
        let cpu = driver();
        cpu.set_registers(&[(13, 0x2000_0000)]);
        cpu.write_u16(0x2000_0000, opcode_ldr_sp(3, 12));
        cpu.write_u32(0x2000_000c, 0x55);
        cpu.single_step();
        assert_eq!(cpu.read_registers().r3, 0x55);
    }

    #[test]
    fn should_execute_ldr_register() {
        let cpu = driver();
        cpu.set_registers(&[(5, 0x2000_0000), (6, 8)]);
        cpu.write_u16(0x2000_0000, opcode_ldr_reg(3, 5, 6));
        cpu.write_u32(0x2000_0008, 0xff55_4211);
        cpu.single_step();
        assert_eq!(cpu.read_registers().r3, 0xff55_4211);
    }

    #[test]
    fn should_execute_ldrb_immediate() {
        let cpu = driver();
        cpu.set_registers(&[(2, 0x2000_0000)]);
        cpu.write_u16(0x2000_0000, opcode_ldrb(4, 2, 5));
        cpu.write_u16(0x2000_0005, 0x7766);
        cpu.single_step();
        assert_eq!(cpu.read_registers().r4, 0x66);
    }

    #[test]
    fn should_execute_ldrb_register() {
        let cpu = driver();
        cpu.set_registers(&[(5, 0x2000_0000), (6, 8)]);
        cpu.write_u16(0x2000_0000, opcode_ldrb_reg(3, 5, 6));
        cpu.write_u32(0x2000_0008, 0xff55_4211);
        cpu.single_step();
        assert_eq!(cpu.read_registers().r3, 0x11);
    }

    #[test]
    fn should_execute_ldrh_immediate_4() {
        let cpu = driver();
        cpu.set_registers(&[(7, 0x2000_0000)]);
        cpu.write_u16(0x2000_0000, opcode_ldrh(3, 7, 4));
        cpu.write_u32(0x2000_0004, 0xffff_7766);
        cpu.single_step();
        assert_eq!(cpu.read_registers().r3, 0x7766);
    }

    #[test]
    fn should_execute_ldrh_immediate_6() {
        let cpu = driver();
        cpu.set_registers(&[(7, 0x2000_0000)]);
        cpu.write_u16(0x2000_0000, opcode_ldrh(3, 7, 6));
        cpu.write_u32(0x2000_0004, 0x3344_7766);
        cpu.single_step();
        assert_eq!(cpu.read_registers().r3, 0x3344);
    }

    #[test]
    fn should_execute_ldrh_register() {
        let cpu = driver();
        cpu.set_registers(&[(5, 0x2000_0000), (6, 8)]);
        cpu.write_u16(0x2000_0000, opcode_ldrh_reg(3, 5, 6));
        cpu.write_u32(0x2000_0008, 0xff55_4211);
        cpu.single_step();
        assert_eq!(cpu.read_registers().r3, 0x4211);
    }

    #[test]
    fn should_execute_ldrsb() {
        let cpu = driver();
        cpu.set_registers(&[(3, 0x2000_0000), (5, 6)]);
        cpu.write_u16(0x2000_0000, opcode_ldrsb(5, 3, 5));
        cpu.write_u32(0x2000_0006, 0x85);
        cpu.single_step();
        assert_eq!(cpu.read_registers().r5, 0xffff_ff85);
    }

    #[test]
    fn should_execute_ldrsh() {
        let cpu = driver();
        cpu.set_registers(&[(3, 0x2000_0000), (5, 6)]);
        cpu.write_u16(0x2000_0000, opcode_ldrsh(5, 3, 5));
        cpu.write_u16(0x2000_0006, 0xf055);
        cpu.single_step();
        assert_eq!(cpu.read_registers().r5, 0xffff_f055);
    }

    #[test]
    fn should_execute_udf_1() {
        let rp = crate::rp2040::Rp2040::new();
        let breaks = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let recorded = breaks.clone();
        rp.set_on_break(Box::new(move |code| recorded.borrow_mut().push(code)));
        rp.core.set_pc(0x2000_0000);
        rp.write_u16(0x2000_0000, opcode_udf(1));
        rp.step();
        assert_eq!(rp.core.pc(), 0x2000_0002);
        assert_eq!(*breaks.borrow(), vec![1]);
    }

    #[test]
    fn should_execute_udf_w_zero() {
        let rp = crate::rp2040::Rp2040::new();
        let breaks = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let recorded = breaks.clone();
        rp.set_on_break(Box::new(move |code| recorded.borrow_mut().push(code)));
        rp.core.set_pc(0x2000_0000);
        rp.write_u32(0x2000_0000, opcode_udf2(0));
        rp.step();
        assert_eq!(rp.core.pc(), 0x2000_0004);
        assert_eq!(*breaks.borrow(), vec![0]);
    }

    #[test]
    fn should_execute_lsls_r5_r5_18() {
        let cpu = driver();
        cpu.set_registers(&[(5, 0x1234_5678)]);
        cpu.write_u16(0x2000_0000, opcode_lsls_imm(5, 5, 18));
        cpu.single_step();
        let r = cpu.read_registers();
        assert_eq!(r.r5, 0x1234_5678u32.wrapping_shl(18));
        assert_eq!(r.pc, 0x2000_0002);
    }
}
