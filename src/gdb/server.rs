use crate::cortex_m0_core::{SYSM_CONTROL, SYSM_MSP, SYSM_PRIMASK, SYSM_PSP};
use crate::gdb::target::GdbTarget;
use crate::gdb::utils::{decode_hex_buf, decode_hex_u32_le, encode_hex_buf, encode_hex_u32_le};

pub const STOP_REPLY_SIGINT: &str = "S02";
pub const STOP_REPLY_TRAP: &str = "S05";

const REGISTER_INFO: [&str; 17] = [
    "name:r0;bitsize:32;offset:0;encoding:int;format:hex;set:General Purpose Registers;",
    "name:r1;bitsize:32;offset:4;encoding:int;format:hex;set:General Purpose Registers;",
    "name:r2;bitsize:32;offset:8;encoding:int;format:hex;set:General Purpose Registers;",
    "name:r3;bitsize:32;offset:12;encoding:int;format:hex;set:General Purpose Registers;",
    "name:r4;bitsize:32;offset:16;encoding:int;format:hex;set:General Purpose Registers;",
    "name:r5;bitsize:32;offset:20;encoding:int;format:hex;set:General Purpose Registers;",
    "name:r6;bitsize:32;offset:24;encoding:int;format:hex;set:General Purpose Registers;",
    "name:r7;bitsize:32;offset:28;encoding:int;format:hex;set:General Purpose Registers;",
    "name:r8;bitsize:32;offset:32;encoding:int;format:hex;set:General Purpose Registers;",
    "name:r9;bitsize:32;offset:36;encoding:int;format:hex;set:General Purpose Registers;",
    "name:r10;bitsize:32;offset:40;encoding:int;format:hex;set:General Purpose Registers;",
    "name:r11;bitsize:32;offset:44;encoding:int;format:hex;set:General Purpose Registers;",
    "name:r12;bitsize:32;offset:48;encoding:int;format:hex;set:General Purpose Registers;",
    "name:sp;bitsize:32;offset:52;encoding:int;format:hex;set:General Purpose Registers;",
    "name:lr;bitsize:32;offset:56;encoding:int;format:hex;set:General Purpose Registers;",
    "name:pc;bitsize:32;offset:60;encoding:int;format:hex;set:General Purpose Registers;",
    "name:cpsr;bitsize:32;offset:64;encoding:int;format:hex;set:General Purpose Registers;",
];

const TARGET_XML: &str = "<?xml version=\"1.0\"?><!DOCTYPE target SYSTEM \"gdb-target.dtd\"><target version=\"1.0\"><architecture>arm</architecture><feature name=\"org.gnu.gdb.arm.m-profile\"><reg name=\"r0\" bitsize=\"32\" regnum=\"0\"/><reg name=\"r1\" bitsize=\"32\" regnum=\"1\"/><reg name=\"r2\" bitsize=\"32\" regnum=\"2\"/><reg name=\"r3\" bitsize=\"32\" regnum=\"3\"/><reg name=\"r4\" bitsize=\"32\" regnum=\"4\"/><reg name=\"r5\" bitsize=\"32\" regnum=\"5\"/><reg name=\"r6\" bitsize=\"32\" regnum=\"6\"/><reg name=\"r7\" bitsize=\"32\" regnum=\"7\"/><reg name=\"r8\" bitsize=\"32\" regnum=\"8\"/><reg name=\"r9\" bitsize=\"32\" regnum=\"9\"/><reg name=\"r10\" bitsize=\"32\" regnum=\"10\"/><reg name=\"r11\" bitsize=\"32\" regnum=\"11\"/><reg name=\"r12\" bitsize=\"32\" regnum=\"12\"/><reg name=\"sp\" bitsize=\"32\" regnum=\"13\"/><reg name=\"lr\" bitsize=\"32\" regnum=\"14\"/><reg name=\"pc\" bitsize=\"32\" regnum=\"15\"/><reg name=\"xPSR\" bitsize=\"32\" regnum=\"16\"/></feature></target>";

pub struct GdbServer {
    pub target: Box<dyn GdbTarget>,
}

impl GdbServer {
    pub fn new(target: Box<dyn GdbTarget>) -> Self {
        Self { target }
    }

