//! IEEE CRC-32 (gzip/zip).

const TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            if c & 1 != 0 {
                c = 0xEDB8_8320 ^ (c >> 1);
            } else {
                c >>= 1;
            }
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
};

#[inline]
pub fn crc32_update(mut crc: u32, data: &[u8]) -> u32 {
    crc = !crc;
    for &b in data {
        crc = TABLE[((crc ^ u32::from(b)) & 0xFF) as usize] ^ (crc >> 8);
    }
    !crc
}

#[inline]
pub fn crc32(data: &[u8]) -> u32 {
    crc32_update(0, data)
}
