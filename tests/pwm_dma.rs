use std::rc::Rc;

use rp2040rs::clock::SimulationClock;
use rp2040rs::rp2040::Rp2040;

const PWM_BASE: u32 = 0x4005_0000;
const CH0_CSR: u32 = PWM_BASE;
const CH0_CC: u32 = PWM_BASE + 0x0c;
const CH0_TOP: u32 = PWM_BASE + 0x10;

const DMA: u32 = 0x5000_0000;

#[test]
fn pwm_wrap_dreq_pulses_for_each_wrap() {
    let clock = Rc::new(SimulationClock::new());
    let rp = Rp2040::with_clock(clock.clone());
    rp.write_u32(0x2000_0000, 0x1111_1111);
    rp.write_u32(0x2000_0004, 0x2222_2222);
    rp.write_u32(DMA, 0x2000_0000);
    rp.write_u32(DMA + 4, 0x2000_0100);
    rp.write_u32(DMA + 8, 2);
    // EN | INCR_READ | INCR_WRITE | TREQ=PWM_WRAP0 | 32-bit size
    rp.write_u32(DMA + 0x0c, 1 | (1 << 4) | (1 << 5) | (24 << 15) | (2 << 2));

    rp.write_u32(CH0_TOP, 4);
    rp.write_u32(CH0_CC, 2);
    rp.write_u32(CH0_CSR, 1);

    clock.tick(40.0);
    assert_eq!(rp.read_u32(0x2000_0100), 0x1111_1111);
    assert_eq!(rp.read_u32(0x2000_0104), 0);

    clock.tick(40.0);
    assert_eq!(rp.read_u32(0x2000_0104), 0x2222_2222);
}
