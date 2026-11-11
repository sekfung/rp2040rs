use std::cell::Cell;
use std::rc::Rc;

use rp2040rs::cortex_m0_core::{StackPointerBank, SYSM_CONTROL, SYSM_PRIMASK};
use rp2040rs::utils::assembler::{opcode_bx, opcode_nop, opcode_udf};
use rp2040rs::{BasePeripheral, Logger, Rp2040};

const VTOR: u32 = 0xe000_ed08;
const ICSR: u32 = 0xe000_ed04;
const SYST_CSR: u32 = 0xe000_e010;
const SYST_RVR: u32 = 0xe000_e014;
const SYST_CVR: u32 = 0xe000_e018;
const PENDSVSET: u32 = 1 << 28;
const PENDSVCLR: u32 = 1 << 27;

#[test]
fn control_uses_bit0_for_npriv_and_bit1_for_spsel() {
    let rp = Rp2040::new();
    let core = &rp.core;

    core.write_special_register(SYSM_CONTROL, 1);
    assert!(core.npriv());
    assert_eq!(core.spsel(), StackPointerBank::Main);
    assert_eq!(core.read_special_register(SYSM_CONTROL), 1);

    core.write_special_register(SYSM_CONTROL, 2);
    assert!(!core.npriv());
    assert_eq!(core.spsel(), StackPointerBank::Process);
    assert_eq!(core.read_special_register(SYSM_CONTROL), 2);
}

#[test]
fn primask_blocks_a_pending_interrupt() {
    let rp = Rp2040::new();
    let core = &rp.core;
    rp.write_u16(0x1000_0000, opcode_nop());
    core.set_pc(0x1000_0000);
    core.set_pending_interrupts(1 << 1);
    core.set_enabled_interrupts(1 << 1);
    core.set_interrupts_updated(true);
    core.write_special_register(SYSM_PRIMASK, 1);

    core.step();

    assert_eq!(core.ipsr(), 0);
    assert_eq!(core.pc(), 0x1000_0002);
}

#[test]
fn process_stack_is_used_for_thread_exception_entry_and_return() {
    let rp = Rp2040::new();
    let core = &rp.core;
    core.set_msp(0x2000_4000);
    core.set_psp(0x2000_5000);
    core.write_special_register(SYSM_CONTROL, 2);
    core.set_pc(0x1000_4000);
    core.set_pending_interrupts(1 << 1);
    core.set_enabled_interrupts(1 << 1);
    core.set_interrupts_updated(true);

    rp.write_u32(VTOR, 0x1000_0000);
    rp.write_u32(0x1000_0000 + 17 * 4, 0x1000_0100);
    rp.write_u16(0x1000_0100, opcode_nop());
    rp.write_u16(0x1000_0102, opcode_bx(14));

    core.step(); // enter IRQ1, execute NOP
    assert_eq!(core.ipsr(), 17);
    assert_eq!(core.psp(), 0x2000_4fe0);
    assert_eq!(core.msp(), 0x2000_4000);

    core.step(); // BX LR
    assert_eq!(core.ipsr(), 0);
    assert_eq!(core.psp(), 0x2000_5000);
    assert_eq!(core.msp(), 0x2000_4000);
}

#[test]
fn ppb_systick_and_icsr_registers_keep_state() {
    let rp = Rp2040::new();

    rp.write_u32(SYST_RVR, 0x1234);
    assert_eq!(rp.read_u32(SYST_RVR), 0x1234);

    rp.write_u32(SYST_CVR, 0x55);
    assert_eq!(rp.read_u32(SYST_CVR), 0);

    rp.write_u32(SYST_CSR, 0x7);
    assert_eq!(rp.read_u32(SYST_CSR), 0x7);

    rp.write_u32(ICSR, PENDSVSET);
    assert_eq!(rp.read_u32(ICSR) & PENDSVSET, PENDSVSET);
    rp.write_u32(ICSR, PENDSVCLR);
    assert_eq!(rp.read_u32(ICSR) & PENDSVSET, 0);
}

#[test]
fn nmi_is_taken_even_when_primask_is_set() {
    let rp = Rp2040::new();
    let core = &rp.core;
    rp.write_u16(0x1000_0100, opcode_nop());
    rp.write_u32(VTOR, 0x1000_0000);
    rp.write_u32(0x1000_0000 + 2 * 4, 0x1000_0100);
    core.set_pc(0x1000_4000);
    core.write_special_register(SYSM_PRIMASK, 1);
    rp.write_u32(ICSR, 1 << 31);

    core.step();

    assert_eq!(core.ipsr(), 2);
    assert_eq!(core.pc(), 0x1000_0102);
}

#[test]
fn out_of_range_ipsr_does_not_panic() {
    let rp = Rp2040::new();
    rp.write_u16(0x1000_0000, opcode_nop());
    rp.core.set_pc(0x1000_0000);
    rp.core.set_ipsr(63);
    rp.core.set_pending_interrupts(1 << 1);
    rp.core.set_enabled_interrupts(1 << 1);
    rp.core.set_interrupts_updated(true);

    rp.core.step();
}

struct ReentrantLogger {
    rp: Rp2040,
    warnings: Rc<Cell<u32>>,
}

impl Logger for ReentrantLogger {
    fn debug(&self, _component: &str, _message: &str) {}
    fn info(&self, _component: &str, _message: &str) {}

    fn warn(&self, _component: &str, _message: &str) {
        self.warnings.set(self.warnings.get() + 1);
        self.rp
            .set_peripheral(0x10, Rc::new(BasePeripheral::new("LoggerTest")));
    }

    fn error(&self, _component: &str, _message: &str) {}
}

#[test]
fn logger_can_reenter_mutable_rp2040_api() {
    let rp = Rp2040::new();
    let warnings = Rc::new(Cell::new(0));
    rp.set_logger(Box::new(ReentrantLogger {
        rp: rp.clone(),
        warnings: warnings.clone(),
    }));

    rp.read_u32(0xf000_0000);
    assert_eq!(warnings.get(), 1);
}

#[test]
fn on_break_can_replace_itself_without_being_overwritten() {
    let rp = Rp2040::new();
    rp.write_u16(0x2000_0000, opcode_udf(1));
    rp.core.set_pc(0x2000_0000);

    let first = Rc::new(Cell::new(0));
    let second = Rc::new(Cell::new(0));
    let first_calls = first.clone();
    let second_calls = second.clone();
    let replacement = rp.clone();
    rp.set_on_break(Box::new(move |_| {
        first_calls.set(first_calls.get() + 1);
        let second_calls = second_calls.clone();
        replacement.set_on_break(Box::new(move |_| {
            second_calls.set(second_calls.get() + 1);
        }));
    }));

    rp.core.step();
    assert_eq!(first.get(), 1);

    rp.core.set_pc(0x2000_0000);
    rp.core.step();
    assert_eq!(second.get(), 1);
}
