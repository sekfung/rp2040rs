use crate::cortex_m0_core::{SYSM_CONTROL, SYSM_MSP, SYSM_PRIMASK, SYSM_PSP};
use crate::gdb::target::GdbTarget;
use crate::gdb::utils::{decode_hex_buf, encode_hex_buf, encode_hex_u32_le};

pub const STOP_REPLY_SIGINT: &str = "S02";
pub const STOP_REPLY_TRAP: &str = "S05";

const REGISTER_INFO: [&str; 23] = [
    "name:r0;bitsize:32;offset:0;encoding:int;format:hex;set:General Purpose Registers;generic:arg1;gcc:0;dwarf:0;",
    "name:r1;bitsize:32;offset:4;encoding:int;format:hex;set:General Purpose Registers;generic:arg2;gcc:1;dwarf:1;",
    "name:r2;bitsize:32;offset:8;encoding:int;format:hex;set:General Purpose Registers;generic:arg3;gcc:2;dwarf:2;",
    "name:r3;bitsize:32;offset:12;encoding:int;format:hex;set:General Purpose Registers;generic:arg4;gcc:3;dwarf:3;",
    "name:r4;bitsize:32;offset:16;encoding:int;format:hex;set:General Purpose Registers;gcc:4;dwarf:4;",
    "name:r5;bitsize:32;offset:20;encoding:int;format:hex;set:General Purpose Registers;gcc:5;dwarf:5;",
    "name:r6;bitsize:32;offset:24;encoding:int;format:hex;set:General Purpose Registers;gcc:6;dwarf:6;",
    "name:r7;bitsize:32;offset:28;encoding:int;format:hex;set:General Purpose Registers;gcc:7;dwarf:7;",
    "name:r8;bitsize:32;offset:32;encoding:int;format:hex;set:General Purpose Registers;gcc:8;dwarf:8;",
    "name:r9;bitsize:32;offset:36;encoding:int;format:hex;set:General Purpose Registers;gcc:9;dwarf:9;",
    "name:r10;bitsize:32;offset:40;encoding:int;format:hex;set:General Purpose Registers;gcc:10;dwarf:10;",
    "name:r11;bitsize:32;offset:44;encoding:int;format:hex;set:General Purpose Registers;generic:fp;gcc:11;dwarf:11;",
    "name:r12;bitsize:32;offset:48;encoding:int;format:hex;set:General Purpose Registers;gcc:12;dwarf:12;",
    "name:sp;bitsize:32;offset:52;encoding:int;format:hex;set:General Purpose Registers;generic:sp;alt-name:r13;gcc:13;dwarf:13;",
    "name:lr;bitsize:32;offset:56;encoding:int;format:hex;set:General Purpose Registers;generic:ra;alt-name:r14;gcc:14;dwarf:14;",
    "name:pc;bitsize:32;offset:60;encoding:int;format:hex;set:General Purpose Registers;generic:pc;alt-name:r15;gcc:15;dwarf:15;",
    "name:cpsr;bitsize:32;offset:64;encoding:int;format:hex;set:General Purpose Registers;generic:flags;alt-name:psr;gcc:16;dwarf:16;",
    "name:msp;bitsize:32;offset:68;encoding:int;format:hex;set:System Registers;",
    "name:psp;bitsize:32;offset:72;encoding:int;format:hex;set:System Registers;",
    "name:primask;bitsize:1;offset:76;encoding:int;format:hex;set:System Registers;",
    "name:basepri;bitsize:8;offset:77;encoding:int;format:hex;set:System Registers;",
    "name:faultmask;bitsize:1;offset:78;encoding:int;format:hex;set:System Registers;",
    "name:control;bitsize:2;offset:79;encoding:int;format:hex;set:System Registers;",
];

