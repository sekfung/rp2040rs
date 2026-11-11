use std::cell::RefCell;
use std::rc::Rc;

use crate::cortex_m0_core::Core;
use crate::irq::MAX_HARDWARE_IRQ;
use crate::peripherals::peripheral::{atomic_update, Peripheral};

const CPUID: u32 = 0xd00;
const ICSR: u32 = 0xd04;
const VTOR: u32 = 0xd08;
const SHPR2: u32 = 0xd1c;
const SHPR3: u32 = 0xd20;

const SYST_CSR: u32 = 0x010;
const SYST_RVR: u32 = 0x014;
const SYST_CVR: u32 = 0x018;
const SYST_CALIB: u32 = 0x01c;

const NVIC_ISER: u32 = 0x100;
const NVIC_ICER: u32 = 0x180;
const NVIC_ISPR: u32 = 0x200;
const NVIC_ICPR: u32 = 0x280;
const NVIC_IPR0: u32 = 0x400;

const NMIPENDSET: u32 = 1 << 31;
const PENDSVSET: u32 = 1 << 28;
const PENDSVCLR: u32 = 1 << 27;
const PENDSTSET: u32 = 1 << 26;
const PENDSTCLR: u32 = 1 << 25;
const ISRPENDING: u32 = 1 << 22;
const VECTPENDING_SHIFT: u32 = 12;
const VECTACTIVE_MASK: u32 = 0x1ff;
const VECTPENDING_MASK: u32 = 0x1ff;

struct PpbState {
    systick_csr: u32,
    systick_rvr: u32,
    systick_cvr: u32,
    systick_count_flag: bool,
}

#[derive(Clone)]
pub struct Ppb {
    core: Core,
    name: String,
    state: Rc<RefCell<PpbState>>,
}

impl Ppb {
    pub fn new(core: Core) -> Self {
        Self {
            core,
            name: "PPB".to_string(),
            state: Rc::new(RefCell::new(PpbState {
                systick_csr: 0,
                systick_rvr: 0x00ff_ffff,
                systick_cvr: 0x00ff_ffff,
                systick_count_flag: false,
            })),
        }
    }

    pub fn reset(&self) {
        let mut state = self.state.borrow_mut();
        state.systick_csr = 0;
        state.systick_rvr = 0x00ff_ffff;
        state.systick_cvr = 0x00ff_ffff;
        state.systick_count_flag = false;
    }

    fn write_register(&self, offset: u32, value: u32) {
        match offset {
            ICSR => {
                if value & NMIPENDSET != 0 {
                    self.core.set_pending_nmi(true);
                    self.core.set_interrupts_updated(true);
                }
                if value & PENDSVSET != 0 {
                    self.core.set_pending_pendsv(true);
                    self.core.set_interrupts_updated(true);
                }
                if value & PENDSVCLR != 0 {
                    self.core.set_pending_pendsv(false);
                }
                if value & PENDSTSET != 0 {
                    self.core.set_pending_systick(true);
                    self.core.set_interrupts_updated(true);
                }
                if value & PENDSTCLR != 0 {
                    self.core.set_pending_systick(false);
                }
            }
            VTOR => self.core.set_vtor(value),
            NVIC_ISPR => {
                self.core
                    .set_pending_interrupts(self.core.pending_interrupts() | value);
                self.core.set_interrupts_updated(true);
            }
            NVIC_ICPR => {
                let mask = (1u32 << MAX_HARDWARE_IRQ) - 1;
                self.core
                    .set_pending_interrupts(self.core.pending_interrupts() & (!value | mask));
            }
            NVIC_ISER => {
                self.core
                    .set_enabled_interrupts(self.core.enabled_interrupts() | value);
                self.core.set_interrupts_updated(true);
            }
            NVIC_ICER => {
                self.core
                    .set_enabled_interrupts(self.core.enabled_interrupts() & !value);
            }
            NVIC_IPR0..=0x41c if (offset - NVIC_IPR0).is_multiple_of(4) => {
                let reg_index = (offset - NVIC_IPR0) >> 2;
                let mut priorities = self.core.interrupt_priorities();
                for byte_index in 0..4 {
                    let interrupt_number = reg_index * 4 + byte_index;
                    let new_priority = (value >> (8 * byte_index + 6)) & 0x3;
                    for priority in &mut priorities {
                        *priority &= !(1 << interrupt_number);
                    }
                    priorities[new_priority as usize] |= 1 << interrupt_number;
                }
                self.core.set_interrupt_priorities(priorities);
                self.core.set_interrupts_updated(true);
            }
            SHPR2 => self.core.set_shpr2(value),
            SHPR3 => self.core.set_shpr3(value),
            SYST_CSR => self.state.borrow_mut().systick_csr = value & 0x7,
            SYST_CVR => self.state.borrow_mut().systick_cvr = 0,
            SYST_RVR => self.state.borrow_mut().systick_rvr = value & 0x00ff_ffff,
            _ => {}
        }
    }

    fn read_register(&self, offset: u32) -> u32 {
        match offset {
            CPUID => 0x410c_c601,
            ICSR => {
                let pending = self.core.pending_interrupts() != 0
                    || self.core.pending_pendsv()
                    || self.core.pending_systick()
                    || self.core.pending_svc();
                let vect_pending = self.core.vect_pending();
                (if self.core.pending_nmi() {
                    NMIPENDSET
                } else {
                    0
                }) | (if self.core.pending_pendsv() {
                    PENDSVSET
                } else {
                    0
                }) | (if self.core.pending_systick() {
                    PENDSTSET
                } else {
                    0
                }) | (if pending { ISRPENDING } else { 0 })
                    | ((vect_pending & VECTPENDING_MASK) << VECTPENDING_SHIFT)
                    | (self.core.ipsr() & VECTACTIVE_MASK)
            }
            VTOR => self.core.vtor(),
            NVIC_ISPR | NVIC_ICPR => self.core.pending_interrupts(),
            NVIC_ISER | NVIC_ICER => self.core.enabled_interrupts(),
            NVIC_IPR0..=0x41c if (offset - NVIC_IPR0).is_multiple_of(4) => {
                let reg_index = (offset - NVIC_IPR0) >> 2;
                let priorities = self.core.interrupt_priorities();
                let mut result: u32 = 0;
                for byte_index in 0..4 {
                    let interrupt_number = reg_index * 4 + byte_index;
                    for priority in 0u32..4 {
                        if priorities[priority as usize] & (1 << interrupt_number) != 0 {
                            result |= priority << (8 * byte_index + 6);
                        }
                    }
                }
                result
            }
            SHPR2 => self.core.shpr2(),
            SHPR3 => self.core.shpr3(),
            SYST_CSR => {
                let mut state = self.state.borrow_mut();
                let count_flag = if state.systick_count_flag {
                    state.systick_count_flag = false;
                    1 << 16
                } else {
                    0
                };
                count_flag | (state.systick_csr & 0x7)
            }
            SYST_CVR => self.state.borrow().systick_cvr,
            SYST_RVR => self.state.borrow().systick_rvr,
            SYST_CALIB => 0x0000_270f,
            _ => 0xffff_ffff,
        }
    }
}

impl Peripheral for Ppb {
    fn name(&self) -> &str {
        &self.name
    }

    fn read_u32(&self, offset: u32) -> u32 {
        self.read_register(offset)
    }

    fn write_u32_atomic(&self, offset: u32, value: u32, atomic_type: u32) {
        let value = if atomic_type == 0 {
            value
        } else {
            atomic_update(self.read_register(offset), atomic_type, value)
        };
        self.write_register(offset, value);
    }
}
