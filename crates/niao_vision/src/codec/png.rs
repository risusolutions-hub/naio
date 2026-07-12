//! PNG decode/encode (8-bit gray/RGB/RGBA, non-interlaced).

use super::{crc32, zlib_decode, zlib_encode};
use crate::error::{VisionError, VisionResult};
use crate::image::{ColorMode, Image};

pub fn decode(bytes: &[u8]) -> VisionResult<Image> {
    if bytes.len() < 8 || &bytes[0..8] != b"\x89PNG\r\n\x1a\n" {
        return Err(VisionError::Codec("not PNG".into()));
    }
    let mut pos = 8usize;
    let mut width = 0u32;
    let mut height = 0u32;
    let mut color_type = 0u8;
    let mut idat = Vec::new();
    let mut saw_ihdr = false;

    while pos + 12 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
        let ctype = &bytes[pos + 4..pos + 8];
        let data_start = pos + 8;
        let data_end = data_start + len;
        if data_end + 4 > bytes.len() {
            return Err(VisionError::Codec("PNG chunk truncated".into()));
        }
        let chunk_data = &bytes[data_start..data_end];
        let mut crc_buf = Vec::with_capacity(4 + len);
        crc_buf.extend_from_slice(ctype);
        crc_buf.extend_from_slice(chunk_data);
        let expect = u32::from_be_bytes(bytes[data_end..data_end + 4].try_into().unwrap());
        if crc32(&crc_buf) != expect {
            return Err(VisionError::Codec("PNG CRC mismatch".into()));
        }

        match ctype {
            b"IHDR" => {
                if len < 13 {
                    return Err(VisionError::Codec("IHDR too short".into()));
                }
                width = u32::from_be_bytes(chunk_data[0..4].try_into().unwrap());
                height = u32::from_be_bytes(chunk_data[4..8].try_into().unwrap());
                let depth = chunk_data[8];
                color_type = chunk_data[9];
                if chunk_data[10] != 0 || chunk_data[12] != 0 {
                    return Err(VisionError::Codec("unsupported PNG compression/interlace".into()));
                }
                if depth != 8 {
                    return Err(VisionError::Codec("only 8-bit PNG supported".into()));
                }
                saw_ihdr = true;
            }
            b"IDAT" => idat.extend_from_slice(chunk_data),
            b"IEND" => break,
            _ => {}
        }
        pos = data_end + 4;
    }

    if !saw_ihdr {
        return Err(VisionError::Codec("missing IHDR".into()));
    }
    let w = width as usize;
    let h = height as usize;
    let (mode, bpp) = match color_type {
        0 => (ColorMode::Gray, 1usize),
        2 => (ColorMode::Rgb, 3),
        6 => (ColorMode::Rgba, 4),
        4 => {
            // gray+alpha → RGBA
            let raw = unfilter(zlib_decode(&idat)?, w, h, 2)?;
            let mut rgba = vec![0u8; w * h * 4];
            for i in 0..w * h {
                let g = raw[i * 2];
                let a = raw[i * 2 + 1];
                rgba[i * 4] = g;
                rgba[i * 4 + 1] = g;
                rgba[i * 4 + 2] = g;
                rgba[i * 4 + 3] = a;
            }
            return Image::new(h, w, ColorMode::Rgba, rgba);
        }
        _ => {
            return Err(VisionError::Codec(format!(
                "unsupported PNG color type {color_type}"
            )))
        }
    };

    let raw = unfilter(zlib_decode(&idat)?, w, h, bpp)?;
    Image::new(h, w, mode, raw)
}

fn unfilter(filtered: Vec<u8>, w: usize, h: usize, bpp: usize) -> VisionResult<Vec<u8>> {
    let stride = w * bpp;
    let expect = h * (1 + stride);
    if filtered.len() < expect {
        return Err(VisionError::Codec(format!(
            "PNG data short: {} < {expect}",
            filtered.len()
        )));
    }
    let mut out = vec![0u8; h * stride];
    let mut prev = vec![0u8; stride];
    for row in 0..h {
        let src = row * (1 + stride);
        let filter = filtered[src];
        let cur = &filtered[src + 1..src + 1 + stride];
        let dst_row = &mut out[row * stride..(row + 1) * stride];
        match filter {
            0 => dst_row.copy_from_slice(cur),
            1 => {
                for i in 0..stride {
                    let left = if i >= bpp { dst_row[i - bpp] } else { 0 };
                    dst_row[i] = cur[i].wrapping_add(left);
                }
            }
            2 => {
                for i in 0..stride {
                    dst_row[i] = cur[i].wrapping_add(prev[i]);
                }
            }
            3 => {
                for i in 0..stride {
                    let left = if i >= bpp { dst_row[i - bpp] } else { 0 };
                    let up = prev[i];
                    dst_row[i] = cur[i].wrapping_add(((u16::from(left) + u16::from(up)) / 2) as u8);
                }
            }
            4 => {
                for i in 0..stride {
                    let left = if i >= bpp { dst_row[i - bpp] } else { 0 };
                    let up = prev[i];
                    let up_left = if i >= bpp { prev[i - bpp] } else { 0 };
                    dst_row[i] = cur[i].wrapping_add(paeth(left, up, up_left));
                }
            }
            _ => return Err(VisionError::Codec(format!("bad PNG filter {filter}"))),
        }
        prev.copy_from_slice(dst_row);
    }
    Ok(out)
}

#[inline]
fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let a = i16::from(a);
    let b = i16::from(b);
    let c = i16::from(c);
    let p = a + b - c;
    let pa = (p - a).abs();
    let pb = (p - b).abs();
    let pc = (p - c).abs();
    if pa <= pb && pa <= pc {
        a as u8
    } else if pb <= pc {
        b as u8
    } else {
        c as u8
    }
}

pub fn encode(img: &Image) -> VisionResult<Vec<u8>> {
    let (color_type, bpp) = match img.mode {
        ColorMode::Gray => (0u8, 1usize),
        ColorMode::Rgb => (2, 3),
        ColorMode::Rgba => (6, 4),
    };
    let w = img.width;
    let h = img.height;
    let stride = w * bpp;
    let mut filtered = Vec::with_capacity(h * (1 + stride));
    for row in 0..h {
        filtered.push(0); // None filter
        let start = row * stride;
        filtered.extend_from_slice(&img.data[start..start + stride]);
    }
    let compressed = zlib_encode(&filtered)?;

    let mut out = Vec::with_capacity(compressed.len() + 128);
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");

    let mut ihdr = [0u8; 13];
    ihdr[0..4].copy_from_slice(&(w as u32).to_be_bytes());
    ihdr[4..8].copy_from_slice(&(h as u32).to_be_bytes());
    ihdr[8] = 8;
    ihdr[9] = color_type;
    write_chunk(&mut out, b"IHDR", &ihdr);
    write_chunk(&mut out, b"IDAT", &compressed);
    write_chunk(&mut out, b"IEND", &[]);
    Ok(out)
}

fn write_chunk(out: &mut Vec<u8>, ctype: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(ctype);
    out.extend_from_slice(data);
    let mut crc_buf = Vec::with_capacity(4 + data.len());
    crc_buf.extend_from_slice(ctype);
    crc_buf.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_buf).to_be_bytes());
}
