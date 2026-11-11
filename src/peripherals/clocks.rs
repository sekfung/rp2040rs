use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::peripherals::peripheral::{atomic_update, Peripheral};
use crate::rp2040::BoardShared;

const CLK_GPOUT0_CTRL: u32 = 0x00;
const CLK_GPOUT0_DIV: u32 = 0x04;
const CLK_GPOUT0_SELECTED: u32 = 0x08;
const CLK_GPOUT1_CTRL: u32 = 0x0c;
const CLK_GPOUT1_DIV: u32 = 0x10;
const CLK_GPOUT1_SELECTED: u32 = 0x14;
const CLK_GPOUT2_CTRL: u32 = 0x18;
const CLK_GPOUT2_DIV: u32 = 0x1c;
const CLK_GPOUT2_SELECTED: u32 = 0x20;
const CLK_GPOUT3_CTRL: u32 = 0x24;
const CLK_GPOUT3_DIV: u32 = 0x28;
const CLK_GPOUT3_SELECTED: u32 = 0x2c;
const CLK_REF_CTRL: u32 = 0x30;
const CLK_REF_DIV: u32 = 0x34;
const CLK_REF_SELECTED: u32 = 0x38;
const CLK_SYS_CTRL: u32 = 0x3c;
const CLK_SYS_DIV: u32 = 0x40;
const CLK_SYS_SELECTED: u32 = 0x44;
const CLK_PERI_CTRL: u32 = 0x48;
const CLK_PERI_DIV: u32 = 0x4c;
const CLK_PERI_SELECTED: u32 = 0x50;
const CLK_USB_CTRL: u32 = 0x54;
const CLK_USB_DIV: u32 = 0x58;
const CLK_USB_SELECTED: u32 = 0x5c;
const CLK_ADC_CTRL: u32 = 0x60;
const CLK_ADC_DIV: u32 = 0x64;
const CLK_ADC_SELECTED: u32 = 0x68;
const CLK_RTC_CTRL: u32 = 0x6c;
const CLK_RTC_DIV: u32 = 0x70;
const CLK_RTC_SELECTED: u32 = 0x74;
const CLK_SYS_RESUS_CTRL: u32 = 0x78;
const CLK_SYS_RESUS_STATUS: u32 = 0x7c;

const REF_SRC_MASK: u32 = 0x3;
const REF_SRC_ROSC: u32 = 0;
const REF_SRC_XOSC: u32 = 2;
const REF_SRC_AUX: u32 = 1;
const REF_AUXSRC_SHIFT: u32 = 5;
const REF_AUXSRC_MASK: u32 = 0x3;
const REF_AUXSRC_PLL_USB: u32 = 0;
const REF_DIV_INT_BITS: u32 = 0x300;

const SYS_SRC_MASK: u32 = 0x1;
const SYS_SRC_REF: u32 = 0;
const SYS_AUXSRC_SHIFT: u32 = 5;
const SYS_AUXSRC_MASK: u32 = 0x7;
const SYS_AUXSRC_PLL_SYS: u32 = 0;
const SYS_AUXSRC_PLL_USB: u32 = 1;
const SYS_AUXSRC_ROSC: u32 = 2;
const SYS_AUXSRC_XOSC: u32 = 3;

const PERI_ENABLE: u32 = 1 << 11;
const PERI_KILL: u32 = 1 << 10;
const PERI_AUXSRC_SHIFT: u32 = 5;
const PERI_AUXSRC_MASK: u32 = 0x7;
const PERI_AUXSRC_CLK_SYS: u32 = 0;
const PERI_AUXSRC_PLL_SYS: u32 = 1;
const PERI_AUXSRC_PLL_USB: u32 = 2;
const PERI_AUXSRC_ROSC: u32 = 3;
const PERI_AUXSRC_XOSC: u32 = 4;

const DIV_INT_SHIFT: u32 = 8;
const DIV_FRAC_MASK: u32 = 0xff;

fn clock_divisor(div: u32) -> f64 {
    let int = div >> DIV_INT_SHIFT;
    if int == 0 {
        65536.0
    } else {
        f64::from(int) + f64::from(div & DIV_FRAC_MASK) / 256.0
    }
}

