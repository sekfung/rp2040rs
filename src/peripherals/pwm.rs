use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::clock::Alarm;
use crate::irq::Irq;
use crate::peripherals::dma::DreqChannel;
use crate::peripherals::peripheral::{atomic_update, Peripheral};
use crate::rp2040::BoardShared;
use crate::utils::Timer32;

const CHN_CSR: u32 = 0x00;
const CHN_DIV: u32 = 0x04;
const CHN_CTR: u32 = 0x08;
const CHN_CC: u32 = 0x0c;
const CHN_TOP: u32 = 0x10;
const EN: u32 = 0xa0;
const INTR: u32 = 0xa4;
const INTE: u32 = 0xa8;
const INTF: u32 = 0xac;
const INTS: u32 = 0xb0;
const CSR_PH_ADV: u32 = 1 << 7;
const CSR_PH_RET: u32 = 1 << 6;
const CSR_EN: u32 = 1 << 0;

#[derive(Clone)]
pub struct PwmChannel {
    pub timer: Timer32,
}

#[derive(Clone)]
pub struct Pwm {
    name: String,
    pub channels: [PwmChannel; 8],
    state: Rc<RefCell<PwmState>>,
    board: Weak<RefCell<BoardShared>>,
}

struct PwmState {
    csr: [u32; 8],
    div: [u32; 8],
    ctr: [u32; 8],
    cc: [u32; 8],
    top: [u32; 8],
    cc_pending: [u32; 8],
    top_pending: [u32; 8],
    cc_updated: [bool; 8],
    top_updated: [bool; 8],
    div_prescaler: [f64; 8],
    alarms: Vec<Alarm>,
    clock_freq: f64,
    int_raw: u32,
    int_enable: u32,
    int_force: u32,
}

impl Pwm {
    pub fn new(name: impl Into<String>, board: Weak<RefCell<BoardShared>>) -> Self {
        let clock = board
            .upgrade()
            .map(|b| b.borrow().clock.clone())
            .expect("PWM requires a board with a clock");
        let state = Rc::new(RefCell::new(PwmState {
            csr: [0; 8],
            div: [1 << 4; 8],
            ctr: [0; 8],
            cc: [0; 8],
            top: [0xffff; 8],
            cc_pending: [0; 8],
            top_pending: [0xffff; 8],
            cc_updated: [false; 8],
            top_updated: [false; 8],
            div_prescaler: [1.0; 8],
            alarms: Vec::with_capacity(8),
            clock_freq: 125_000_000.0,
            int_raw: 0,
            int_enable: 0,
            int_force: 0,
        }));
        let mut alarms = Vec::with_capacity(8);
        for index in 0..8 {
            let state_for_alarm = state.clone();
            let board_for_alarm = board.clone();
            let alarm = clock.create_alarm(Box::new(move || {
                pwm_tick(&state_for_alarm, &board_for_alarm, index);
            }));
            alarms.push(alarm);
        }
        state.borrow_mut().alarms = alarms;
        Self {
            name: name.into(),
            channels: std::array::from_fn(|_| PwmChannel {
                timer: Timer32::new(125_000_000.0),
            }),
            state,
            board,
        }
    }

    pub fn set_clock_freq(&self, frequency: f64) {
        self.state.borrow_mut().clock_freq = frequency;
        for channel in &self.channels {
            channel.timer.set_frequency(frequency);
        }
        for index in 0..8 {
            if self.state.borrow().csr[index] & CSR_EN != 0 {
                self.schedule_wrap(index);
            }
        }
    }

    pub fn reset(&self) {
        let mut state = self.state.borrow_mut();
        for alarm in &mut state.alarms {
            alarm.cancel();
        }
        state.csr = [0; 8];
        state.div = [1 << 4; 8];
        state.ctr = [0; 8];
        state.cc = [0; 8];
        state.top = [0xffff; 8];
        state.cc_pending = [0; 8];
        state.top_pending = [0xffff; 8];
        state.cc_updated = [false; 8];
        state.top_updated = [false; 8];
        state.div_prescaler = [1.0; 8];
        state.int_raw = 0;
        state.int_enable = 0;
        state.int_force = 0;
    }

