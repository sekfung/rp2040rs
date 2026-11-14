use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::clock::{Alarm, Clock};
use crate::gpio_pin::SOURCE_PWM;
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
const CSR_A_INV: u32 = 1 << 2;
const CSR_B_INV: u32 = 1 << 3;
const CSR_PH_CORRECT: u32 = 1 << 1;
const CSR_DIVMODE_SHIFT: u32 = 4;
const CSR_DIVMODE_MASK: u32 = 0x3;
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
    clock: Rc<dyn Clock>,
}

#[derive(Clone, Copy)]
struct CounterTiming {
    first_tick: f64,
    period: f64,
    event_ticks: u32,
}

impl CounterTiming {
    fn at(self, ticks: u32) -> f64 {
        self.first_tick + f64::from(ticks - 1) * self.period
    }

    fn elapsed_before_event(self, now: f64) -> u32 {
        if now < self.first_tick {
            return 0;
        }
        let mut elapsed = (((now - self.first_tick) / self.period) as u32)
            .saturating_add(1)
            .min(self.event_ticks - 1);
        // Rounding the quotient can move a read by one count at a tick boundary.
        if elapsed != 0 && self.at(elapsed) > now {
            elapsed -= 1;
        } else if elapsed + 1 < self.event_ticks && self.at(elapsed + 1) <= now {
            elapsed += 1;
        }
        elapsed
    }
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
    counting_up: [bool; 8],
    div_prescaler: [f64; 8],
    timing: [Option<CounterTiming>; 8],
    alarms: Vec<Alarm>,
    clock_freq: f64,
    int_raw: u32,
    int_enable: u32,
    int_force: u32,
}

impl PwmState {
    fn int_status(&self) -> u32 {
        (self.int_raw & self.int_enable) | self.int_force
    }

    fn phase_correct(&self, index: usize) -> bool {
        self.csr[index] & CSR_PH_CORRECT != 0
    }

    fn counter_after(&self, index: usize, ticks: u32) -> u32 {
        if self.phase_correct(index) && !self.counting_up[index] {
            self.ctr[index] - ticks
        } else {
            self.ctr[index] + ticks
        }
    }

    fn counter_at(&self, index: usize, now: f64) -> u32 {
        self.timing[index].map_or(self.ctr[index], |timing| {
            self.counter_after(index, timing.elapsed_before_event(now))
        })
    }

    /// Store all quiet counts up to now and return the time of the next count.
    fn sync_counter(&mut self, index: usize, now: f64) -> Option<f64> {
        let timing = self.timing[index]?;
        let elapsed = timing.elapsed_before_event(now);
        self.ctr[index] = self.counter_after(index, elapsed);
        Some(timing.at(elapsed + 1))
    }

    fn schedule(&mut self, index: usize, now: f64, first_tick: Option<f64>) {
        let period = self.div_prescaler[index] * 1e9 / self.clock_freq;
        if self.csr[index] & CSR_EN == 0 || !period.is_finite() || period <= 0.0 || !now.is_finite()
        {
            self.timing[index] = None;
            self.alarms[index].cancel();
            return;
        }
        let ctr = self.ctr[index];
        let top = self.top[index];
        let down = self.phase_correct(index) && !self.counting_up[index];
        let mut event_ticks = if down {
            ctr + 1
        } else {
            top.saturating_sub(ctr) + 1
        };
        for compare in [self.cc[index] & 0xffff, self.cc[index] >> 16] {
            if down && compare < ctr {
                event_ticks = event_ticks.min(ctr - compare);
            } else if !down && (ctr < compare && compare <= top) {
                event_ticks = event_ticks.min(compare - ctr);
            }
        }
        // The NVIC can clear its pending bit while the peripheral level stays high.
        if self.int_status() != 0 {
            event_ticks = 1;
        }
        let timing = CounterTiming {
            first_tick: first_tick.unwrap_or(now + period),
            period,
            event_ticks,
        };
        self.timing[index] = Some(timing);
        self.alarms[index].schedule((timing.at(event_ticks) - now).max(0.0));
    }