struct ClocksState {
    gpout_ctrl: [u32; 4],
    gpout_div: [u32; 4],
    ref_ctrl: u32,
    ref_div: u32,
    peri_ctrl: u32,
    peri_div: u32,
    usb_ctrl: u32,
    usb_div: u32,
    sys_ctrl: u32,
    sys_div: u32,
    adc_ctrl: u32,
    adc_div: u32,
    rtc_ctrl: u32,
    rtc_div: u32,
}

#[derive(Clone)]
pub struct Clocks {
    name: String,
    state: Rc<RefCell<ClocksState>>,
    board: Weak<RefCell<BoardShared>>,
}

impl Clocks {
    pub fn new(name: impl Into<String>, board: Weak<RefCell<BoardShared>>) -> Self {
        Self {
            name: name.into(),
            state: Rc::new(RefCell::new(ClocksState {
                gpout_ctrl: [0; 4],
                gpout_div: [0x100; 4],
                ref_ctrl: 0,
                ref_div: 0x100,
                peri_ctrl: 0,
                peri_div: 0x100,
                usb_ctrl: 0,
                usb_div: 0x100,
                sys_ctrl: 0,
                sys_div: 0x100,
                adc_ctrl: 0,
                adc_div: 0x100,
                rtc_ctrl: 0,
                rtc_div: 0x100,
            })),
            board,
        }
    }

    fn ref_source_freq(&self, xosc: f64, rosc: f64, pll_usb: f64) -> f64 {
        let state = self.state.borrow();
        match state.ref_ctrl & REF_SRC_MASK {
            REF_SRC_ROSC => rosc,
            REF_SRC_XOSC => xosc,
            REF_SRC_AUX => {
                let aux = (state.ref_ctrl >> REF_AUXSRC_SHIFT) & REF_AUXSRC_MASK;
                if aux == REF_AUXSRC_PLL_USB {
                    pll_usb
                } else {
                    0.0
                }
            }
            _ => 0.0,
        }
    }

    pub fn ref_freq(&self, xosc: f64, rosc: f64, pll_usb: f64) -> f64 {
        self.ref_source_freq(xosc, rosc, pll_usb)
            / clock_divisor(self.state.borrow().ref_div & REF_DIV_INT_BITS)
    }

    fn sys_source_freq(&self, xosc: f64, rosc: f64, pll_sys: f64, pll_usb: f64) -> f64 {
        let state = self.state.borrow();
        if state.sys_ctrl & SYS_SRC_MASK == SYS_SRC_REF {
            return self.ref_freq(xosc, rosc, pll_usb);
        }
        match (state.sys_ctrl >> SYS_AUXSRC_SHIFT) & SYS_AUXSRC_MASK {
            SYS_AUXSRC_PLL_SYS => pll_sys,
            SYS_AUXSRC_PLL_USB => pll_usb,
            SYS_AUXSRC_ROSC => rosc,
            SYS_AUXSRC_XOSC => xosc,
            _ => 0.0,
        }
    }

    pub fn sys_freq(&self, xosc: f64, rosc: f64, pll_sys: f64, pll_usb: f64) -> f64 {
        self.sys_source_freq(xosc, rosc, pll_sys, pll_usb)
            / clock_divisor(self.state.borrow().sys_div)
    }

    pub fn peri_freq(&self, xosc: f64, rosc: f64, pll_sys: f64, pll_usb: f64) -> f64 {
        let state = self.state.borrow();
        if state.peri_ctrl & PERI_ENABLE == 0 || state.peri_ctrl & PERI_KILL != 0 {
            return 0.0;
        }
        match (state.peri_ctrl >> PERI_AUXSRC_SHIFT) & PERI_AUXSRC_MASK {
            PERI_AUXSRC_CLK_SYS => self.sys_freq(xosc, rosc, pll_sys, pll_usb),
            PERI_AUXSRC_PLL_SYS => pll_sys,
            PERI_AUXSRC_PLL_USB => pll_usb,
            PERI_AUXSRC_ROSC => rosc,
            PERI_AUXSRC_XOSC => xosc,
            _ => 0.0,
        }
    }

