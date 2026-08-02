//! Binary primitives for Niao: struct pack/unpack, bitstrings, varints, CRC.

pub mod bitstring;
pub mod crc64;
pub mod endian;
pub mod struct_fmt;
pub mod varint;

pub use bitstring::BitString;
pub use crc64::{crc64, crc64_update};
pub use endian::Endian;
pub use struct_fmt::{CompiledStruct, PackValue, UnpackValue};
pub use varint::{
    uvarint_decode, uvarint_encode, varint_decode, varint_encode, zigzag_decode, zigzag_encode,
};

/// IEEE CRC-32 (gzip/zip polynomial) via hardware-accelerated `crc32fast`.
#[inline]
pub fn crc32(data: &[u8]) -> u32 {
    crc32fast::hash(data)
}

/// Incremental CRC-32; `crc` is the running value (use `0` to start).
#[inline]
pub fn crc32_update(crc: u32, data: &[u8]) -> u32 {
    let mut h = crc32fast::Hasher::new_with_initial(crc);
    h.update(data);
    h.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_known() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }
}
