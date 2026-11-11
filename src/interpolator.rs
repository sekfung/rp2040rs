fn s32(value: u32) -> i32 {
    value as i32
}

fn u32(value: i32) -> u32 {
    value as u32
}

#[derive(Clone, Copy, Default)]
struct Config {
    shift: u32,
    mask_lsb: u32,
    mask_msb: u32,
    signed: bool,
    cross_input: bool,
    cross_result: bool,
    add_raw: bool,
    force_msb: u32,
    blend: bool,
    clamp: bool,
    overf0: bool,
    overf1: bool,
    overf: bool,
}

impl Config {
    fn from(value: u32) -> Self {
        Self {
            shift: value & 0x1f,
            mask_lsb: (value >> 5) & 0x1f,
            mask_msb: (value >> 10) & 0x1f,
            signed: (value >> 15) & 1 != 0,
            cross_input: (value >> 16) & 1 != 0,
            cross_result: (value >> 17) & 1 != 0,
            add_raw: (value >> 18) & 1 != 0,
            force_msb: (value >> 19) & 0b11,
            blend: (value >> 21) & 1 != 0,
            clamp: (value >> 22) & 1 != 0,
            overf0: (value >> 23) & 1 != 0,
            overf1: (value >> 24) & 1 != 0,
            overf: (value >> 25) & 1 != 0,
        }
    }

    fn to_u32(self) -> u32 {
        self.shift
            | (self.mask_lsb << 5)
            | (self.mask_msb << 10)
            | ((self.signed as u32) << 15)
            | ((self.cross_input as u32) << 16)
            | ((self.cross_result as u32) << 17)
            | ((self.add_raw as u32) << 18)
            | (self.force_msb << 19)
            | ((self.blend as u32) << 21)
            | ((self.clamp as u32) << 22)
            | ((self.overf0 as u32) << 23)
            | ((self.overf1 as u32) << 24)
            | ((self.overf as u32) << 25)
    }
}

#[derive(Clone)]
pub struct Interpolator {
    index: usize,
    pub accum0: u32,
    pub accum1: u32,
    pub base0: u32,
    pub base1: u32,
    pub base2: u32,
    pub ctrl0: u32,
    pub ctrl1: u32,
    pub result0: u32,
    pub result1: u32,
    pub result2: u32,
    pub smresult0: u32,
    pub smresult1: u32,
}

impl Interpolator {
    pub fn new(index: usize) -> Self {
        let mut value = Self {
            index,
            accum0: 0,
            accum1: 0,
            base0: 0,
            base1: 0,
            base2: 0,
            ctrl0: 0,
            ctrl1: 0,
            result0: 0,
            result1: 0,
            result2: 0,
            smresult0: 0,
            smresult1: 0,
        };
        value.update();
        value
    }

