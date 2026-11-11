use rp2040rs::simulator::Simulator;
use rp2040rs::utils::assembler::{opcode_nop, opcode_wfi};
use rp2040rs::GdbTarget;

#[test]
fn simulator_executes_a_bounded_batch_and_stops() {
    let mut sim = Simulator::new();
    sim.rp2040.write_u16(0x2000_0000, opcode_nop());
    sim.rp2040.write_u16(0x2000_0002, opcode_nop());
    sim.rp2040.write_u16(0x2000_0004, opcode_nop());
    sim.rp2040.core.set_pc(0x2000_0000);
    sim.execute_steps(3);
    assert_eq!(sim.rp2040.core.pc(), 0x2000_0006);
    assert!(!sim.executing());
}

#[test]
fn simulator_stops_when_waiting_without_an_alarm() {
    let mut sim = Simulator::new();
    sim.rp2040.write_u16(0x2000_0010, opcode_wfi());
    sim.rp2040.core.set_pc(0x2000_0010);
    sim.execute_once();
    assert!(sim.rp2040.core.waiting());
    assert!(!sim.executing());
}

#[test]
fn explicit_stop_keeps_target_stopped() {
    let mut sim = Simulator::new();
    sim.stop();
    assert!(!sim.executing());
}
