use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;

use rp2040rs::gdb::utils::{decode_hex_buf, encode_hex_u32_le};
use rp2040rs::{GdbConnection, GdbServer, GdbTarget, GdbTcpServer, Rp2040, Simulator};

fn server() -> GdbServer {
    GdbServer::new(Box::new(Simulator::new()))
}

fn server_with_rp() -> (GdbServer, Rp2040) {
    let simulator = Simulator::new();
    let rp2040 = simulator.rp2040.clone();
    (GdbServer::new(Box::new(simulator)), rp2040)
}

#[test]
fn reads_and_writes_registers_and_memory() {
    let (mut gdb, rp2040) = server_with_rp();
    rp2040.core.set_reg(0, 0x1234_5678);
    rp2040.core.set_xpsr(0x0100_0000);

    assert_eq!(gdb.process_message("p0").unwrap(), "78563412");
    assert_eq!(gdb.process_message("g").unwrap().len(), 160);
    assert_eq!(
        gdb.process_message("p10").unwrap(),
        encode_hex_u32_le(0x0100_0000)
    );

    assert_eq!(gdb.process_message("p13").unwrap(), "00");
    assert_eq!(gdb.process_message("P13=01").unwrap(), "OK");
    assert_eq!(gdb.process_message("p13").unwrap(), "01");

    assert_eq!(gdb.process_message("P0=efbeadde").unwrap(), "OK");
    assert_eq!(rp2040.core.reg(0), 0xdead_beef);

    assert_eq!(gdb.process_message("M20000000,4:78563412").unwrap(), "OK");
    assert_eq!(rp2040.read_u32(0x2000_0000), 0x1234_5678);
    assert_eq!(gdb.process_message("m20000000,4").unwrap(), "78563412");
}

#[test]
fn steps_continues_and_reports_break() {
    let (mut gdb, rp2040) = server_with_rp();
    rp2040.write_u16(0x2000_0000, 0xbf00); // NOP
    rp2040.core.set_pc(0x2000_0000);
    let step = gdb.process_message("vCont;s").unwrap();
    assert!(step.starts_with("T05"));
    assert_eq!(rp2040.core.pc(), 0x2000_0002);

    rp2040.write_u16(0x2000_0000, 0xbe00); // BKPT
    rp2040.core.set_pc(0x2000_0000);
    assert!(gdb.process_message("c").unwrap().starts_with("S05"));
    assert_eq!(rp2040.core.pc(), 0x2000_0000);
    assert!(!gdb.target.executing());

    rp2040.write_u16(0x2000_0000, 0xbf00); // NOP
    rp2040.core.set_pc(0x2000_0000);
    let plain_step = gdb.process_message("s").unwrap();
    assert!(plain_step.starts_with("T05"));
    assert_eq!(rp2040.core.pc(), 0x2000_0002);
}

#[test]
fn continue_without_break_does_not_report_trap() {
    let (mut gdb, rp2040) = server_with_rp();
    rp2040.write_u16(0x2000_0000, 0xe7fe); // b .
    rp2040.core.set_pc(0x2000_0000);
    let reply = gdb.process_message("c");
    assert_eq!(reply.as_deref(), Some("OK"), "false SIGTRAP: {reply:?}");
}

#[test]
fn stepping_onto_bkpt_rewinds_pc_and_reports_trap() {
    let (mut gdb, rp2040) = server_with_rp();
    rp2040.write_u16(0x2000_0000, 0xbe00); // BKPT
    rp2040.core.set_pc(0x2000_0000);
    let reply = gdb.process_message("s").unwrap();
    assert!(reply.starts_with("S05"), "reply was {reply}");
    assert_eq!(rp2040.core.pc(), 0x2000_0000);
}

#[test]
fn target_xml_register_sizes_match_g_packet() {
    let mut gdb = server();
    let xml = gdb
        .process_message("qXfer:features:read:target.xml:0,ffff")
        .unwrap();
    assert!(xml.contains("name=\"control\" bitsize=\"2\""));
    assert!(xml.contains("name=\"primask\" bitsize=\"1\""));
    assert_eq!(gdb.process_message("g").unwrap().len(), 160);
}

#[test]
fn simulator_batches_can_continue_again() {
    let mut simulator = Simulator::new();
    simulator.execute_steps(8);
    assert!(!simulator.executing());
    simulator.execute_steps(8);
    assert!(!simulator.executing());
}

#[test]
fn query_commands_return_expected_packets() {
    let mut gdb = server();
    assert_eq!(gdb.process_message("?").unwrap(), "S05");
    assert_eq!(
        gdb.process_message("qSupported:").unwrap(),
        "PacketSize=4000;vContSupported+;qXfer:features:read+"
    );
    assert!(gdb
        .process_message("qXfer:features:read:target.xml:0,ffff")
        .unwrap()
        .starts_with('l'));
    let chunk = gdb
        .process_message("qXfer:features:read:target.xml:0,10")
        .unwrap();
    assert!(chunk.starts_with('m'));
    assert_eq!(chunk.len(), 17);
    assert!(gdb
        .process_message("qRegisterInfo0")
        .unwrap()
        .contains("name:r0"));
    assert!(gdb
        .process_message("qRegisterInfo11")
        .unwrap()
        .contains("name:msp"));
    assert!(gdb
        .process_message("qXfer:features:read:target.xml:0,ffff")
        .unwrap()
        .contains("org.gnu.gdb.arm.m-system"));
    assert_eq!(gdb.process_message("qRegisterInfo99").unwrap(), "E45");
    assert_eq!(gdb.process_message("P0=12").unwrap(), "E00");
    assert_eq!(gdb.process_message("p99").unwrap(), "");
}

#[test]
#[ignore = "requires loopback bind"]
fn tcp_server_completes_one_handshake() {
    let tcp = GdbTcpServer::bind(0).expect("bind loopback GDB listener");
    let server_addr = tcp.local_addr().unwrap();
    let client = thread::spawn(move || {
        let mut stream = TcpStream::connect(server_addr).expect("connect to gdb server");
        stream.write_all(b"$?#3f").unwrap();
        let mut response = String::new();
        let mut buffer = [0u8; 64];
        while !response.contains("$S05#b8") {
            let count = stream.read(&mut buffer).expect("read gdb response");
            assert!(count > 0, "server closed before stop reply: {response:?}");
            response.push_str(&String::from_utf8_lossy(&buffer[..count]));
        }
        response
    });
    let mut gdb = server();
    tcp.serve_one(&mut gdb).expect("serve one gdb connection");
    let response = client.join().unwrap();
    assert!(response.contains("+$S05#b8"), "response was {response:?}");
}

#[test]
fn qxfer_without_range_returns_full_xml() {
    let mut gdb = server();
    let xml = gdb
        .process_message("qXfer:features:read:target.xml")
        .unwrap();
    assert!(xml.starts_with('l'));
    assert!(xml.contains("<architecture>arm</architecture>"));
}

#[test]
fn rsp_framing_acknowledges_and_wraps_responses() {
    let mut gdb = server();
    let mut connection = GdbConnection::new();
    let responses = connection.feed_data("$?#3f", &mut gdb);
    assert_eq!(responses, vec!["+".to_string(), "$S05#b8".to_string()]);
}

#[test]
fn gdb_packet_helpers_round_trip() {
    assert_eq!(decode_hex_buf("78563412"), vec![0x78, 0x56, 0x34, 0x12]);
}
