//! BMP decode/encode (24-bit / 32-bit / 8-bit gray BI_RGB).

use crate::error::{VisionError, VisionResult};
use crate::image::{ColorMode, Image};

pub fn decode(bytes: &[u8]) -> VisionResult<Image> {
    if bytes.len() < 54 || bytes[0] != b'B' || bytes[1] != b'M' {
        return Err(VisionError::Codec("not BMP".into()));
    }
    let pixel_offset = u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize;
    let dib = u32::from_le_bytes(bytes[14..18].try_into().unwrap()) as usize;
    if dib < 40 {
        return Err(VisionError::Codec("unsupported BMP header".into()));
    }
    let width = i32::from_le_bytes(bytes[18..22].try_into().unwrap()).unsigned_abs() as usize;
    let height_signed = i32::from_le_bytes(bytes[22..26].try_into().unwrap());
    let bottom_up = height_signed > 0;
    let height = height_signed.unsigned_abs() as usize;
    let planes = u16::from_le_bytes(bytes[26..28].try_into().unwrap());
    let bpp = u16::from_le_bytes(bytes[28..30].try_into().unwrap());
    let compression = u32::from_le_bytes(bytes[30..34].try_into().unwrap());
    if planes != 1 || compression != 0 {
        return Err(VisionError::Codec("compressed BMP not supported".into()));
    }
    if pixel_offset > bytes.len() {
        return Err(VisionError::Codec("BMP pixel offset OOB".into()));
    }

    match bpp {
        24 => {
            let row_bytes = ((width * 3 + 3) / 4) * 4;
            let mut data = vec![0u8; height * width * 3];
            for y in 0..height {
                let src_y = if bottom_up { height - 1 - y } else { y };
                let row = pixel_offset + src_y * row_bytes;
                if row + width * 3 > bytes.len() {
                    return Err(VisionError::Codec("BMP truncated".into()));
                }
                for x in 0..width {
                    let s = row + x * 3;
                    let d = (y * width + x) * 3;
                    data[d] = bytes[s + 2];
                    data[d + 1] = bytes[s + 1];
                    data[d + 2] = bytes[s];
                }
            }
            Image::new(height, width, ColorMode::Rgb, data)
        }
        32 => {
            let row_bytes = width * 4;
            let mut data = vec![0u8; height * width * 4];
            for y in 0..height {
                let src_y = if bottom_up { height - 1 - y } else { y };
                let row = pixel_offset + src_y * row_bytes;
                for x in 0..width {
                    let s = row + x * 4;
                    let d = (y * width + x) * 4;
                    data[d] = bytes[s + 2];
                    data[d + 1] = bytes[s + 1];
                    data[d + 2] = bytes[s];
                    data[d + 3] = bytes[s + 3];
                }
            }
            Image::new(height, width, ColorMode::Rgba, data)
        }
        8 => {
            // grayscale or palette — treat as gray if no meaningful palette
            let row_bytes = ((width + 3) / 4) * 4;
            let mut data = vec![0u8; height * width];
            for y in 0..height {
                let src_y = if bottom_up { height - 1 - y } else { y };
                let row = pixel_offset + src_y * row_bytes;
                data[y * width..(y + 1) * width]
                    .copy_from_slice(&bytes[row..row + width]);
            }
            Image::new(height, width, ColorMode::Gray, data)
        }
        _ => Err(VisionError::Codec(format!("unsupported BMP bpp {bpp}"))),
    }
}

pub fn encode(img: &Image) -> VisionResult<Vec<u8>> {
    let (bpp, mode_data): (u16, Vec<u8>) = match img.mode {
        ColorMode::Gray => (8, img.data.clone()),
        ColorMode::Rgb => {
            let mut bgr = vec![0u8; img.data.len()];
            for i in 0..img.height * img.width {
                bgr[i * 3] = img.data[i * 3 + 2];
                bgr[i * 3 + 1] = img.data[i * 3 + 1];
                bgr[i * 3 + 2] = img.data[i * 3];
            }
            (24, bgr)
        }
        ColorMode::Rgba => {
            let mut bgra = vec![0u8; img.data.len()];
            for i in 0..img.height * img.width {
                bgra[i * 4] = img.data[i * 4 + 2];
                bgra[i * 4 + 1] = img.data[i * 4 + 1];
                bgra[i * 4 + 2] = img.data[i * 4];
                bgra[i * 4 + 3] = img.data[i * 4 + 3];
            }
            (32, bgra)
        }
    };

    let w = img.width;
    let h = img.height;
    let bytes_pp = (bpp / 8) as usize;
    let row_bytes = ((w * bytes_pp + 3) / 4) * 4;
    let pixel_size = row_bytes * h;
    let palette_size = if bpp == 8 { 256 * 4 } else { 0 };
    let pixel_offset = 54 + palette_size;
    let file_size = pixel_offset + pixel_size;

    let mut out = vec![0u8; file_size];
    out[0] = b'B';
    out[1] = b'M';
    out[2..6].copy_from_slice(&(file_size as u32).to_le_bytes());
    out[10..14].copy_from_slice(&(pixel_offset as u32).to_le_bytes());
    out[14..18].copy_from_slice(&40u32.to_le_bytes());
    out[18..22].copy_from_slice(&(w as i32).to_le_bytes());
    out[22..26].copy_from_slice(&(h as i32).to_le_bytes()); // bottom-up
    out[26..28].copy_from_slice(&1u16.to_le_bytes());
    out[28..30].copy_from_slice(&bpp.to_le_bytes());
    out[34..38].copy_from_slice(&(pixel_size as u32).to_le_bytes());

    if bpp == 8 {
        for i in 0..256 {
            let o = 54 + i * 4;
            out[o] = i as u8;
            out[o + 1] = i as u8;
            out[o + 2] = i as u8;
        }
    }

    for y in 0..h {
        let src_y = h - 1 - y;
        let dst_row = pixel_offset + y * row_bytes;
        let src = src_y * w * bytes_pp;
        out[dst_row..dst_row + w * bytes_pp]
            .copy_from_slice(&mode_data[src..src + w * bytes_pp]);
    }
    Ok(out)
}