    fn read_register(&self, offset: u32) -> u32 {
        let state = self.state.borrow();
        match offset {
            CLK_GPOUT0_CTRL => state.gpout_ctrl[0] & 0b100110001110111100000,
            CLK_GPOUT0_DIV => state.gpout_div[0],
            CLK_GPOUT0_SELECTED => 1,
            CLK_GPOUT1_CTRL => state.gpout_ctrl[1] & 0b100110001110111100000,
            CLK_GPOUT1_DIV => state.gpout_div[1],
            CLK_GPOUT1_SELECTED => 1,
            CLK_GPOUT2_CTRL => state.gpout_ctrl[2] & 0b100110001110111100000,
            CLK_GPOUT2_DIV => state.gpout_div[2],
            CLK_GPOUT2_SELECTED => 1,
            CLK_GPOUT3_CTRL => state.gpout_ctrl[3] & 0b100110001110111100000,
            CLK_GPOUT3_DIV => state.gpout_div[3],
            CLK_GPOUT3_SELECTED => 1,
            CLK_REF_CTRL => state.ref_ctrl & 0b000001100011,
            CLK_REF_DIV => state.ref_div & 0x30,
            CLK_REF_SELECTED => 1 << (state.ref_ctrl & 0x03),
            CLK_SYS_CTRL => state.sys_ctrl & 0b000011100001,
            CLK_SYS_DIV => state.sys_div,
            CLK_SYS_SELECTED => 1 << (state.sys_ctrl & 0x01),
            CLK_PERI_CTRL => state.peri_ctrl & 0b110011100000,
            CLK_PERI_DIV => state.peri_div,
            CLK_PERI_SELECTED => 1,
            CLK_USB_CTRL => state.usb_ctrl & 0b100110000110011100000,
            CLK_USB_DIV => state.usb_div,
            CLK_USB_SELECTED => 1,
            CLK_ADC_CTRL => state.adc_ctrl & 0b100110000110011100000,
            CLK_ADC_DIV => state.adc_div & 0x30,
            CLK_ADC_SELECTED => 1,
            CLK_RTC_CTRL => state.rtc_ctrl & 0b100110000110011100000,
            CLK_RTC_DIV => state.rtc_div & 0x30,
            CLK_RTC_SELECTED => 1,
            CLK_SYS_RESUS_CTRL => 0xff,
            CLK_SYS_RESUS_STATUS => 0,
            _ => 0xffff_ffff,
        }
    }

    fn write_register(&self, offset: u32, value: u32) {
        let mut update = false;
        {
            let mut state = self.state.borrow_mut();
            match offset {
                CLK_GPOUT0_CTRL => state.gpout_ctrl[0] = value,
                CLK_GPOUT0_DIV => state.gpout_div[0] = value,
                CLK_GPOUT1_CTRL => state.gpout_ctrl[1] = value,
                CLK_GPOUT1_DIV => state.gpout_div[1] = value,
                CLK_GPOUT2_CTRL => state.gpout_ctrl[2] = value,
                CLK_GPOUT2_DIV => state.gpout_div[2] = value,
                CLK_GPOUT3_CTRL => state.gpout_ctrl[3] = value,
                CLK_GPOUT3_DIV => state.gpout_div[3] = value,
                CLK_REF_CTRL => {
                    state.ref_ctrl = value;
                    update = true;
                }
                CLK_REF_DIV => {
                    state.ref_div = value;
                    update = true;
                }
                CLK_SYS_CTRL => {
                    state.sys_ctrl = value;
                    update = true;
                }
                CLK_SYS_DIV => {
                    state.sys_div = value;
                    update = true;
                }
                CLK_PERI_CTRL => {
                    state.peri_ctrl = value;
                    update = true;
                }
                CLK_PERI_DIV => state.peri_div = value,
                CLK_USB_CTRL => state.usb_ctrl = value,
                CLK_USB_DIV => state.usb_div = value,
                CLK_ADC_CTRL => state.adc_ctrl = value,
                CLK_ADC_DIV => state.adc_div = value,
                CLK_RTC_CTRL => state.rtc_ctrl = value,
                CLK_RTC_DIV => state.rtc_div = value,
                CLK_SYS_RESUS_CTRL => {}
                _ => {}
            }
        }
        if update {
            if let Some(board) = self.board.upgrade() {
                crate::rp2040::update_clocks(&board);
            }
        }
    }
}

impl Peripheral for Clocks {
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

#[cfg(test)]
mod tests {
    use super::super::super::rp2040::Rp2040;

