use crate::rp2040::Rp2040;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Registers {
    pub r0: u32,
    pub r1: u32,
    pub r2: u32,
    pub r3: u32,
    pub r4: u32,
    pub r5: u32,
    pub r6: u32,
    pub r7: u32,
    pub r8: u32,
    pub r9: u32,
    pub r10: u32,
    pub r11: u32,
    pub r12: u32,
    pub sp: u32,
    pub lr: u32,
    pub pc: u32,
    pub n: bool,
    pub z: bool,
    pub c: bool,
    pub v: bool,
}

pub struct TestDriver {
    pub rp2040: Rp2040,
}

impl Default for TestDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl TestDriver {
    pub fn new() -> Self {
        Self {
            rp2040: Rp2040::new(),
        }
    }

    pub fn set_pc(&self, value: u32) {
        self.rp2040.core.set_pc(value);
    }

    pub fn write_u8(&self, address: u32, value: u8) {
        self.rp2040.write_u8(address, value);
    }

    pub fn write_u16(&self, address: u32, value: u16) {
        self.rp2040.write_u16(address, value);
    }

    pub fn write_u32(&self, address: u32, value: u32) {
        self.rp2040.write_u32(address, value);
    }

    pub fn read_u8(&self, address: u32) -> u8 {
        self.rp2040.read_u8(address)
    }

    pub fn read_u16(&self, address: u32) -> u16 {
        self.rp2040.read_u16(address)
    }

    pub fn read_u32(&self, address: u32) -> u32 {
        self.rp2040.read_u32(address)
    }

    pub fn set_registers(&self, values: &[(usize, u32)]) {
        for (index, value) in values {
            self.rp2040.core.set_reg(*index, *value);
        }
    }

    pub fn set_reg(&self, index: usize, value: u32) {
        self.rp2040.core.set_reg(index, value);
    }

    pub fn reg(&self, index: usize) -> u32 {
        self.rp2040.core.reg(index)
    }

    pub fn set_flags(&self, n: bool, z: bool, c: bool, v: bool) {
        self.rp2040.core.set_n(n);
        self.rp2040.core.set_z(z);
        self.rp2040.core.set_c(c);
        self.rp2040.core.set_v(v);
    }

    pub fn set_c(&self, value: bool) {
        self.rp2040.core.set_c(value);
    }

    pub fn read_registers(&self) -> Registers {
        let r = self.rp2040.core.registers();
        Registers {
            r0: r[0],
            r1: r[1],
            r2: r[2],
            r3: r[3],
            r4: r[4],
            r5: r[5],
            r6: r[6],
            r7: r[7],
            r8: r[8],
            r9: r[9],
            r10: r[10],
            r11: r[11],
            r12: r[12],
            sp: r[13],
            lr: r[14],
            pc: r[15],
            n: self.rp2040.core.n(),
            z: self.rp2040.core.z(),
            c: self.rp2040.core.c(),
            v: self.rp2040.core.v(),
        }
    }

    pub fn single_step(&self) {
        self.rp2040.core.step();
    }

    pub fn tear_down(&self) {}
}
