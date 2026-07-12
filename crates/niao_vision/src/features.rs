//! Lightweight features: Harris corners, HOG, template matching.

use crate::error::VisionResult;
use crate::image::{ColorMode, Image};
use crate::ops::{gaussian_blur, sobel};
use crate::transform::to_grayscale;

pub fn harris_corners(img: &Image, k: f32, threshold: f32) -> VisionResult<Vec<(usize, usize)>> {
    let gray = to_grayscale(img)?;
    let (gx, gy) = sobel(&gray)?;
    let w = gray.width;
    let h = gray.height;
    let mut rmap = vec![0.0f32; w * h];
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let mut ixx = 0.0f32;
            let mut iyy = 0.0f32;
            let mut ixy = 0.0f32;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let i = ((y as isize + dy) as usize) * w + (x as isize + dx) as usize;
                    let ix = gx.data[i] as f32 - 128.0;
                    let iy = gy.data[i] as f32 - 128.0;
                    ixx += ix * ix;
                    iyy += iy * iy;
                    ixy += ix * iy;
                }
            }
            let det = ixx * iyy - ixy * ixy;
            let trace = ixx + iyy;
            rmap[y * w + x] = det - k * trace * trace;
        }
    }
    let mut corners = Vec::new();
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let v = rmap[y * w + x];
            if v < threshold {
                continue;
            }
            let mut is_max = true;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if dy == 0 && dx == 0 {
                        continue;
                    }
                    if rmap[((y as isize + dy) as usize) * w + (x as isize + dx) as usize] >= v {
                        is_max = false;
                    }
                }
            }
            if is_max {
                corners.push((x, y));
            }
        }
    }
    Ok(corners)
}

/// Simple HOG: unsigned gradients, 9 bins, 8×8 cells.
pub fn hog(img: &Image) -> VisionResult<Vec<f32>> {
    let gray = to_grayscale(img)?;
    let blur = gaussian_blur(&gray, 3, 0.5)?;
    let (gx, gy) = sobel(&blur)?;
    let cell = 8usize;
    let bins = 9usize;
    let cells_x = gray.width / cell;
    let cells_y = gray.height / cell;
    let mut hist = vec![0.0f32; cells_y * cells_x * bins];
    for cy in 0..cells_y {
        for cx in 0..cells_x {
            let base = (cy * cells_x + cx) * bins;
            for y in cy * cell..(cy + 1) * cell {
                for x in cx * cell..(cx + 1) * cell {
                    let i = y * gray.width + x;
                    let dx = gx.data[i] as f32 - 128.0;
                    let dy = gy.data[i] as f32 - 128.0;
                    let mag = (dx * dx + dy * dy).sqrt();
                    let ang = dy.atan2(dx).to_degrees().rem_euclid(180.0);
                    let bin_f = ang / 20.0;
                    let b0 = bin_f.floor() as usize % bins;
                    let b1 = (b0 + 1) % bins;
                    let t = bin_f - bin_f.floor();
                    hist[base + b0] += mag * (1.0 - t);
                    hist[base + b1] += mag * t;
                }
            }
        }
    }
    // L2 block normalize 2×2
    let mut out = hist.clone();
    for cy in 0..cells_y.saturating_sub(1) {
        for cx in 0..cells_x.saturating_sub(1) {
            let mut norm = 1e-6f32;
            for dy in 0..2 {
                for dx in 0..2 {
                    let base = ((cy + dy) * cells_x + (cx + dx)) * bins;
                    for b in 0..bins {
                        norm += hist[base + b] * hist[base + b];
                    }
                }
            }
            norm = norm.sqrt();
            for dy in 0..2 {
                for dx in 0..2 {
                    let base = ((cy + dy) * cells_x + (cx + dx)) * bins;
                    for b in 0..bins {
                        out[base + b] = hist[base + b] / norm;
                    }
                }
            }
        }
    }
    Ok(out)
}

/// Normalized cross-correlation template match. Returns (x, y, score).
pub fn match_template(img: &Image, templ: &Image) -> VisionResult<(usize, usize, f32)> {
    let a = to_grayscale(img)?;
    let t = to_grayscale(templ)?;
    if t.width > a.width || t.height > a.height {
        return Err(crate::error::VisionError::Shape(
            "template larger than image".into(),
        ));
    }
    let mut best = (0usize, 0usize, f32::NEG_INFINITY);
    let tw = t.width;
    let th = t.height;
    let mut t_mean = 0.0f32;
    for &v in &t.data {
        t_mean += v as f32;
    }
    t_mean /= t.data.len() as f32;
    let mut t_var = 0.0f32;
    for &v in &t.data {
        let d = v as f32 - t_mean;
        t_var += d * d;
    }
    t_var = t_var.sqrt().max(1e-6);

    for y in 0..=a.height - th {
        for x in 0..=a.width - tw {
            let mut mean = 0.0f32;
            for yy in 0..th {
                for xx in 0..tw {
                    mean += a.data[(y + yy) * a.width + (x + xx)] as f32;
                }
            }
            mean /= (tw * th) as f32;
            let mut num = 0.0f32;
            let mut den = 0.0f32;
            for yy in 0..th {
                for xx in 0..tw {
                    let av = a.data[(y + yy) * a.width + (x + xx)] as f32 - mean;
                    let tv = t.data[yy * tw + xx] as f32 - t_mean;
                    num += av * tv;
                    den += av * av;
                }
            }
            let score = num / (den.sqrt().max(1e-6) * t_var);
            if score > best.2 {
                best = (x, y, score);
            }
        }
    }
    Ok(best)
}

/// Dummy to keep ColorMode import used when only features used.
#[allow(dead_code)]
fn _mode() -> ColorMode {
    ColorMode::Gray
}