    const CLOCKS_BASE: u32 = 0x4000_8000;
    const CLK_REF_CTRL: u32 = CLOCKS_BASE + 0x30;
    const CLK_REF_DIV: u32 = CLOCKS_BASE + 0x34;
    const CLK_SYS_CTRL: u32 = CLOCKS_BASE + 0x3c;
    const CLK_SYS_DIV: u32 = CLOCKS_BASE + 0x40;
    const CLK_PERI_CTRL: u32 = CLOCKS_BASE + 0x48;
    const PLL_SYS_BASE: u32 = 0x4002_8000;
    const PLL_USB_BASE: u32 = 0x4002_c000;
    const PLL_CS: u32 = 0x00;
    const PLL_FBDIV_INT: u32 = 0x08;
    const PLL_PRIM: u32 = 0x0c;
    const UARTIBRD: u32 = 0x4003_4000 + 0x24;
    const UARTFBRD: u32 = 0x4003_4000 + 0x28;
    const MHZ: f64 = 1_000_000.0;
    const CLK_REF_SRC_XOSC: u32 = 0x2;
    const CLK_SYS_SRC_AUX: u32 = 0x1;
    const CLK_SYS_AUXSRC_PLL_SYS: u32 = 0;
    const CLK_SYS_AUXSRC_XOSC: u32 = 0x3 << 5;
    const CLK_SYS_AUXSRC_GPIN0: u32 = 0x4 << 5;
    const CLK_PERI_ENABLE: u32 = 1 << 11;
    const CLK_PERI_AUXSRC_CLK_SYS: u32 = 0;
    const CLK_PERI_AUXSRC_PLL_USB: u32 = 2 << 5;

    fn init_pll(rp: &Rp2040, base: u32, refdiv: u32, fbdiv: u32, postdiv1: u32, postdiv2: u32) {
        rp.write_u32(base + PLL_CS, refdiv);
        rp.write_u32(base + PLL_FBDIV_INT, fbdiv);
        rp.write_u32(base + PLL_PRIM, (postdiv1 << 16) | (postdiv2 << 12));
    }

    fn set_sys_clock(rp: &Rp2040, fbdiv: u32, postdiv1: u32, postdiv2: u32) {
        init_pll(rp, PLL_SYS_BASE, 1, fbdiv, postdiv1, postdiv2);
        rp.write_u32(CLK_REF_CTRL, CLK_REF_SRC_XOSC);
        rp.write_u32(CLK_SYS_CTRL, CLK_SYS_SRC_AUX | CLK_SYS_AUXSRC_PLL_SYS);
    }

    #[test]
    fn defaults_to_125_mhz() {
        let rp = Rp2040::new();
        assert_eq!(rp.clk_sys(), 125.0 * MHZ);
    }

    #[test]
    fn computes_200_mhz() {
        let rp = Rp2040::new();
        set_sys_clock(&rp, 100, 6, 1);
        assert_eq!(rp.clk_sys(), 200.0 * MHZ);
    }

    #[test]
    fn applies_integer_and_fractional_divisor() {
        let rp = Rp2040::new();
        set_sys_clock(&rp, 125, 6, 2);
        rp.write_u32(CLK_SYS_DIV, (2 << 8) | 128);
        assert_eq!(rp.clk_sys(), 50.0 * MHZ);
    }

    #[test]
    fn integer_zero_divides_by_2_pow_16() {
        let rp = Rp2040::new();
        set_sys_clock(&rp, 125, 6, 2);
        rp.write_u32(CLK_SYS_DIV, 0);
        assert_eq!(rp.clk_sys(), (125.0 * MHZ) / 65536.0);
    }

    #[test]
    fn runs_straight_off_xosc() {
        let rp = Rp2040::new();
        rp.write_u32(CLK_SYS_CTRL, CLK_SYS_SRC_AUX | CLK_SYS_AUXSRC_XOSC);
        assert_eq!(rp.clk_sys(), 12.0 * MHZ);
    }

    #[test]
    fn follows_clk_ref() {
        let rp = Rp2040::new();
        rp.write_u32(CLK_REF_CTRL, CLK_REF_SRC_XOSC);
        rp.write_u32(CLK_REF_DIV, 2 << 8);
        rp.write_u32(CLK_SYS_CTRL, 0);
        assert_eq!(rp.clk_sys(), 6.0 * MHZ);
    }

