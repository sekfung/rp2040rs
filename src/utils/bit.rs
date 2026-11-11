pub fn bit(n: u32) -> u32 {
    1u32.wrapping_shl(n)
}

pub fn s32(n: u32) -> i32 {
    n as i32
}

pub fn u32(n: i32) -> u32 {
    n as u32
}
