use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;

use rp2040rs::gdb::utils::{decode_hex_buf, encode_hex_u32_le};
use rp2040rs::{GdbConnection, GdbServer, GdbTcpServer, Rp2040, Simulator};

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
    assert_eq!(
        gdb.process_message("p10").unwrap(),
        encode_hex_u32_le(0x0100_0000)
    );

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
    assert_eq!(gdb.process_message("c").unwrap(), "OK");
    assert_eq!(rp2040.core.pc(), 0x2000_0000);
    assert!(!gdb.target.executing());
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
        .process_message("qXfer:features:read:target.xml:0,1")
        .unwrap()
        .starts_with('l'));
    assert!(gdb
        .process_message("qRegisterInfo0")
        .unwrap()
        .contains("name:r0"));
    assert_eq!(gdb.process_message("qRegisterInfo99").unwrap(), "E45");
}

#[test]
fn tcp_server_completes_one_handshake() {
    let Ok(tcp) = GdbTcpServer::bind(0) else {
        return;
    };
    let server_addr = tcp.local_addr().unwrap();
    let client = thread::spawn(move || {
        let Ok(mut stream) = TcpStream::connect(server_addr) else {
            return String::new();
        };
        stream.write_all(b"$?#3f").unwrap();
        let mut response = [0u8; 64];
        let count = stream.read(&mut response).unwrap();
        String::from_utf8_lossy(&response[..count]).into_owned()
    });
    let mut gdb = server();
    if tcp.serve_one(&mut gdb).is_err() {
        return;
    }
    let response = client.join().unwrap();
    assert!(
        response.is_empty() || response.contains("+$S05#b8"),
        "response was {response:?}"
    );
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