    #[test]
    fn keeps_last_good_frequency_for_unmodelled_source() {
        let rp = Rp2040::new();
        set_sys_clock(&rp, 100, 6, 1);
        rp.write_u32(CLK_SYS_CTRL, CLK_SYS_SRC_AUX | CLK_SYS_AUXSRC_GPIN0);
        assert_eq!(rp.clk_sys(), 200.0 * MHZ);
    }

    #[test]
    fn notifies_listeners_on_change() {
        let rp = Rp2040::new();
        let calls = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = calls.clone();
        let _id = rp.add_clock_listener(Box::new(move |new, old| {
            sink.borrow_mut().push((new, old));
        }));
        set_sys_clock(&rp, 100, 6, 1);
        assert_eq!(
            calls.borrow().last().copied(),
            Some((200.0 * MHZ, 12.0 * MHZ))
        );
    }

    #[test]
    fn does_not_notify_when_unchanged() {
        let rp = Rp2040::new();
        set_sys_clock(&rp, 100, 6, 1);
        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
        let sink = calls.clone();
        rp.add_clock_listener(Box::new(move |_, _| sink.set(sink.get() + 1)));
        rp.write_u32(CLK_SYS_CTRL, CLK_SYS_SRC_AUX | CLK_SYS_AUXSRC_PLL_SYS);
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn stops_notifying_after_unsubscribe() {
        let rp = Rp2040::new();
        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
        let sink = calls.clone();
        let id = rp.add_clock_listener(Box::new(move |_, _| sink.set(sink.get() + 1)));
        rp.remove_clock_listener(id);
        set_sys_clock(&rp, 100, 6, 1);
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn retunes_systick_and_pwm() {
        let rp = Rp2040::new();
        set_sys_clock(&rp, 100, 6, 1);
        assert_eq!(rp.ppb.systick_timer.frequency(), 200.0 * MHZ);
        assert_eq!(rp.pwm.channels[0].timer.frequency(), 200.0 * MHZ);
    }

    fn init_pll_usb(rp: &Rp2040) {
        init_pll(rp, PLL_USB_BASE, 1, 40, 5, 2);
    }

    #[test]
    fn peri_follows_clk_sys() {
        let rp = Rp2040::new();
        set_sys_clock(&rp, 100, 6, 1);
        rp.write_u32(CLK_PERI_CTRL, CLK_PERI_ENABLE | CLK_PERI_AUXSRC_CLK_SYS);
        assert_eq!(rp.clk_peri(), 200.0 * MHZ);
    }

    #[test]
    fn peri_runs_off_usb_pll() {
        let rp = Rp2040::new();
        init_pll_usb(&rp);
        set_sys_clock(&rp, 100, 6, 1);
        rp.write_u32(CLK_PERI_CTRL, CLK_PERI_ENABLE | CLK_PERI_AUXSRC_PLL_USB);
        assert_eq!(rp.clk_peri(), 48.0 * MHZ);
    }

    #[test]
    fn keeps_last_peri_frequency_when_stopped() {
        let rp = Rp2040::new();
        rp.write_u32(CLK_PERI_CTRL, CLK_PERI_AUXSRC_PLL_USB);
        assert_eq!(rp.clk_peri(), 125.0 * MHZ);
    }

    #[test]
    fn renotifies_uart_when_peri_changes() {
        let rp = Rp2040::new();
        init_pll_usb(&rp);
        rp.write_u32(UARTIBRD, 26);
        rp.write_u32(UARTFBRD, 3);
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = seen.clone();
        rp.uart[0].set_on_baud_rate_change(Box::new(move |baud| sink.borrow_mut().push(baud)));
        rp.write_u32(CLK_PERI_CTRL, CLK_PERI_ENABLE | CLK_PERI_AUXSRC_PLL_USB);
        assert_eq!(*seen.borrow(), vec![115177]);
    }

    #[test]
    fn does_not_notify_uart_without_divider() {
        let rp = Rp2040::new();
        init_pll_usb(&rp);
        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
        let sink = calls.clone();
        rp.uart[0].set_on_baud_rate_change(Box::new(move |_| sink.set(sink.get() + 1)));
        rp.write_u32(CLK_PERI_CTRL, CLK_PERI_ENABLE | CLK_PERI_AUXSRC_PLL_USB);
        assert_eq!(calls.get(), 0);
    }
}