    fn int_status(&self) -> u32 {
        let state = self.state.borrow();
        (state.int_raw & state.int_enable) | state.int_force
    }

    fn check_interrupts(&self) {
        let status = self.int_status();
        if let Some(board) = self.board.upgrade() {
            let core = board.borrow().core.clone();
            core.set_interrupt(Irq::PwmWrap as u32, status != 0);
        }
    }

    fn schedule_wrap(&self, index: usize) {
        let (freq, enabled, mut alarm) = {
            let state = self.state.borrow();
            (
                state.clock_freq,
                state.csr[index] & CSR_EN != 0,
                state.alarms[index].clone(),
            )
        };
        if !enabled || freq <= 0.0 {
            alarm.cancel();
            return;
        }
        let prescaler = self.state.borrow().div_prescaler[index];
        let nanos = prescaler * 1_000_000_000.0 / freq;
        alarm.schedule(nanos);
        self.state.borrow_mut().alarms[index] = alarm;
    }

    fn apply_double_buffered(&self, index: usize) {
        let mut state = self.state.borrow_mut();
        if state.cc_updated[index] {
            state.cc[index] = state.cc_pending[index];
            state.cc_updated[index] = false;
        }
        if state.top_updated[index] {
            state.top[index] = state.top_pending[index];
            state.top_updated[index] = false;
        }
    }

    fn set_pin_direction(&self, pin: usize, output: bool) {
        if let Some(board) = self.board.upgrade() {
            if let Some(gpio) = board.borrow().gpio.get(pin) {
                let value = gpio.raw_output_value();
                gpio.set_output(value, output);
            }
        }
    }

    fn set_direction(&self, index: usize, output: bool) {
        self.set_pin_direction(index * 2, output);
        self.set_pin_direction(index * 2 + 1, output);
        if index < 7 {
            self.set_pin_direction(16 + index * 2, output);
            self.set_pin_direction(17 + index * 2, output);
        }
    }

    pub fn gpio_value(&self) -> u32 {
        self.board
            .upgrade()
            .map(|board| {
                board
                    .borrow()
                    .gpio
                    .iter()
                    .enumerate()
                    .fold(0, |acc, (index, pin)| {
                        if pin.raw_output_enable() && pin.raw_output_value() {
                            acc | (1 << index)
                        } else {
                            acc
                        }
                    })
            })
            .unwrap_or(0)
    }

    pub fn gpio_direction(&self) -> u32 {
        self.board
            .upgrade()
            .map(|board| {
                board
                    .borrow()
                    .gpio
                    .iter()
                    .enumerate()
                    .fold(0, |acc, (index, pin)| {
                        if pin.raw_output_enable() {
                            acc | (1 << index)
                        } else {
                            acc
                        }
                    })
            })
            .unwrap_or(0)
    }

    fn cancel_wrap(&self, index: usize) {
        let mut state = self.state.borrow_mut();
        if let Some(alarm) = state.alarms.get_mut(index) {
            alarm.cancel();
        }
    }

