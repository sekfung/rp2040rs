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
            if shift != 0 {
                let (result, carry) = self.arithmetic_shift_right(self.reg(rdn), shift);
                self.set_reg(rdn, result);
                self.set_nz(result);
                self.set_c(carry);
            } else {
                self.set_nz(self.reg(rdn));
            }
        } else if opcode >> 6 == 0b0100001110 {
            // BICS Rdn, Rm
            let rdn = (opcode & 0x7) as usize;
            let rm = ((opcode >> 3) & 0x7) as usize;
            let result = self.reg(rdn) & !self.reg(rm);
            self.set_reg(rdn, result);
            self.set_nz(result);
        } else if opcode >> 11 == 0b00100 {
            // MOVS Rd, #imm8
            let value = u32::from(opcode & 0xff);
            let rd = ((opcode >> 8) & 0x7) as usize;
            self.state.borrow_mut().registers[rd] = value;
            self.set_n(value & 0x8000_0000 != 0);
            self.set_z(value == 0);
        } else if opcode >> 7 == 0x8e && opcode & 0x7 == 0 {
            // BX Rm
            let rm = ((opcode >> 3) & 0xf) as usize;
            let address = self.reg(rm);
            self.bx_write_pc(address);
            delta_cycles += 1;
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
    use crate::test_support::TestDriver;
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
}
