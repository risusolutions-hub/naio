//! Geometric + photometric transforms and Compose pipelines.
//!
//! Interpolation: **half-pixel / align_corners=False** (torchvision default for resize).
//! Not bit-identical to PIL; compare with rtol≈1e-4.

use crate::error::{VisionError, VisionResult};
use crate::image::{normalize_tensor, ColorMode, Image};
use niao_rand::{Rng, SeedableRng, StdRng};
use niao_tensor::Tensor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interp {
    Nearest,
    Bilinear,
    Bicubic,
}

pub trait Transform: Send + Sync {
    fn apply(&self, img: &Image) -> VisionResult<Image>;
}

/// Sequential torchvision-style pipeline.
pub struct Compose {
    pub steps: Vec<Box<dyn Transform>>,
}

impl Compose {
    pub fn new(steps: Vec<Box<dyn Transform>>) -> Self {
        Self { steps }
    }

    pub fn apply(&self, img: &Image) -> VisionResult<Image> {
        let mut cur = img.clone();
        for s in &self.steps {
            cur = s.apply(&cur)?;
        }
        Ok(cur)
    }
}

pub fn resize(img: &Image, out_h: usize, out_w: usize, interp: Interp) -> VisionResult<Image> {
    if out_h == 0 || out_w == 0 {
        return Err(VisionError::Shape("resize: zero dimension".into()));
    }
    let c = img.channels();
    let mut out = vec![0u8; out_h * out_w * c];
    // align_corners=False: x_in = (x_out + 0.5) * scale - 0.5
    let scale_y = img.height as f32 / out_h as f32;
    let scale_x = img.width as f32 / out_w as f32;

    match interp {
        Interp::Nearest => {
            for y in 0..out_h {
                let sy = ((y as f32 + 0.5) * scale_y).floor() as isize;
                let sy = sy.clamp(0, img.height as isize - 1) as usize;
                for x in 0..out_w {
                    let sx = ((x as f32 + 0.5) * scale_x).floor() as isize;
                    let sx = sx.clamp(0, img.width as isize - 1) as usize;
                    let src = img.pixel_offset(sy, sx);
                    let dst = (y * out_w + x) * c;
                    out[dst..dst + c].copy_from_slice(&img.data[src..src + c]);
                }
            }
        }
        Interp::Bilinear => {
            for y in 0..out_h {
                let fy = (y as f32 + 0.5) * scale_y - 0.5;
                let y0 = fy.floor() as isize;
                let y1 = y0 + 1;
                let wy = fy - y0 as f32;
                for x in 0..out_w {
                    let fx = (x as f32 + 0.5) * scale_x - 0.5;
                    let x0 = fx.floor() as isize;
                    let x1 = x0 + 1;
                    let wx = fx - x0 as f32;
                    let dst = (y * out_w + x) * c;
                    for ch in 0..c {
                        let mut acc = 0.0f32;
                        for (iy, wyi) in [(y0, 1.0 - wy), (y1, wy)] {
                            for (ix, wxi) in [(x0, 1.0 - wx), (x1, wx)] {
                                let yy = iy.clamp(0, img.height as isize - 1) as usize;
                                let xx = ix.clamp(0, img.width as isize - 1) as usize;
                                acc += img.data[img.pixel_offset(yy, xx) + ch] as f32 * wyi * wxi;
                            }
                        }
                        out[dst + ch] = acc.round().clamp(0.0, 255.0) as u8;
                    }
                }
            }
        }
        Interp::Bicubic => {
            for y in 0..out_h {
                let fy = (y as f32 + 0.5) * scale_y - 0.5;
                let y0 = fy.floor() as isize;
                let wy = fy - y0 as f32;
                for x in 0..out_w {
                    let fx = (x as f32 + 0.5) * scale_x - 0.5;
                    let x0 = fx.floor() as isize;
                    let wx = fx - x0 as f32;
                    let dst = (y * out_w + x) * c;
                    for ch in 0..c {
                        let mut acc = 0.0f32;
                        for j in -1..=2 {
                            let cy = cubic_weight(wy - j as f32);
                            let yy = (y0 + j).clamp(0, img.height as isize - 1) as usize;
                            for i in -1..=2 {
                                let cx = cubic_weight(wx - i as f32);
                                let xx = (x0 + i).clamp(0, img.width as isize - 1) as usize;
                                acc += img.data[img.pixel_offset(yy, xx) + ch] as f32 * cy * cx;
                            }
                        }
                        out[dst + ch] = acc.round().clamp(0.0, 255.0) as u8;
                    }
                }
            }
        }
    }
    Image::new(out_h, out_w, img.mode, out)
}

