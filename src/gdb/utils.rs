pub fn encode_hex_byte(value: u8) -> String {
    format!("{value:02x}")
}

pub fn encode_hex_buf(buf: &[u8]) -> String {
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn encode_hex_u32_le(value: u32) -> String {
    encode_hex_buf(&value.to_le_bytes())
}

pub fn decode_hex_buf(encoded: &str) -> Vec<u8> {
    let bytes = encoded.as_bytes();
    let mut result = Vec::with_capacity(bytes.len() / 2);
    let mut i = 0;
    while i + 1 < bytes.len() {
        let hi = (bytes[i] as char).to_digit(16).unwrap_or(0) as u8;
        let lo = (bytes[i + 1] as char).to_digit(16).unwrap_or(0) as u8;
        result.push((hi << 4) | lo);
        i += 2;
    }
    result
}

pub fn decode_hex_u32_le(encoded: &str) -> u32 {
    let bytes = decode_hex_buf(encoded);
    let mut raw = [0u8; 4];
    for (dst, src) in raw.iter_mut().zip(bytes.iter().take(4)) {
        *dst = *src;
    }
    u32::from_le_bytes(raw)
}

pub fn gdb_checksum(text: &str) -> String {
    let sum = text.bytes().fold(0u8, |acc, byte| acc.wrapping_add(byte));
    encode_hex_byte(sum)
}

pub fn gdb_message(value: &str) -> String {
    format!("${value}#{}", gdb_checksum(value))
}