    fn write_channel(&self, index: usize, register: u32, value: u32, atomic_type: u32) {
        let old_csr = self.state.borrow().csr[index];
        let current = match register {
            CHN_CSR => old_csr,
            CHN_DIV => self.state.borrow().div[index],
            CHN_CTR => self.state.borrow().ctr[index],
            CHN_CC => self.state.borrow().cc[index],
            CHN_TOP => self.state.borrow().top[index],
            _ => 0,
        };
        let atomic_value = if atomic_type == 0 {
            value
        } else {
            atomic_update(current, atomic_type, value)
        };
        match register {
            CHN_CSR => {
                let old_enabled = old_csr & CSR_EN != 0;
                let new_enabled = atomic_value & CSR_EN != 0;
                if new_enabled && !old_enabled {
                    self.apply_double_buffered(index);
                }
                let mut state = self.state.borrow_mut();
                state.csr[index] = atomic_value & !(CSR_PH_ADV | CSR_PH_RET);
                if new_enabled {
                    if atomic_value & CSR_PH_ADV != 0 {
                        let modulus = state.top[index].wrapping_add(1);
                        state.ctr[index] = state.ctr[index].wrapping_add(1) % modulus;
                    }
                    if atomic_value & CSR_PH_RET != 0 {
                        state.ctr[index] = if state.ctr[index] == 0 {
                            state.top[index]
                        } else {
                            state.ctr[index] - 1
                        };
                    }
                }
                drop(state);
                self.set_direction(index, new_enabled);
                if new_enabled {
                    self.schedule_wrap(index);
                } else if old_enabled {
                    self.cancel_wrap(index);
                }
            }
            CHN_DIV => {
                let div = atomic_value & 0x000f_ffff;
                let int_value = (div >> 4) & 0xff;
                let frac_value = div & 0xf;
                let prescaler = if int_value == 0 {
                    256.0
                } else {
                    f64::from(int_value)
                } + f64::from(frac_value) / 16.0;
                let mut state = self.state.borrow_mut();
                state.div[index] = div;
                state.div_prescaler[index] = prescaler;
            }
            CHN_CTR => {
                self.state.borrow_mut().ctr[index] = atomic_value & 0xffff;
                self.schedule_wrap(index);
            }
            CHN_CC => {
                let mut state = self.state.borrow_mut();
                state.cc_pending[index] = atomic_value;
                state.cc_updated[index] = true;
            }
            CHN_TOP => {
                let mut state = self.state.borrow_mut();
                state.top_pending[index] = atomic_value & 0xffff;
                state.top_updated[index] = true;
            }
            _ => {}
        }
    }
}

fn pwm_tick(state: &Rc<RefCell<PwmState>>, board: &Weak<RefCell<BoardShared>>, index: usize) {
    let (wrapped, cc, ctr, enabled, freq, prescaler, mut alarm) = {
        let mut state = state.borrow_mut();
        state.ctr[index] = state.ctr[index].wrapping_add(1);
        let mut wrapped = false;
        if state.ctr[index] > state.top[index] {
            state.ctr[index] = 0;
            state.int_raw |= 1 << index;
            if state.cc_updated[index] {
                state.cc[index] = state.cc_pending[index];
                state.cc_updated[index] = false;
            }
            if state.top_updated[index] {
                state.top[index] = state.top_pending[index];
                state.top_updated[index] = false;
            }
            wrapped = true;
        }
        let enabled = state.csr[index] & CSR_EN != 0;
        let freq = state.clock_freq;
        (
            wrapped,
            state.cc[index],
            state.ctr[index],
            enabled,
            freq,
            state.div_prescaler[index],
            state.alarms[index].clone(),
        )
    };
    if enabled && freq > 0.0 {
        alarm.schedule(prescaler * 1_000_000_000.0 / freq);
        state.borrow_mut().alarms[index] = alarm;
    } else {
        alarm.cancel();
    }
    if wrapped {
        if let Some(board) = board.upgrade() {
            if let Some(gpio) = board.borrow().gpio.get(index * 2) {
                gpio.set_output(cc & 0xffff != 0, true);
            }
            if let Some(gpio) = board.borrow().gpio.get(index * 2 + 1) {
                gpio.set_output(cc >> 16 != 0, true);
            }
            if index < 7 {
                if let Some(gpio) = board.borrow().gpio.get(16 + index * 2) {
                    gpio.set_output(cc & 0xffff != 0, true);
                }
                if let Some(gpio) = board.borrow().gpio.get(17 + index * 2) {
                    gpio.set_output(cc >> 16 != 0, true);
                }
            }
            let dma = board.borrow().dma.clone();
            if let Some(dma) = dma {
                let channel = match index {
                    0 => DreqChannel::PwmWrap0,
                    1 => DreqChannel::PwmWrap1,
                    2 => DreqChannel::PwmWrap2,
                    3 => DreqChannel::PwmWrap3,
                    4 => DreqChannel::PwmWrap4,
                    5 => DreqChannel::PwmWrap5,
                    6 => DreqChannel::PwmWrap6,
                    _ => DreqChannel::PwmWrap7,
                };
                dma.set_dreq(channel);
            }
        }
    } else {
        if let Some(board) = board.upgrade() {
            if ctr == (cc & 0xffff) {
                if let Some(gpio) = board.borrow().gpio.get(index * 2) {
                    gpio.set_output(false, true);
                }
                if index < 7 {
                    if let Some(gpio) = board.borrow().gpio.get(16 + index * 2) {
                        gpio.set_output(false, true);
                    }
                }
            }
            if ctr == (cc >> 16) {
                if let Some(gpio) = board.borrow().gpio.get(index * 2 + 1) {
                    gpio.set_output(false, true);
                }
                if index < 7 {
                    if let Some(gpio) = board.borrow().gpio.get(17 + index * 2) {
                        gpio.set_output(false, true);
                    }
                }
            }
        }
    }
    if let Some(board) = board.upgrade() {
        let core = board.borrow().core.clone();
        let status = {
            let state = state.borrow();
            (state.int_raw & state.int_enable) | state.int_force
        };
        core.set_interrupt(Irq::PwmWrap as u32, status != 0);
    }
}