#[inline]
fn cubic_weight(t: f32) -> f32 {
    let a = -0.75f32; // Keys / torchvision
    let x = t.abs();
    if x <= 1.0 {
        ((a + 2.0) * x - (a + 3.0)) * x * x + 1.0
    } else if x < 2.0 {
        ((a * x - 5.0 * a) * x + 8.0 * a) * x - 4.0 * a
    } else {
        0.0
    }
}

pub fn crop(img: &Image, top: usize, left: usize, height: usize, width: usize) -> VisionResult<Image> {
    if top + height > img.height || left + width > img.width {
        return Err(VisionError::Shape("crop out of bounds".into()));
    }
    let c = img.channels();
    let mut out = vec![0u8; height * width * c];
    for y in 0..height {
        let src = img.pixel_offset(top + y, left);
        let dst = y * width * c;
        out[dst..dst + width * c].copy_from_slice(&img.data[src..src + width * c]);
    }
    Image::new(height, width, img.mode, out)
}

pub fn center_crop(img: &Image, height: usize, width: usize) -> VisionResult<Image> {
    if height > img.height || width > img.width {
        return Err(VisionError::Shape("center_crop larger than image".into()));
    }
    let top = (img.height - height) / 2;
    let left = (img.width - width) / 2;
    crop(img, top, left, height, width)
}

pub fn pad(img: &Image, top: usize, bottom: usize, left: usize, right: usize, value: u8) -> VisionResult<Image> {
    let h = img.height + top + bottom;
    let w = img.width + left + right;
    let c = img.channels();
    let mut out = vec![value; h * w * c];
    for y in 0..img.height {
        let src = img.pixel_offset(y, 0);
        let dst = ((y + top) * w + left) * c;
        out[dst..dst + img.width * c].copy_from_slice(&img.data[src..src + img.width * c]);
    }
    Image::new(h, w, img.mode, out)
}

pub fn flip_horizontal(img: &Image) -> Image {
    let c = img.channels();
    let mut out = vec![0u8; img.data.len()];
    for y in 0..img.height {
        for x in 0..img.width {
            let src = img.pixel_offset(y, x);
            let dst = img.pixel_offset(y, img.width - 1 - x);
            out[dst..dst + c].copy_from_slice(&img.data[src..src + c]);
        }
    }
    Image {
        height: img.height,
        width: img.width,
        mode: img.mode,
        data: out,
    }
}

pub fn flip_vertical(img: &Image) -> Image {
    let c = img.channels();
    let mut out = vec![0u8; img.data.len()];
    for y in 0..img.height {
        let src = img.pixel_offset(y, 0);
        let dst = img.pixel_offset(img.height - 1 - y, 0);
        out[dst..dst + img.width * c].copy_from_slice(&img.data[src..src + img.width * c]);
    }
    Image {
        height: img.height,
        width: img.width,
        mode: img.mode,
        data: out,
    }
}

