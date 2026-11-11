use crate::rp2040::{Memory, FLASH_START_ADDRESS};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    InvalidHex(String),
    InvalidUf2(String),
    AddressOutOfRange(u32),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::InvalidHex(s) => write!(f, "invalid Intel HEX: {s}"),
            LoadError::InvalidUf2(s) => write!(f, "invalid UF2: {s}"),
            LoadError::AddressOutOfRange(a) => write!(f, "image address out of flash: {a:#010x}"),
        }
    }
}

impl std::error::Error for LoadError {}

fn hex_u32(bytes: &[u8]) -> Result<u32, LoadError> {
    let text = std::str::from_utf8(bytes).map_err(|e| LoadError::InvalidHex(e.to_string()))?;
    u32::from_str_radix(text, 16).map_err(|_| LoadError::InvalidHex(text.into()))
}

fn hex_u8(bytes: &[u8]) -> Result<u8, LoadError> {
    let value = hex_u32(bytes)?;
    u8::try_from(value).map_err(|_| LoadError::InvalidHex("byte out of range".into()))
}

fn write_flash(flash: &Memory, address: u32, data: &[u8]) -> Result<usize, LoadError> {
    let offset = address
        .checked_sub(FLASH_START_ADDRESS)
        .ok_or(LoadError::AddressOutOfRange(address))? as usize;
    if offset
        .checked_add(data.len())
        .is_none_or(|end| end > flash.len())
    {
        return Err(LoadError::AddressOutOfRange(address));
    }
    flash.copy_from_slice(offset, data);
    Ok(data.len())
}

/// Load Intel HEX records into flash at `FLASH_START_ADDRESS`.
pub fn load_hex(source: &str, flash: &Memory) -> Result<usize, LoadError> {
    let mut upper = 0u32;
    let mut written = 0usize;
    for (line_no, raw) in source.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if !line.starts_with(':') {
            return Err(LoadError::InvalidHex(format!(
                "line {} missing ':'",
                line_no + 1
            )));
        }
        let digits = &line[1..];
        if digits.len() < 10 || digits.len() % 2 != 0 {
            return Err(LoadError::InvalidHex(format!(
                "line {} bad length",
                line_no + 1
            )));
        }
        let mut bytes = Vec::with_capacity(digits.len() / 2);
        for pair in digits.as_bytes().chunks(2) {
            bytes.push(hex_u8(pair)?);
        }
        let count = bytes[0] as usize;
        if bytes.len() != count + 5 {
            return Err(LoadError::InvalidHex(format!(
                "line {} bad record length",
                line_no + 1
            )));
        }
        if bytes.iter().fold(0u8, |sum, b| sum.wrapping_add(*b)) != 0 {
            return Err(LoadError::InvalidHex(format!(
                "line {} bad checksum",
                line_no + 1
            )));
        }
        let address = u16::from_be_bytes([bytes[1], bytes[2]]) as u32;
        let record_type = bytes[3];
        let data = &bytes[4..4 + count];
        match record_type {
            0x00 => {
                written += write_flash(flash, upper.wrapping_add(address), data)?;
            }
            0x01 => break,
            0x02 if data.len() == 2 => {
                upper = (u16::from_be_bytes([data[0], data[1]]) as u32) << 4;
            }
            0x04 if data.len() == 2 => {
                upper = (u16::from_be_bytes([data[0], data[1]]) as u32) << 16;
            }
            0x03 | 0x05 => {}
            other => {
                return Err(LoadError::InvalidHex(format!(
                    "unsupported record type {other:#04x}"
                )))
            }
        }
    }
    Ok(written)
}

fn u32_at(block: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        block[offset],
        block[offset + 1],
        block[offset + 2],
        block[offset + 3],
    ])
}

/// Load UF2 blocks into flash at their target addresses.
pub fn load_uf2(source: &[u8], flash: &Memory) -> Result<usize, LoadError> {
    const MAGIC0: u32 = 0x0A32_4655;
    const MAGIC1: u32 = 0x9E5D_5157;
    const MAGIC2: u32 = 0x0AB1_6F30;
    if source.is_empty() || !source.len().is_multiple_of(512) {
        return Err(LoadError::InvalidUf2(
            "file length is not a multiple of 512".into(),
        ));
    }
    let mut written = 0usize;
    for (index, block) in source.as_chunks::<512>().0.iter().enumerate() {
        if u32_at(block, 0) != MAGIC0 || u32_at(block, 4) != MAGIC1 || u32_at(block, 8) != MAGIC2 {
            return Err(LoadError::InvalidUf2(format!(
                "block {index} has bad magic"
            )));
        }
        let flash_address = u32_at(block, 12);
        let payload_size = u32_at(block, 16) as usize;
        if payload_size > 476 {
            return Err(LoadError::InvalidUf2(format!(
                "block {index} payload too large"
            )));
        }
        written += write_flash(flash, flash_address, &block[32..32 + payload_size])?;
    }
    Ok(written)
}
