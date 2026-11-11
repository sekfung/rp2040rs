pub const PIO_SRC_PINS: u32 = 0;
pub const PIO_SRC_X: u32 = 1;
pub const PIO_SRC_Y: u32 = 2;
pub const PIO_SRC_NULL: u32 = 3;
pub const PIO_SRC_STATUS: u32 = 5;
pub const PIO_SRC_ISR: u32 = 6;
pub const PIO_SRC_OSR: u32 = 7;

pub const PIO_DEST_PINS: u32 = 0;
pub const PIO_DEST_X: u32 = 1;
pub const PIO_DEST_Y: u32 = 2;
pub const PIO_DEST_NULL: u32 = 3;
pub const PIO_DEST_PINDIRS: u32 = 4;
pub const PIO_DEST_PC: u32 = 5;
pub const PIO_DEST_ISR: u32 = 6;
pub const PIO_DEST_EXEC: u32 = 7;

pub const PIO_MOV_DEST_PINS: u32 = 0;
pub const PIO_MOV_DEST_X: u32 = 1;
pub const PIO_MOV_DEST_Y: u32 = 2;
pub const PIO_MOV_DEST_EXEC: u32 = 4;
pub const PIO_MOV_DEST_PC: u32 = 5;
pub const PIO_MOV_DEST_ISR: u32 = 6;
pub const PIO_MOV_DEST_OSR: u32 = 7;

pub const PIO_OP_NONE: u32 = 0;
pub const PIO_OP_INVERT: u32 = 1;
pub const PIO_OP_BITREV: u32 = 2;

pub const PIO_WAIT_SRC_GPIO: u32 = 0;
pub const PIO_WAIT_SRC_PIN: u32 = 1;
pub const PIO_WAIT_SRC_IRQ: u32 = 2;

pub const PIO_COND_ALWAYS: u32 = 0;
pub const PIO_COND_NOTX: u32 = 1;
pub const PIO_COND_XDEC: u32 = 2;
pub const PIO_COND_NOTY: u32 = 3;
pub const PIO_COND_YDEC: u32 = 4;
pub const PIO_COND_XNEY: u32 = 5;
pub const PIO_COND_PIN: u32 = 6;
pub const PIO_COND_NOTEMPTYOSR: u32 = 7;

pub fn pio_jmp(cond: u32, address: u32, delay: u32) -> u16 {
    (((delay & 0x1f) << 8) | ((cond & 0x7) << 5) | (address & 0x1f)) as u16
}

pub fn pio_wait(polarity: bool, src: u32, index: u32, delay: u32) -> u16 {
    ((1 << 13)
        | ((delay & 0x1f) << 8)
        | ((polarity as u32) << 7)
        | ((src & 0x3) << 5)
        | (index & 0x1f)) as u16
}

pub fn pio_in(src: u32, bit_count: u32, delay: u32) -> u16 {
    ((2 << 13) | ((delay & 0x1f) << 8) | ((src & 0x7) << 5) | (bit_count & 0x1f)) as u16
}

pub fn pio_out(dest: u32, bit_count: u32, delay: u32) -> u16 {
    ((3 << 13) | ((delay & 0x1f) << 8) | ((dest & 0x7) << 5) | (bit_count & 0x1f)) as u16
}

pub fn pio_push(if_full: bool, no_block: bool, delay: u32) -> u16 {
    ((4 << 13) | ((delay & 0x1f) << 8) | ((if_full as u32) << 6) | ((no_block as u32) << 5)) as u16
}

pub fn pio_pull(if_empty: bool, no_block: bool, delay: u32) -> u16 {
    ((4 << 13)
        | ((delay & 0x1f) << 8)
        | (1 << 7)
        | ((if_empty as u32) << 6)
        | ((no_block as u32) << 5)) as u16
}

pub fn pio_mov(dest: u32, op: u32, src: u32, delay: u32) -> u16 {
    ((5 << 13) | ((delay & 0x1f) << 8) | ((dest & 0x7) << 5) | ((op & 0x3) << 3) | (src & 0x7))
        as u16
}

pub fn pio_irq(clear: bool, wait: bool, index: u32, delay: u32) -> u16 {
    ((6 << 13)
        | ((delay & 0x1f) << 8)
        | ((clear as u32) << 6)
        | ((wait as u32) << 5)
        | (index & 0x1f)) as u16
}

pub fn pio_set(dest: u32, data: u32, delay: u32) -> u16 {
    ((7 << 13) | ((delay & 0x1f) << 8) | ((dest & 0x7) << 5) | (data & 0x1f)) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_correctly_encode_a_jmp_pin_5_pio_instruction() {
        assert_eq!(pio_jmp(PIO_COND_PIN, 5, 0), 0xc5);
    }

    #[test]
    fn should_correctly_encode_a_wait_1_gpio_12_pio_instruction() {
        assert_eq!(pio_wait(true, PIO_WAIT_SRC_GPIO, 12, 0), 0x208c);
    }

    #[test]
    fn should_correctly_encode_an_in_x_12_pio_instruction() {
        assert_eq!(pio_in(PIO_SRC_X, 12, 0), 0x402c);
    }

    #[test]
    fn should_correctly_encode_an_out_y_30_pio_instruction() {
        assert_eq!(pio_out(PIO_DEST_Y, 30, 0), 0x605e);
    }

    #[test]
    fn should_correctly_encode_a_push_iffull_noblock_pio_instruction() {
        assert_eq!(pio_push(true, true, 12), 0x8c60);
    }

    #[test]
    fn should_correctly_encode_a_pull_block_pio_instruction() {
        assert_eq!(pio_pull(true, false, 0), 0x80c0);
    }

    #[test]
    fn should_correctly_encode_a_mov_x_not_status_pio_instruction() {
        assert_eq!(
            pio_mov(PIO_MOV_DEST_X, PIO_OP_INVERT, PIO_SRC_STATUS, 0),
            0xa02d
        );
    }

    #[test]
    fn should_correctly_encode_an_irq_set_4_pio_instruction() {
        assert_eq!(pio_irq(false, false, 4, 0), 0xc004);
    }

    #[test]
    fn should_correctly_encode_a_set_x_12_pio_instruction() {
        assert_eq!(pio_set(PIO_MOV_DEST_X, 12, 0), 0xe02c);
    }
}