    fn apply_double_buffered(&mut self, index: usize) {
        if self.cc_updated[index] {
            self.cc[index] = self.cc_pending[index];
            self.cc_updated[index] = false;
        }
        if self.top_updated[index] {
            self.top[index] = self.top_pending[index];
            self.top_updated[index] = false;
        }
    }
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
            counting_up: [true; 8],
            div_prescaler: [1.0; 8],
            timing: [None; 8],
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
            let clock_for_alarm = Rc::downgrade(&clock);
            let alarm = clock.create_alarm(Box::new(move || {
                if let Some(clock) = clock_for_alarm.upgrade() {
                    pwm_tick(&state_for_alarm, &board_for_alarm, index, clock.nanos());
                }
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
            clock,
        }
    }

    pub fn set_clock_freq(&self, frequency: f64) {
        let now = self.clock.nanos();
        let mut state = self.state.borrow_mut();
        state.clock_freq = frequency;
        for channel in &self.channels {
            channel.timer.set_frequency(frequency);
        }
        for index in 0..8 {
            if state.csr[index] & CSR_EN != 0 {
                state.sync_counter(index, now);
                state.schedule(index, now, None);
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
        state.counting_up = [true; 8];
        state.div_prescaler = [1.0; 8];
        state.timing = [None; 8];
        state.int_raw = 0;
        state.int_enable = 0;
        state.int_force = 0;
    }

    fn int_status(&self) -> u32 {
        self.state.borrow().int_status()
    }

    fn check_interrupts(&self) {
        let status = self.int_status();
        if let Some(board) = self.board.upgrade() {
            let core = board.borrow().core.clone();
            core.set_interrupt(Irq::PwmWrap as u32, status != 0);
        }
    }

    fn reschedule_interrupt_change(&self, old_status: u32) {
        let now = self.clock.nanos();
        let mut state = self.state.borrow_mut();
        if (old_status == 0) != (state.int_status() == 0) {
            for index in 0..8 {
                if let Some(next_tick) = state.sync_counter(index, now) {
                    state.schedule(index, now, Some(next_tick));
                }
            }
        }
    }

    fn set_pin_direction(&self, pin: usize, output: bool) {
        if let Some(board) = self.board.upgrade() {
            if let Some(gpio) = board.borrow().gpio.get(pin) {
                gpio.set_source_output(SOURCE_PWM, false, output);
            }
        }
    }

    fn set_a_direction(&self, index: usize, output: bool) {
        self.set_pin_direction(index * 2, output);
        if index < 7 {
            self.set_pin_direction(16 + index * 2, output);
        }
    }

    fn set_b_direction(&self, index: usize, output: bool) {
        self.set_pin_direction(index * 2 + 1, output);
        if index < 7 {
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

    fn write_channel(&self, index: usize, register: u32, value: u32, atomic_type: u32) {
        let now = self.clock.nanos();
        let mut state = self.state.borrow_mut();
        let old_csr = state.csr[index];
        let current = match register {
            CHN_CSR => old_csr,
            CHN_DIV => state.div[index],
            CHN_CTR => state.counter_at(index, now),
            CHN_CC => state.cc[index],
            CHN_TOP => state.top[index],
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
                state.sync_counter(index, now);
                if new_enabled && !old_enabled {
                    state.apply_double_buffered(index);
                }
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
                let div_mode = (state.csr[index] >> CSR_DIVMODE_SHIFT) & CSR_DIVMODE_MASK;
                state.schedule(index, now, None);
                drop(state);
                self.set_a_direction(index, new_enabled);
                self.set_b_direction(index, new_enabled && div_mode == 0);
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
                let next_tick = state.sync_counter(index, now);
                state.div[index] = div;
                state.div_prescaler[index] = prescaler;
                if let Some(next_tick) = next_tick {
                    state.schedule(index, now, Some(next_tick));
                }
            }
            CHN_CTR => {
                state.ctr[index] = atomic_value & 0xffff;
                state.schedule(index, now, None);
            }
            CHN_CC => {
                state.cc_pending[index] = atomic_value;
                state.cc_updated[index] = true;
            }
            CHN_TOP => {
                state.top_pending[index] = atomic_value & 0xffff;
                state.top_updated[index] = true;
            }
            _ => {}
        }
    }
}

fn pwm_tick(state: &RefCell<PwmState>, board: &Weak<RefCell<BoardShared>>, index: usize, now: f64) {
    let (wrapped, cc, ctr, csr, status) = {
        let mut state = state.borrow_mut();
        let Some(timing) = state.timing[index] else {
            return;
        };
        state.ctr[index] = state.counter_after(index, timing.event_ticks - 1);
        let mut wrapped = false;
        let csr = state.csr[index];
        if csr & CSR_PH_CORRECT != 0 {
            if state.counting_up[index] {
                if state.ctr[index] < state.top[index] {
                    state.ctr[index] += 1;
                } else {
                    state.counting_up[index] = false;
                }
            } else if state.ctr[index] > 0 {
                state.ctr[index] -= 1;
            } else {
                state.counting_up[index] = true;
                wrapped = true;
            }
        } else {
            state.ctr[index] = state.ctr[index].wrapping_add(1);
            if state.ctr[index] > state.top[index] {
                state.ctr[index] = 0;
                wrapped = true;
            }
        }
        if wrapped {
            let old_status = state.int_status();
            state.int_raw |= 1 << index;
            state.apply_double_buffered(index);
            if old_status == 0 && state.int_status() != 0 {
                for other in (0..8).filter(|&other| other != index) {
                    if let Some(next_tick) = state.sync_counter(other, now) {
                        state.schedule(other, now, Some(next_tick));
                    }
                }
            }
        }
        state.schedule(index, now, None);
        (
            wrapped,
            state.cc[index],
            state.ctr[index],
            csr,
            state.int_status(),
        )
    };
    if wrapped {
        if let Some(board) = board.upgrade() {
            set_pwm_pin(&board, index, index * 2, cc & 0xffff != 0, csr);
            set_pwm_pin(&board, index, index * 2 + 1, cc >> 16 != 0, csr);
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
                dma.pulse_dreq(channel);
            }
        }
    } else {
        if let Some(board) = board.upgrade() {
            if ctr == (cc & 0xffff) {
                set_pwm_pin(&board, index, index * 2, false, csr);
            }
            if ctr == (cc >> 16) {
                set_pwm_pin(&board, index, index * 2 + 1, false, csr);
            }
        }
    }
    if let Some(board) = board.upgrade() {
        let core = board.borrow().core.clone();
        core.set_interrupt(Irq::PwmWrap as u32, status != 0);
    }
}

fn set_pwm_pin(board: &Rc<RefCell<BoardShared>>, index: usize, pin: usize, value: bool, csr: u32) {
    let inverted = if pin & 1 == 0 {
        csr & CSR_A_INV != 0
    } else {
        csr & CSR_B_INV != 0
    };
    if let Some(gpio) = board.borrow().gpio.get(pin) {
        gpio.set_source_output(SOURCE_PWM, value ^ inverted, true);
    }
    if index < 7 {
        let mirror = 16 + index * 2 + (pin & 1);
        if let Some(gpio) = board.borrow().gpio.get(mirror) {
            gpio.set_source_output(SOURCE_PWM, value ^ inverted, true);
        }
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
                CHN_CTR => state.counter_at(index, self.clock.nanos()),
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
                let now = self.clock.nanos();
                for index in 0..8 {
                    let mut state = self.state.borrow_mut();
                    let old_enabled = state.csr[index] & CSR_EN != 0;
                    let enabled = enabled_mask & (1 << index) != 0;
                    if enabled != old_enabled {
                        state.sync_counter(index, now);
                        if enabled {
                            state.apply_double_buffered(index);
                            state.csr[index] |= CSR_EN;
                        } else {
                            state.csr[index] &= !CSR_EN;
                        }
                        state.schedule(index, now, None);
                    }
                    let div_mode = (state.csr[index] >> CSR_DIVMODE_SHIFT) & CSR_DIVMODE_MASK;
                    drop(state);
                    self.set_a_direction(index, enabled);
                    self.set_b_direction(index, enabled && div_mode == 0);
                }
            }
            INTR => {
                let current = self.read_u32(INTR);
                let clear_mask = if atomic_type == 0 {
                    value
                } else {
                    atomic_update(current, atomic_type, value)
                } & 0xff;
                let old_status = self.int_status();
                self.state.borrow_mut().int_raw &= !clear_mask;
                self.reschedule_interrupt_change(old_status);
                self.check_interrupts();
            }
            INTE => {
                let current = self.read_u32(INTE);
                let atomic_value = if atomic_type == 0 {
                    value
                } else {
                    atomic_update(current, atomic_type, value)
                };
                let old_status = self.int_status();
                self.state.borrow_mut().int_enable = atomic_value & 0xff;
                self.reschedule_interrupt_change(old_status);
                self.check_interrupts();
            }
            INTF => {
                let current = self.read_u32(INTF);
                let atomic_value = if atomic_type == 0 {
                    value
                } else {
                    atomic_update(current, atomic_type, value)
                };
                let old_status = self.int_status();
                self.state.borrow_mut().int_force = atomic_value & 0xff;
                self.reschedule_interrupt_change(old_status);
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