    pub fn process_message(&mut self, cmd: &str) -> Option<String> {
        if cmd == "Hg0" {
            return Some("OK".into());
        }
        let rp2040 = self.target.rp2040();
        let core = &rp2040.core;
        match cmd.chars().next()? {
            '?' => Some(STOP_REPLY_TRAP.into()),
            'q' => {
                if cmd.starts_with("qSupported:") {
                    return Some("PacketSize=4000;vContSupported+;qXfer:features:read+".into());
                }
                if cmd == "qAttached" {
                    return Some("1".into());
                }
                if cmd.starts_with("qXfer:features:read:target.xml") {
                    return Some(format!("l{TARGET_XML}"));
                }
                if cmd.starts_with("qRegisterInfo") {
                    let index = usize::from_str_radix(cmd.get(13..).unwrap_or(""), 16).ok();
                    return Some(match index.and_then(|i| REGISTER_INFO.get(i)) {
                        Some(info) => (*info).into(),
                        None => "E45".into(),
                    });
                }
                if cmd == "qHostInfo" {
                    return Some("triple:61726d76366d2d6e6f6e652d756e6b6e6f776e2d65616269;endian:little;ptrsize:4;".into());
                }
                if cmd == "qProcessInfo" {
                    return Some("pid:1;endian:little;ptrsize:4;".into());
                }
                Some(String::new())
            }
            'v' => {
                if cmd == "vCont?" {
                    return Some("vCont;c;C;s;S".into());
                }
                if cmd.starts_with("vCont;c") {
                    self.continue_target();
                    return None;
                }
                if cmd.starts_with("vCont;s") {
                    rp2040.step();
                    return Some(self.step_reply());
                }
                Some(String::new())
            }
            'c' => {
                self.continue_target();
                Some("OK".into())
            }
            'g' => {
                let mut bytes = Vec::with_capacity(17 * 4);
                for i in 0..16 {
                    bytes.extend_from_slice(&core.reg(i).to_le_bytes());
                }
                bytes.extend_from_slice(&core.xpsr().to_le_bytes());
                Some(encode_hex_buf(&bytes))
            }
            'p' => {
                let index = usize::from_str_radix(cmd.get(1..).unwrap_or(""), 16).ok()?;
                let value = match index {
                    0..=15 => core.reg(index),
                    0x10 => core.xpsr(),
                    0x11 => core.msp(),
                    0x12 => core.psp(),
                    0x13 => u32::from(core.pm()),
                    0x14 | 0x15 => 0,
                    0x16 => core.read_special_register(SYSM_CONTROL),
                    _ => return Some("E45".into()),
                };
                Some(encode_hex_u32_le(value))
            }
            'P' => {
                let (index, value) = cmd.get(1..)?.split_once('=')?;
                let index = usize::from_str_radix(index, 16).ok()?;
                let value = decode_hex_u32_le(value.trim());
                match index {
                    0..=15 => core.set_reg(index, value),
                    0x10 => core.set_xpsr(value),
                    0x11 => core.write_special_register(SYSM_MSP, value),
                    0x12 => core.write_special_register(SYSM_PSP, value),
                    0x13 => core.write_special_register(SYSM_PRIMASK, value),
                    0x14 | 0x15 => {}
                    0x16 => core.write_special_register(SYSM_CONTROL, value),
                    _ => return Some("E00".into()),
                }
                Some("OK".into())
            }
            'm' => {
                let (address, length) = cmd.get(1..)?.split_once(',')?;
                let address = u32::from_str_radix(address, 16).ok()?;
                let length = usize::from_str_radix(length, 16).ok()?;
                let mut bytes = Vec::with_capacity(length);
                for i in 0..length {
                    bytes.push(rp2040.read_u8(address.wrapping_add(i as u32)));
                }
                Some(encode_hex_buf(&bytes))
            }
            'M' => {
                let parts: Vec<&str> = cmd.get(1..)?.split([',', ':']).collect();
                if parts.len() < 3 {
                    return Some("E00".into());
                }
                let address = u32::from_str_radix(parts[0], 16).ok()?;
                let length = usize::from_str_radix(parts[1], 16).ok()?;
                let data = decode_hex_buf(parts[2]);
                for (i, byte) in data.iter().take(length).enumerate() {
                    rp2040.write_u8(address.wrapping_add(i as u32), *byte);
                }
                Some("OK".into())
            }
            _ => Some(String::new()),
        }
    }

    fn continue_target(&mut self) {
        if !self.target.executing() {
            self.target.execute();
        }
        if !self.target.executing() {
            let rp = self.target.rp2040();
            let pc = rp.core.pc().wrapping_sub(rp.core.break_rewind());
            rp.core.set_pc(pc);
        }
    }

    fn step_reply(&self) -> String {
        let rp = self.target.rp2040();
        let mut status = String::from("T05");
        for i in 0..16 {
            status.push_str(&format!("{:02x}:{};", i, encode_hex_u32_le(rp.core.reg(i))));
        }
        status.push_str(&format!("10:{};", encode_hex_u32_le(rp.core.xpsr())));
        status.push_str("reason:trace;");
        status
    }
}
