//! ECMA-182 CRC-64 via the `crc` crate (ISO/IEC 3309 polynomial).

use crc::{Crc, CRC_64_XZ};

static CRC64: Crc<u64> = Crc::<u64>::new(&CRC_64_XZ);

#[inline]
pub fn crc64_update(crc: u64, data: &[u8]) -> u64 {
    let mut d = CRC64.digest_with_initial(crc);
    d.update(data);
    d.finalize()
}

#[inline]
pub fn crc64(data: &[u8]) -> u64 {
    CRC64.checksum(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ecma_vector() {
        assert_eq!(crc64(b"123456789"), 0x995D_C9BB_DF19_39FA);
    }
}