impl Peripheral for Pwm {
    fn name(&self) -> &str {
        &self.name
    }

    fn read_u32(&self, offset: u32) -> u32 {
        if offset < EN {
            let index = (offset / 0x14) as usize;
            let register = offset % 0x14;
            if index >= 8 {
                return 0;
            }
            let state = self.state.borrow();
            return match register {
                CHN_CSR => state.csr[index],
                CHN_DIV => state.div[index],
                CHN_CTR => state.ctr[index],
                CHN_CC => state.cc[index],
                CHN_TOP => state.top[index],
                _ => 0,
            };
        }
        match offset {
            EN => self
                .state
                .borrow()
                .csr
                .iter()
                .enumerate()
                .fold(0, |acc, (i, csr)| acc | ((csr & CSR_EN) << i)),
            INTR => self.state.borrow().int_raw,
            INTE => self.state.borrow().int_enable,
            INTF => self.state.borrow().int_force,
            INTS => self.int_status(),
            _ => 0xffff_ffff,
        }
    }

    fn write_u32_atomic(&self, offset: u32, value: u32, atomic_type: u32) {
        if offset < EN {
            let index = (offset / 0x14) as usize;
            if index < 8 {
                self.write_channel(index, offset % 0x14, value, atomic_type);
            }
            return;
        }
        match offset {
            EN => {
                let current = self.read_u32(EN);
                let enabled_mask = if atomic_type == 0 {
                    value
                } else {
                    atomic_update(current, atomic_type, value)
                } & 0xff;
                for index in 0..8 {
                    let old_enabled = self.state.borrow().csr[index] & CSR_EN != 0;
                    let enabled = enabled_mask & (1 << index) != 0;
                    if enabled && !old_enabled {
                        self.apply_double_buffered(index);
                        self.set_direction(index, true);
                    } else if !enabled && old_enabled {
                        self.set_direction(index, false);
                    }
                    {
                        let mut state = self.state.borrow_mut();
                        if enabled {
                            state.csr[index] |= CSR_EN;
                        } else {
                            state.csr[index] &= !CSR_EN;
                        }
                    }
                    if enabled && !old_enabled {
                        self.schedule_wrap(index);
                    } else if !enabled && old_enabled {
                        self.cancel_wrap(index);
                    }
                }
            }
            INTR => {
                let current = self.read_u32(INTR);
                let clear_mask = if atomic_type == 0 {
                    value
                } else {
                    atomic_update(current, atomic_type, value)
                } & 0xff;
                self.state.borrow_mut().int_raw &= !clear_mask;
                self.check_interrupts();
            }
            INTE => {
                let current = self.read_u32(INTE);
                let atomic_value = if atomic_type == 0 {
                    value
                } else {
                    atomic_update(current, atomic_type, value)
                };
                self.state.borrow_mut().int_enable = atomic_value & 0xff;
                self.check_interrupts();
            }
            INTF => {
                let current = self.read_u32(INTF);
                let atomic_value = if atomic_type == 0 {
                    value
                } else {
                    atomic_update(current, atomic_type, value)
                };
                self.state.borrow_mut().int_force = atomic_value & 0xff;
                self.check_interrupts();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::{MockClock, SimulationClock};
    use crate::rp2040::Rp2040;

    const PWM_BASE: u32 = 0x4005_0000;
    const CH0_CSR: u32 = PWM_BASE;
    const CH0_CTR: u32 = PWM_BASE + 0x08;
    const CH0_TOP: u32 = PWM_BASE + 0x10;
    const PWM_INTR: u32 = PWM_BASE + 0xa4;
    const ATOMIC_SET: u32 = 0x2000;
    const CSR_PH_ADV: u32 = 1 << 7;
    const CSR_PH_RET: u32 = 1 << 6;
    const CSR_EN: u32 = 1;

    #[test]
    fn should_advance_counter_by_one_when_setting_ph_adv() {
        let rp = Rp2040::with_clock(Rc::new(MockClock::new()));
        rp.write_u32(CH0_CSR, CSR_EN);
        rp.write_u32(CH0_CTR, 10);
        rp.write_u32(CH0_CSR | ATOMIC_SET, CSR_PH_ADV);
        assert_eq!(rp.read_u32(CH0_CTR), 11);
    }

    #[test]
    fn should_retard_counter_by_one_when_setting_ph_ret() {
        let rp = Rp2040::with_clock(Rc::new(MockClock::new()));
        rp.write_u32(CH0_CSR, CSR_EN);
        rp.write_u32(CH0_CTR, 10);
        rp.write_u32(CH0_CSR | ATOMIC_SET, CSR_PH_RET);
        assert_eq!(rp.read_u32(CH0_CTR), 9);
    }

    #[test]
    fn should_self_clear_ph_adv_and_ph_ret() {
        let rp = Rp2040::with_clock(Rc::new(MockClock::new()));
        rp.write_u32(CH0_CSR, CSR_EN | CSR_PH_ADV | CSR_PH_RET);
        assert_eq!(rp.read_u32(CH0_CSR) & (CSR_PH_ADV | CSR_PH_RET), 0);
    }

    #[test]
    fn should_wrap_to_top_when_retarding_past_zero() {
        let rp = Rp2040::with_clock(Rc::new(MockClock::new()));
        rp.write_u32(CH0_CSR, CSR_EN);
        rp.write_u32(CH0_CTR, 0);
        rp.write_u32(CH0_CSR | ATOMIC_SET, CSR_PH_RET);
        assert_eq!(rp.read_u32(CH0_CTR), 0xffff);
    }

    #[test]
    fn should_not_change_counter_when_channel_not_running() {
        let rp = Rp2040::with_clock(Rc::new(MockClock::new()));
        rp.write_u32(CH0_CTR, 10);
        rp.write_u32(CH0_CSR | ATOMIC_SET, CSR_PH_ADV);
        assert_eq!(rp.read_u32(CH0_CTR), 10);
    }

    #[test]
    fn should_apply_advance_when_en_and_ph_adv_set_together() {
        let rp = Rp2040::with_clock(Rc::new(MockClock::new()));
        rp.write_u32(CH0_CSR, CSR_EN);
        rp.write_u32(CH0_CTR, 10);
        rp.write_u32(CH0_CSR, 0);
        rp.write_u32(CH0_CSR, CSR_EN | CSR_PH_ADV);
        assert_eq!(rp.read_u32(CH0_CTR), 11);
    }

    #[test]
    fn should_not_advance_on_write_that_clears_en() {
        let rp = Rp2040::with_clock(Rc::new(MockClock::new()));
        rp.write_u32(CH0_CSR, CSR_EN);
        rp.write_u32(CH0_CTR, 10);
        rp.write_u32(CH0_CSR, CSR_PH_ADV);
        assert_eq!(rp.read_u32(CH0_CTR), 10);
    }

    #[test]
    fn should_bring_wrap_interrupt_forward_by_one_cycle_when_advancing() {
        let clock = Rc::new(SimulationClock::new());
        let rp = Rp2040::with_clock(clock.clone());
        rp.write_u32(CH0_TOP, 999);
        rp.write_u32(CH0_CSR, CSR_EN);
        rp.write_u32(CH0_CTR, 0);
        rp.write_u32(CH0_CSR | ATOMIC_SET, CSR_PH_ADV);
        clock.tick(7991.0);
        assert_eq!(rp.read_u32(PWM_INTR) & 1, 0);
        clock.tick(1.0);
        assert_eq!(rp.read_u32(PWM_INTR) & 1, 1);
    }
}
