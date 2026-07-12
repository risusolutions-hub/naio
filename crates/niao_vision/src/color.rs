//! Color space conversions.

use crate::error::VisionResult;
use crate::image::{ColorMode, Image};
use crate::transform::to_grayscale;

pub fn cvt_color(img: &Image, to: ColorMode) -> VisionResult<Image> {
    match (img.mode, to) {
        (a, b) if a == b => Ok(img.clone()),
        (_, ColorMode::Gray) => to_grayscale(img),
        (ColorMode::Gray, ColorMode::Rgb) => {
            let mut data = vec![0u8; img.height * img.width * 3];
            for i in 0..img.data.len() {
                data[i * 3] = img.data[i];
                data[i * 3 + 1] = img.data[i];
                data[i * 3 + 2] = img.data[i];
            }
            Image::new(img.height, img.width, ColorMode::Rgb, data)
        }
        (ColorMode::Gray, ColorMode::Rgba) => {
            let mut data = vec![0u8; img.height * img.width * 4];
            for i in 0..img.data.len() {
                data[i * 4] = img.data[i];
                data[i * 4 + 1] = img.data[i];
                data[i * 4 + 2] = img.data[i];
                data[i * 4 + 3] = 255;
            }
            Image::new(img.height, img.width, ColorMode::Rgba, data)
        }
        (ColorMode::Rgb, ColorMode::Rgba) => {
            let mut data = vec![0u8; img.height * img.width * 4];
            for i in 0..img.height * img.width {
                data[i * 4..i * 4 + 3].copy_from_slice(&img.data[i * 3..i * 3 + 3]);
                data[i * 4 + 3] = 255;
            }
            Image::new(img.height, img.width, ColorMode::Rgba, data)
        }
        (ColorMode::Rgba, ColorMode::Rgb) => {
            let mut data = vec![0u8; img.height * img.width * 3];
            for i in 0..img.height * img.width {
                data[i * 3..i * 3 + 3].copy_from_slice(&img.data[i * 4..i * 4 + 3]);
            }
            Image::new(img.height, img.width, ColorMode::Rgb, data)
        }
        _ => to_grayscale(img).and_then(|g| cvt_color(&g, to)),
    }
}

pub fn rgb_to_hsv(img: &Image) -> VisionResult<Image> {
    let rgb = cvt_color(img, ColorMode::Rgb)?;
    let mut out = vec![0u8; rgb.data.len()];
    for i in 0..rgb.height * rgb.width {
        let r = rgb.data[i * 3] as f32 / 255.0;
        let g = rgb.data[i * 3 + 1] as f32 / 255.0;
        let b = rgb.data[i * 3 + 2] as f32 / 255.0;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        let h = if delta < 1e-8 {
            0.0
        } else if (max - r).abs() < 1e-8 {
            60.0 * (((g - b) / delta) % 6.0)
        } else if (max - g).abs() < 1e-8 {
            60.0 * (((b - r) / delta) + 2.0)
        } else {
            60.0 * (((r - g) / delta) + 4.0)
        };
        let h = if h < 0.0 { h + 360.0 } else { h };
        let s = if max < 1e-8 { 0.0 } else { delta / max };
        let v = max;
        out[i * 3] = (h / 360.0 * 255.0).round() as u8;
        out[i * 3 + 1] = (s * 255.0).round() as u8;
        out[i * 3 + 2] = (v * 255.0).round() as u8;
    }
    Image::new(rgb.height, rgb.width, ColorMode::Rgb, out)
}

pub fn rgb_to_ycbcr(img: &Image) -> VisionResult<Image> {
    let rgb = cvt_color(img, ColorMode::Rgb)?;
    let mut out = vec![0u8; rgb.data.len()];
    for i in 0..rgb.height * rgb.width {
        let r = rgb.data[i * 3] as f32;
        let g = rgb.data[i * 3 + 1] as f32;
        let b = rgb.data[i * 3 + 2] as f32;
        let y = 0.299 * r + 0.587 * g + 0.114 * b;
        let cb = 128.0 - 0.168736 * r - 0.331264 * g + 0.5 * b;
        let cr = 128.0 + 0.5 * r - 0.418688 * g - 0.081312 * b;
        out[i * 3] = y.round().clamp(0.0, 255.0) as u8;
        out[i * 3 + 1] = cb.round().clamp(0.0, 255.0) as u8;
        out[i * 3 + 2] = cr.round().clamp(0.0, 255.0) as u8;
    }
    Image::new(rgb.height, rgb.width, ColorMode::Rgb, out)
}