const TARGET_XML: &str = r#"<?xml version="1.0"?>
<!DOCTYPE target SYSTEM "gdb-target.dtd">
<target version="1.0">
<architecture>arm</architecture>
<feature name="org.gnu.gdb.arm.m-profile">
<reg name="r0" bitsize="32" regnum="0" save-restore="yes" type="int" group="general"/>
<reg name="r1" bitsize="32" regnum="1" save-restore="yes" type="int" group="general"/>
<reg name="r2" bitsize="32" regnum="2" save-restore="yes" type="int" group="general"/>
<reg name="r3" bitsize="32" regnum="3" save-restore="yes" type="int" group="general"/>
<reg name="r4" bitsize="32" regnum="4" save-restore="yes" type="int" group="general"/>
<reg name="r5" bitsize="32" regnum="5" save-restore="yes" type="int" group="general"/>
<reg name="r6" bitsize="32" regnum="6" save-restore="yes" type="int" group="general"/>
<reg name="r7" bitsize="32" regnum="7" save-restore="yes" type="int" group="general"/>
<reg name="r8" bitsize="32" regnum="8" save-restore="yes" type="int" group="general"/>
<reg name="r9" bitsize="32" regnum="9" save-restore="yes" type="int" group="general"/>
<reg name="r10" bitsize="32" regnum="10" save-restore="yes" type="int" group="general"/>
<reg name="r11" bitsize="32" regnum="11" save-restore="yes" type="int" group="general"/>
<reg name="r12" bitsize="32" regnum="12" save-restore="yes" type="int" group="general"/>
<reg name="sp" bitsize="32" regnum="13" save-restore="yes" type="data_ptr" group="general"/>
<reg name="lr" bitsize="32" regnum="14" save-restore="yes" type="int" group="general"/>
<reg name="pc" bitsize="32" regnum="15" save-restore="yes" type="code_ptr" group="general"/>
<reg name="xPSR" bitsize="32" regnum="16" save-restore="yes" type="int" group="general"/>
</feature>
<feature name="org.gnu.gdb.arm.m-system">
<reg name="msp" bitsize="32" regnum="17" save-restore="yes" type="data_ptr" group="system"/>
<reg name="psp" bitsize="32" regnum="18" save-restore="yes" type="data_ptr" group="system"/>
<reg name="primask" bitsize="1" regnum="19" save-restore="yes" type="int8" group="system"/>
<reg name="basepri" bitsize="8" regnum="20" save-restore="yes" type="int8" group="system"/>
<reg name="faultmask" bitsize="1" regnum="21" save-restore="yes" type="int8" group="system"/>
<reg name="control" bitsize="2" regnum="22" save-restore="yes" type="int8" group="system"/>
</feature>
</target>"#;

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
                if let Some(range) = cmd.strip_prefix("qXfer:features:read:target.xml:") {
                    let (offset, length) = range.split_once(',').unwrap_or(("0", "ffff"));
                    let offset = usize::from_str_radix(offset, 16).unwrap_or(0);
                    let length = usize::from_str_radix(length, 16).unwrap_or(TARGET_XML.len());
                    let start = offset.min(TARGET_XML.len());
                    let end = start.saturating_add(length).min(TARGET_XML.len());
                    let prefix = if end < TARGET_XML.len() { 'm' } else { 'l' };
                    return Some(format!("{prefix}{}", &TARGET_XML[start..end]));
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
                if cmd.starts_with("vCont;c") || cmd.starts_with("vCont;C") {
                    return self.continue_target().then(|| STOP_REPLY_TRAP.to_string());
                }
                if cmd.starts_with("vCont;s") || cmd.starts_with("vCont;S") {
                    self.target.step();
                    return Some(self.step_reply());
                }
                Some(String::new())
            }
            'c' => self.continue_target().then(|| STOP_REPLY_TRAP.to_string()),
            's' => {
                self.target.step();
                Some(self.step_reply())
            }
            'g' => {
                let mut bytes = Vec::with_capacity(80);
                for i in 0..16 {
                    bytes.extend_from_slice(&core.reg(i).to_le_bytes());
                }
                bytes.extend_from_slice(&core.xpsr().to_le_bytes());
                bytes.extend_from_slice(&core.msp().to_le_bytes());
                bytes.extend_from_slice(&core.psp().to_le_bytes());
                bytes.push(u8::from(core.pm()));
                bytes.push(0); // basepri
                bytes.push(0); // faultmask
                bytes.push(core.read_special_register(SYSM_CONTROL) as u8);
                Some(encode_hex_buf(&bytes))
            }
            'p' => {
                let index = usize::from_str_radix(cmd.get(1..).unwrap_or(""), 16).ok()?;
                let value = match index {
                    0..=15 => core.reg(index),
                    0x10 => core.xpsr(),
                    0x11 => core.msp(),
                    0x12 => core.psp(),
                    0x13 => return Some(encode_hex_buf(&[u8::from(core.pm())])),
                    0x14 | 0x15 => return Some(encode_hex_buf(&[0])),
                    0x16 => {
                        return Some(encode_hex_buf(&[
                            core.read_special_register(SYSM_CONTROL) as u8
                        ]))
                    }
                    _ => return Some(String::new()),
                };
                Some(encode_hex_u32_le(value))
            }
            'P' => {
                let (index, value) = cmd.get(1..)?.split_once('=')?;
                let index = usize::from_str_radix(index, 16).ok()?;
                let decoded = decode_hex_buf(value.trim());
                let expected_bytes = if (0x13..=0x16).contains(&index) { 1 } else { 4 };
                if decoded.len() != expected_bytes {
                    return Some("E00".into());
                }
                let mut raw = [0u8; 4];
                for (dst, src) in raw.iter_mut().zip(decoded.iter()) {
                    *dst = *src;
                }
                let value = u32::from_le_bytes(raw);
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

    fn continue_target(&mut self) -> bool {
        // A synchronous GDB target runs in bounded batches. Keep executing
        // until an actual BKPT or an explicit stop; a batch boundary alone is
        // not a stop event and must not be reported as SIGTRAP.
        const MAX_BATCHES: usize = 4;
        for _ in 0..MAX_BATCHES {
            self.target.execute();
            if self.target.last_break() {
                let rp = self.target.rp2040();
                let pc = rp.core.pc().wrapping_sub(rp.core.break_rewind());
                rp.core.set_pc(pc);
                return true;
            }
        }
        false
    }

    fn step_reply(&self) -> String {
        let rp = self.target.rp2040();
        if self.target.last_break() {
            let pc = rp.core.pc().wrapping_sub(rp.core.break_rewind());
            rp.core.set_pc(pc);
            return STOP_REPLY_TRAP.into();
        }
        let mut status = String::from("T05");
        for i in 0..16 {
            status.push_str(&format!("{:02x}:{};", i, encode_hex_u32_le(rp.core.reg(i))));
        }
        status.push_str(&format!("10:{};", encode_hex_u32_le(rp.core.xpsr())));
        status.push_str("reason:trace;");
        status
    }
}
