use crate::bail;
use crate::error::Result;

pub fn u16le(d: &[u8], o: usize) -> Result<u16> {
    match d.get(o..o + 2) {
        Some(b) => Ok(u16::from_le_bytes([b[0], b[1]])),
        None => bail!("unexpected end of data at offset {}", o),
    }
}
pub fn u32le(d: &[u8], o: usize) -> Result<u32> {
    match d.get(o..o + 4) {
        Some(b) => Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        None => bail!("unexpected end of data at offset {}", o),
    }
}
pub fn i32le(d: &[u8], o: usize) -> Result<i32> {
    Ok(u32le(d, o)? as i32)
}
