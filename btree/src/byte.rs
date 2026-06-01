#[inline]
pub fn read_u16(buf: &[u8], off: usize) -> u16 {
    let b0 = buf[off] as u16;
    let b1 = buf[off + 1] as u16;
    b0 | (b1 << 8)
}

#[inline]
pub fn read_u32(buf: &[u8], off: usize) -> u32 {
    let b0 = buf[off] as u32;
    let b1 = buf[off + 1] as u32;
    let b2 = buf[off + 2] as u32;
    let b3 = buf[off + 3] as u32;
    b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)
}

#[inline]
pub fn write_u16(buf: &mut [u8], off: usize, v: u16) {
    buf[off] = (v & 0xFF) as u8;
    buf[off + 1] = (v >> 8) as u8;
}

#[inline]
pub fn write_u32(buf: &mut [u8], off: usize, v: u32) {
    buf[off] = (v & 0xFF) as u8;
    buf[off + 1] = ((v >> 8) & 0xFF) as u8;
    buf[off + 2] = ((v >> 16) & 0xFF) as u8;
    buf[off + 3] = ((v >> 24) & 0xFF) as u8;
}
