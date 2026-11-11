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
        let mut state = self.state.borrow_mut();
        state.registers[13] = bus.read_u32(state.vtor) & !0x3;
        state.registers[15] = bus.read_u32(state.vtor.wrapping_add(4)) & 0xffff_fffe;
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
        let mut state = self.state.borrow_mut();
        match state.spsel {
            StackPointerBank::Main => state.registers[13] = value & !0x3,
            StackPointerBank::Process => state.banked_sp = value & !0x3,
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
        let mut state = self.state.borrow_mut();
        if value && state.pending_interrupts & bit == 0 {
            state.pending_interrupts |= bit;
            state.interrupts_updated = true;
        } else if !value {
            state.pending_interrupts &= !bit;
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

        if opcode >> 11 == 0b00100 {
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
        let (pending, enabled, priorities, ipsr) = {
            let state = self.state.borrow();
            (
                state.pending_interrupts,
                state.enabled_interrupts,
                state.interrupt_priorities,
                state.ipsr,
            )
        };
        let current_priority = if ipsr < 16 { LOWEST_PRIORITY } else { 4 };
        let interrupt_set = pending & enabled;
        for priority in 0..current_priority {
            let level = interrupt_set & priorities[priority as usize];
            if level != 0 {
                for n in 0..32 {
                    if level & (1 << n) != 0 {
                        if n > crate::irq::MAX_HARDWARE_IRQ {
                            self.state.borrow_mut().pending_interrupts &= !(1 << n);
                        }
                        self.exception_entry(16 + n);
                        self.state.borrow_mut().interrupts_updated = false;
                        return true;
                    }
                }
            }
        }
        self.state.borrow_mut().interrupts_updated = false;
        false
    }

    fn exception_entry(&self, exception_number: u32) {
        let Some(bus) = self.bus() else {
            return;
        };
        let frame_align = u32::from(self.sp() & 0b100 != 0);
        let frame_ptr = self.sp().wrapping_sub(0x20) & !0b100;
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
        state.registers[13] = frame_ptr;
        state.registers[14] = if state.current_mode == ExecutionMode::Handler {
            0xffff_fff1
        } else if state.spsel == StackPointerBank::Main {
            0xffff_fff9
        } else {
            0xffff_fffd
        };
        state.current_mode = ExecutionMode::Handler;
        state.ipsr = exception_number;
        state.spsel = StackPointerBank::Main;
        let vector = state.vtor;
        drop(state);
        self.set_pc(bus.read_u32(vector.wrapping_add(4 * exception_number)));
    }

    fn exception_return(&self, exc_return: u32) {
        let Some(bus) = self.bus() else {
            return;
        };
        let frame_ptr = self.sp();
        self.set_reg(0, bus.read_u32(frame_ptr));
        self.set_reg(1, bus.read_u32(frame_ptr.wrapping_add(4)));
        self.set_reg(2, bus.read_u32(frame_ptr.wrapping_add(8)));
        self.set_reg(3, bus.read_u32(frame_ptr.wrapping_add(12)));
        self.set_reg(12, bus.read_u32(frame_ptr.wrapping_add(16)));
        self.set_lr(bus.read_u32(frame_ptr.wrapping_add(20)));
        self.set_pc(bus.read_u32(frame_ptr.wrapping_add(24)));
        let psr = bus.read_u32(frame_ptr.wrapping_add(28));
        let frame_align = if psr & (1 << 9) != 0 { 0b100 } else { 0 };
        let mut state = self.state.borrow_mut();
        match exc_return & 0xf {
            0b0001 => {
                state.current_mode = ExecutionMode::Handler;
                state.registers[13] = (state.registers[13].wrapping_add(0x20)) | frame_align;
            }
            0b1001 => {
                state.current_mode = ExecutionMode::Thread;
                state.registers[13] = (state.registers[13].wrapping_add(0x20)) | frame_align;
            }
            0b1101 => {
                state.current_mode = ExecutionMode::Thread;
                state.registers[13] = (state.banked_sp.wrapping_add(0x20)) | frame_align;
            }
            _ => {}
        }
        state.n = psr & 0x8000_0000 != 0;
        state.z = psr & 0x4000_0000 != 0;
        state.c = psr & 0x2000_0000 != 0;
        state.v = psr & 0x1000_0000 != 0;
        state.ipsr = psr & 0x3f;
        state.interrupts_updated = true;
    }

    pub fn read_special_register(&self, sysm: u32) -> u32 {
        match sysm {
            SYSM_APSR => self.apsr(),
            SYSM_XPSR => self.xpsr(),
            SYSM_IPSR => self.ipsr(),
            SYSM_MSP => self.state.borrow().registers[13],
            SYSM_PSP => self.state.borrow().banked_sp,
            SYSM_PRIMASK => u32::from(self.state.borrow().pm),
            SYSM_CONTROL => {
                let state = self.state.borrow();
                (u32::from(state.spsel == StackPointerBank::Process))
                    | (u32::from(state.npriv) << 1)
            }
            _ => 0,
        }
    }

    pub fn write_special_register(&self, sysm: u32, value: u32) {
        match sysm {
            SYSM_APSR => self.set_apsr(value),
            SYSM_XPSR => self.set_xpsr(value),
            SYSM_IPSR => self.set_ipsr(value),
            SYSM_MSP => self.state.borrow_mut().registers[13] = value & !0x3,
            SYSM_PSP => self.state.borrow_mut().banked_sp = value & !0x3,
            SYSM_PRIMASK => self.state.borrow_mut().pm = value & 1 != 0,
            SYSM_CONTROL => {
                let mut state = self.state.borrow_mut();
                state.spsel = if value & 1 != 0 {
                    StackPointerBank::Process
                } else {
                    StackPointerBank::Main
                };
                state.npriv = value & 2 != 0;
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