    pub fn update(&mut self) {
        let mut c0 = Config::from(self.ctrl0);
        let mut c1 = Config::from(self.ctrl1);
        let do_clamp = c0.clamp && self.index == 1;
        let do_blend = c0.blend && self.index == 0;
        c0.clamp = do_clamp;
        c0.blend = do_blend;
        c1.clamp = false;
        c1.blend = false;
        c1.overf0 = false;
        c1.overf1 = false;
        c1.overf = false;

        let uinput0 = if c0.cross_input {
            self.accum1
        } else {
            self.accum0
        };
        let uinput1 = if c1.cross_input {
            self.accum0
        } else {
            self.accum1
        };
        let input0 = s32(uinput0);
        let input1 = s32(uinput1);

        let msbmask0 = if c0.mask_msb == 31 {
            0xffff_ffff
        } else {
            (1u32 << (c0.mask_msb + 1)).wrapping_sub(1)
        };
        let msbmask1 = if c1.mask_msb == 31 {
            0xffff_ffff
        } else {
            (1u32 << (c1.mask_msb + 1)).wrapping_sub(1)
        };
        let mask0 = msbmask0 & !(1u32.wrapping_shl(c0.mask_lsb).wrapping_sub(1));
        let mask1 = msbmask1 & !(1u32.wrapping_shl(c1.mask_lsb).wrapping_sub(1));

        let uresult0 = (uinput0 >> c0.shift) & mask0;
        let uresult1 = (uinput1 >> c1.shift) & mask1;
        let overf0 = ((uinput0 >> c0.shift) & !msbmask0) != 0;
        let overf1 = ((uinput1 >> c1.shift) & !msbmask1) != 0;
        let overf = overf0 || overf1;

        let sextmask0 = if uresult0 & (1u32 << c0.mask_msb) != 0 {
            0xffff_ffffu32 << c0.mask_msb
        } else {
            0
        };
        let sextmask1 = if uresult1 & (1u32 << c1.mask_msb) != 0 {
            0xffff_ffffu32 << c1.mask_msb
        } else {
            0
        };
        let sresult0 = uresult0 | sextmask0;
        let sresult1 = uresult1 | sextmask1;
        let result0 = if c0.signed { sresult0 } else { uresult0 };
        let result1 = if c1.signed { sresult1 } else { uresult1 };

        let addresult0 = self
            .base0
            .wrapping_add(if c0.add_raw { input0 as u32 } else { result0 });
        let addresult1 = self
            .base1
            .wrapping_add(if c1.add_raw { input1 as u32 } else { result1 });
        let addresult2 =
            self.base2
                .wrapping_add(result0)
                .wrapping_add(if do_blend { 0 } else { result1 });

        let uclamp0 = if result0 < self.base0 {
            self.base0
        } else if result0 > self.base1 {
            self.base1
        } else {
            result0
        };
        let sclamp0 = if s32(result0) < s32(self.base0) {
            self.base0
        } else if s32(result0) > s32(self.base1) {
            self.base1
        } else {
            result0
        };
        let clamp0 = if c0.signed { sclamp0 } else { uclamp0 };

        let alpha1 = result1 & 0xff;
        let delta = if c1.signed {
            s32(self.base1).wrapping_sub(s32(self.base0))
        } else {
            self.base1.wrapping_sub(self.base0) as i32
        };
        let blend_value =
            s32(self.base0).wrapping_add((alpha1 as i64 * delta as i64).div_euclid(256) as i32);
        let blend1 = if c1.signed {
            u32(blend_value)
        } else {
            self.base0.wrapping_add(
                (alpha1 as u64 * self.base1.wrapping_sub(self.base0) as u64 / 256) as u32,
            )
        };

        self.smresult0 = result0;
        self.smresult1 = result1;
        self.result0 = if do_blend {
            alpha1
        } else if do_clamp {
            clamp0
        } else {
            addresult0
        } | (c0.force_msb << 28);
        self.result1 = if do_blend { blend1 } else { addresult1 } | (c0.force_msb << 28);
        self.result2 = addresult2;

        c0.overf0 = overf0;
        c0.overf1 = overf1;
        c0.overf = overf;
        self.ctrl0 = c0.to_u32();
        self.ctrl1 = c1.to_u32();
    }

    pub fn writeback(&mut self) {
        let c0 = Config::from(self.ctrl0);
        let c1 = Config::from(self.ctrl1);
        self.accum0 = if c0.cross_result {
            self.result1
        } else {
            self.result0
        };
        self.accum1 = if c1.cross_result {
            self.result0
        } else {
            self.result1
        };
        self.update();
    }

    pub fn set_base01(&mut self, value: u32) {
        let c0 = Config::from(self.ctrl0);
        let c1 = Config::from(self.ctrl1);
        let do_blend = c0.blend && self.index == 0;
        let input0 = value & 0xffff;
        let input1 = (value >> 16) & 0xffff;
        let sextmask0 = if input0 & (1 << 15) != 0 {
            0xffff_0000
        } else {
            0
        };
        let sextmask1 = if input1 & (1 << 15) != 0 {
            0xffff_0000
        } else {
            0
        };
        self.base0 = if if do_blend { c1.signed } else { c0.signed } {
            input0 | sextmask0
        } else {
            input0
        };
        self.base1 = if c1.signed {
            input1 | sextmask1
        } else {
            input1
        };
        self.update();
    }
}
