//! Image codecs (PNG / BMP / baseline JPEG).
//!
//! Spec routes codecs through `ncodec`; that crate currently exposes only base64/hex/UUID.
//! Codecs live here and depend on `niao_archive` for zlib. Orchestrator may migrate to
//! `niao_codec::image` later. `niao_codec` remains a declared dependency for the contract.

pub mod bmp;
pub mod jpeg;
pub mod png;

use crate::error::{VisionError, VisionResult};
use crate::image::Image;

/// Touch `niao_codec` so the declared dependency stays linked for the ncodec contract.
#[inline]
pub fn _codec_contract_hex(bytes: &[u8]) -> String {
    niao_codec::hex_encode(bytes)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Bmp,
}

pub fn detect_format(bytes: &[u8]) -> VisionResult<ImageFormat> {
    if bytes.len() >= 8 && &bytes[0..8] == b"\x89PNG\r\n\x1a\n" {
        return Ok(ImageFormat::Png);
    }
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
        return Ok(ImageFormat::Jpeg);
    }
    if bytes.len() >= 2 && bytes[0] == b'B' && bytes[1] == b'M' {
        return Ok(ImageFormat::Bmp);
    }
    Err(VisionError::Codec("unrecognized image format".into()))
}

pub fn decode(bytes: &[u8]) -> VisionResult<Image> {
    match detect_format(bytes)? {
        ImageFormat::Png => png::decode(bytes),
        ImageFormat::Jpeg => jpeg::decode(bytes),
        ImageFormat::Bmp => bmp::decode(bytes),
    }
}

pub fn encode(img: &Image, format: ImageFormat) -> VisionResult<Vec<u8>> {
    match format {
        ImageFormat::Png => png::encode(img),
        ImageFormat::Jpeg => jpeg::encode(img, 90),
        ImageFormat::Bmp => bmp::encode(img),
    }
}

/// zlib inflate for PNG IDAT (CMF/FLG + deflate + Adler-32).
pub(crate) fn zlib_decode(data: &[u8]) -> VisionResult<Vec<u8>> {
    if data.len() < 6 {
        return Err(VisionError::Codec("zlib stream too short".into()));
    }
    let cmf = data[0] as u16;
    let flg = data[1] as u16;
    if (cmf * 256 + flg) % 31 != 0 {
        return Err(VisionError::Codec("zlib header checksum".into()));
    }
    let mut start = 2usize;
    if (flg & 0x20) != 0 {
        if data.len() < 10 {
            return Err(VisionError::Codec("zlib dict truncated".into()));
        }
        start = 6;
    }
    if data.len() < start + 4 {
        return Err(VisionError::Codec("zlib missing adler".into()));
    }
    let end = data.len() - 4;
    niao_archive::inflate(&data[start..end])
        .map_err(|e| VisionError::Codec(format!("zlib inflate: {e}")))
}

pub(crate) fn zlib_encode(data: &[u8]) -> VisionResult<Vec<u8>> {
    niao_archive::zlib_encode(data).map_err(|e| VisionError::Codec(format!("zlib encode: {e}")))
}

/// IEEE CRC-32 (PNG chunk CRC).
pub(crate) fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in data {
        let idx = ((crc ^ u32::from(b)) & 0xFF) as usize;
        crc = CRC_TABLE[idx] ^ (crc >> 8);
    }
    !crc
}

static CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            if c & 1 != 0 {
                c = 0xEDB8_8320 ^ (c >> 1);
            } else {
                c >>= 1;
            }
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
};