/// Rotate by degrees clockwise about center (bilinear).
pub fn rotate(img: &Image, degrees: f32) -> VisionResult<Image> {
    let rad = -degrees.to_radians(); // clockwise positive → negate for math
    let (cx, cy) = (img.width as f32 * 0.5, img.height as f32 * 0.5);
    let (cos_t, sin_t) = (rad.cos(), rad.sin());
    let c = img.channels();
    let mut out = vec![0u8; img.data.len()];
    for y in 0..img.height {
        for x in 0..img.width {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let sx = cos_t * dx - sin_t * dy + cx - 0.5;
            let sy = sin_t * dx + cos_t * dy + cy - 0.5;
            let dst = img.pixel_offset(y, x);
            if sx < 0.0 || sy < 0.0 || sx >= img.width as f32 - 1.0 || sy >= img.height as f32 - 1.0
            {
                continue;
            }
            let x0 = sx.floor() as usize;
            let y0 = sy.floor() as usize;
            let wx = sx - x0 as f32;
            let wy = sy - y0 as f32;
            for ch in 0..c {
                let v00 = img.data[img.pixel_offset(y0, x0) + ch] as f32;
                let v10 = img.data[img.pixel_offset(y0, x0 + 1) + ch] as f32;
                let v01 = img.data[img.pixel_offset(y0 + 1, x0) + ch] as f32;
                let v11 = img.data[img.pixel_offset(y0 + 1, x0 + 1) + ch] as f32;
                let v = v00 * (1.0 - wx) * (1.0 - wy)
                    + v10 * wx * (1.0 - wy)
                    + v01 * (1.0 - wx) * wy
                    + v11 * wx * wy;
                out[dst + ch] = v.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    Ok(Image {
        height: img.height,
        width: img.width,
        mode: img.mode,
        data: out,
    })
}

/// Affine warp: 2×3 matrix [[a,b,tx],[c,d,ty]] maps output→input.
pub fn warp_affine(img: &Image, m: [[f32; 3]; 2], out_h: usize, out_w: usize) -> VisionResult<Image> {
    let c = img.channels();
    let mut out = vec![0u8; out_h * out_w * c];
    for y in 0..out_h {
        for x in 0..out_w {
            let sx = m[0][0] * x as f32 + m[0][1] * y as f32 + m[0][2];
            let sy = m[1][0] * x as f32 + m[1][1] * y as f32 + m[1][2];
            let dst = (y * out_w + x) * c;
            if sx < 0.0 || sy < 0.0 || sx >= img.width as f32 - 1.0 || sy >= img.height as f32 - 1.0
            {
                continue;
            }
            let x0 = sx.floor() as usize;
            let y0 = sy.floor() as usize;
            let wx = sx - x0 as f32;
            let wy = sy - y0 as f32;
            for ch in 0..c {
                let v = img.data[img.pixel_offset(y0, x0) + ch] as f32 * (1.0 - wx) * (1.0 - wy)
                    + img.data[img.pixel_offset(y0, x0 + 1) + ch] as f32 * wx * (1.0 - wy)
                    + img.data[img.pixel_offset(y0 + 1, x0) + ch] as f32 * (1.0 - wx) * wy
                    + img.data[img.pixel_offset(y0 + 1, x0 + 1) + ch] as f32 * wx * wy;
                out[dst + ch] = v.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    Image::new(out_h, out_w, img.mode, out)
}

/// Perspective: 3×3 homography output→input.
pub fn warp_perspective(
    img: &Image,
    h: [[f32; 3]; 3],
    out_h: usize,
    out_w: usize,
) -> VisionResult<Image> {
    let c = img.channels();
    let mut out = vec![0u8; out_h * out_w * c];
    for y in 0..out_h {
        for x in 0..out_w {
            let denom = h[2][0] * x as f32 + h[2][1] * y as f32 + h[2][2];
            if denom.abs() < 1e-8 {
                continue;
            }
            let sx = (h[0][0] * x as f32 + h[0][1] * y as f32 + h[0][2]) / denom;
            let sy = (h[1][0] * x as f32 + h[1][1] * y as f32 + h[1][2]) / denom;
            if sx < 0.0 || sy < 0.0 || sx >= img.width as f32 - 1.0 || sy >= img.height as f32 - 1.0
            {
                continue;
            }
            let x0 = sx.floor() as usize;
            let y0 = sy.floor() as usize;
            let wx = sx - x0 as f32;
            let wy = sy - y0 as f32;
            let dst = (y * out_w + x) * c;
            for ch in 0..c {
                let v = img.data[img.pixel_offset(y0, x0) + ch] as f32 * (1.0 - wx) * (1.0 - wy)
                    + img.data[img.pixel_offset(y0, x0 + 1) + ch] as f32 * wx * (1.0 - wy)
                    + img.data[img.pixel_offset(y0 + 1, x0) + ch] as f32 * (1.0 - wx) * wy
                    + img.data[img.pixel_offset(y0 + 1, x0 + 1) + ch] as f32 * wx * wy;
                out[dst + ch] = v.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    Image::new(out_h, out_w, img.mode, out)
}

pub fn to_grayscale(img: &Image) -> VisionResult<Image> {
    match img.mode {
        ColorMode::Gray => Ok(img.clone()),
        ColorMode::Rgb | ColorMode::Rgba => {
            let mut out = vec![0u8; img.height * img.width];
            for i in 0..img.height * img.width {
                let o = i * img.channels();
                let y = 0.299 * img.data[o] as f32
                    + 0.587 * img.data[o + 1] as f32
                    + 0.114 * img.data[o + 2] as f32;
                out[i] = y.round() as u8;
            }
            Image::new(img.height, img.width, ColorMode::Gray, out)
        }
    }
}

pub struct Resize {
    pub height: usize,
    pub width: usize,
    pub interp: Interp,
}
impl Transform for Resize {
    fn apply(&self, img: &Image) -> VisionResult<Image> {
        resize(img, self.height, self.width, self.interp)
    }
}

pub struct CenterCrop {
    pub height: usize,
    pub width: usize,
}
impl Transform for CenterCrop {
    fn apply(&self, img: &Image) -> VisionResult<Image> {
        center_crop(img, self.height, self.width)
    }
}

pub struct RandomHorizontalFlip {
    pub p: f32,
    pub seed: u64,
}
impl Transform for RandomHorizontalFlip {
    fn apply(&self, img: &Image) -> VisionResult<Image> {
        let mut rng = StdRng::seed_from_u64(self.seed.wrapping_add(img.height as u64 * 31));
        if rng.gen_f32() < self.p {
            Ok(flip_horizontal(img))
        } else {
            Ok(img.clone())
        }
    }
}

pub struct ColorJitter {
    pub brightness: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub hue: f32,
    pub seed: u64,
}
impl Transform for ColorJitter {
    fn apply(&self, img: &Image) -> VisionResult<Image> {
        let mut rng = StdRng::seed_from_u64(self.seed);
        let mut out = img.clone();
        if self.brightness > 0.0 {
            let f = 1.0 + (rng.gen_f32() * 2.0 - 1.0) * self.brightness;
            for v in out.data.iter_mut() {
                *v = (*v as f32 * f).round().clamp(0.0, 255.0) as u8;
            }
        }
        if self.contrast > 0.0 {
            let f = 1.0 + (rng.gen_f32() * 2.0 - 1.0) * self.contrast;
            let mean = out.data.iter().map(|&v| v as f32).sum::<f32>() / out.data.len() as f32;
            for v in out.data.iter_mut() {
                *v = ((*v as f32 - mean) * f + mean).round().clamp(0.0, 255.0) as u8;
            }
        }
        let _ = (self.saturation, self.hue); // HSV jitter deferred lightly
        Ok(out)
    }
}

pub struct GaussianBlur {
    pub kernel: usize,
    pub sigma: f32,
}
impl Transform for GaussianBlur {
    fn apply(&self, img: &Image) -> VisionResult<Image> {
        crate::ops::gaussian_blur(img, self.kernel, self.sigma)
    }
}

pub struct RandomErasing {
    pub p: f32,
    pub scale: (f32, f32),
    pub seed: u64,
}
impl Transform for RandomErasing {
    fn apply(&self, img: &Image) -> VisionResult<Image> {
        let mut rng = StdRng::seed_from_u64(self.seed);
        if rng.gen_f32() >= self.p {
            return Ok(img.clone());
        }
        let area = (img.height * img.width) as f32;
        let target = area * (self.scale.0 + (self.scale.1 - self.scale.0) * rng.gen_f32());
        let h = (target.sqrt() as usize).clamp(1, img.height);
        let w = (target.sqrt() as usize).clamp(1, img.width);
        let top = if img.height > h {
            rng.gen_range_usize(0, img.height - h + 1)
        } else {
            0
        };
        let left = if img.width > w {
            rng.gen_range_usize(0, img.width - w + 1)
        } else {
            0
        };
        let mut out = img.clone();
        let c = out.channels();
        for y in top..top + h {
            for x in left..left + w {
                let o = out.pixel_offset(y, x);
                for ch in 0..c {
                    out.data[o + ch] = 0;
                }
            }
        }
        Ok(out)
    }
}

pub struct RandomResizedCrop {
    pub size: usize,
    pub scale: (f32, f32),
    pub seed: u64,
}
impl Transform for RandomResizedCrop {
    fn apply(&self, img: &Image) -> VisionResult<Image> {
        let mut rng = StdRng::seed_from_u64(self.seed);
        let area = (img.height * img.width) as f32;
        let target = area * (self.scale.0 + (self.scale.1 - self.scale.0) * rng.gen_f32());
        let side = (target.sqrt() as usize).clamp(1, img.height.min(img.width));
        let top = if img.height > side {
            rng.gen_range_usize(0, img.height - side + 1)
        } else {
            0
        };
        let left = if img.width > side {
            rng.gen_range_usize(0, img.width - side + 1)
        } else {
            0
        };
        let cropped = crop(img, top, left, side.min(img.height - top), side.min(img.width - left))?;
        resize(&cropped, self.size, self.size, Interp::Bilinear)
    }
}

/// Convert image to CHW tensor [0,1].
pub fn to_tensor(img: &Image) -> VisionResult<Tensor> {
    img.to_tensor()
}

pub fn normalize(t: &Tensor, mean: &[f32], std: &[f32]) -> VisionResult<Tensor> {
    normalize_tensor(t, mean, std)
}
