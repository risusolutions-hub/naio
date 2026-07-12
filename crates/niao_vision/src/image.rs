//! Contiguous HWC image buffer (u8 or f32).

use crate::error::{VisionError, VisionResult};
use niao_tensor::{Device, Tensor};

/// Pixel layout / channel count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Gray = 1,
    Rgb = 3,
    Rgba = 4,
}

impl ColorMode {
    pub fn channels(self) -> usize {
        self as usize
    }

    pub fn from_channels(c: usize) -> VisionResult<Self> {
        match c {
            1 => Ok(Self::Gray),
            3 => Ok(Self::Rgb),
            4 => Ok(Self::Rgba),
            _ => Err(VisionError::Shape(format!("unsupported channel count {c}"))),
        }
    }
}

/// Contiguous row-major H×W×C image. Default dtype for vision pipelines: u8 storage,
/// with f32 used after photometric / tensor transforms.
#[derive(Debug, Clone)]
pub struct Image {
    pub height: usize,
    pub width: usize,
    pub mode: ColorMode,
    /// Length = H * W * C. Contiguous HWC.
    pub data: Vec<u8>,
}

impl Image {
    pub fn new(height: usize, width: usize, mode: ColorMode, data: Vec<u8>) -> VisionResult<Self> {
        let n = height
            .checked_mul(width)
            .and_then(|hw| hw.checked_mul(mode.channels()))
            .ok_or_else(|| VisionError::Shape("image size overflow".into()))?;
        if data.len() != n {
            return Err(VisionError::Shape(format!(
                "buffer len {} != H*W*C ({n})",
                data.len()
            )));
        }
        Ok(Self {
            height,
            width,
            mode,
            data,
        })
    }

    pub fn zeros(height: usize, width: usize, mode: ColorMode) -> Self {
        let n = height * width * mode.channels();
        Self {
            height,
            width,
            mode,
            data: vec![0u8; n],
        }
    }

    pub fn channels(&self) -> usize {
        self.mode.channels()
    }

    #[inline]
    pub fn pixel_offset(&self, y: usize, x: usize) -> usize {
        (y * self.width + x) * self.channels()
    }

    /// Convert HWC u8 [0,255] → CHW f32 [0,1] tensor (torchvision ToTensor convention).
    pub fn to_tensor(&self) -> VisionResult<Tensor> {
        let c = self.channels();
        let hw = self.height * self.width;
        let mut out = vec![0.0f32; c * hw];
        for y in 0..self.height {
            for x in 0..self.width {
                let src = self.pixel_offset(y, x);
                let dst_px = y * self.width + x;
                for ch in 0..c {
                    out[ch * hw + dst_px] = self.data[src + ch] as f32 / 255.0;
                }
            }
        }
        Ok(Tensor::from_cpu_data(
            &[c, self.height, self.width],
            out,
            Device::Cpu,
        )?)
    }

    /// CHW f32 [0,1] (or any float) → HWC u8, clamped.
    pub fn from_tensor(t: &Tensor) -> VisionResult<Self> {
        let data = t.to_cpu()?;
        if t.shape.len() == 3 {
            let (c, h, w) = (t.shape[0], t.shape[1], t.shape[2]);
            let mode = ColorMode::from_channels(c)?;
            let mut buf = vec![0u8; h * w * c];
            let hw = h * w;
            for y in 0..h {
                for x in 0..w {
                    let dst = (y * w + x) * c;
                    let px = y * w + x;
                    for ch in 0..c {
                        let v = data[ch * hw + px].clamp(0.0, 1.0) * 255.0;
                        buf[dst + ch] = v.round() as u8;
                    }
                }
            }
            Self::new(h, w, mode, buf)
        } else if t.shape.len() == 2 {
            let (h, w) = (t.shape[0], t.shape[1]);
            let mut buf = vec![0u8; h * w];
            for (i, &v) in data.iter().enumerate() {
                buf[i] = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
            Self::new(h, w, ColorMode::Gray, buf)
        } else {
            Err(VisionError::Shape(format!(
                "from_tensor expects CHW or HW, got {:?}",
                t.shape
            )))
        }
    }

    /// Flatten pixels as f64 column-major frame buffer (H*W rows × C cols) for nframe interop.
    pub fn to_frame_pixels(&self) -> (usize, usize, Vec<f64>) {
        let rows = self.height * self.width;
        let cols = self.channels();
        let mut out = vec![0.0f64; rows * cols];
        for i in 0..rows {
            for c in 0..cols {
                out[i * cols + c] = self.data[i * cols + c] as f64;
            }
        }
        (rows, cols, out)
    }

    pub fn as_f32_hwc(&self) -> Vec<f32> {
        self.data.iter().map(|&v| v as f32).collect()
    }

    pub fn from_f32_hwc(
        height: usize,
        width: usize,
        mode: ColorMode,
        data: &[f32],
    ) -> VisionResult<Self> {
        let n = height * width * mode.channels();
        if data.len() != n {
            return Err(VisionError::Shape(format!(
                "f32 buffer len {} != {n}",
                data.len()
            )));
        }
        let buf: Vec<u8> = data
            .iter()
            .map(|&v| v.round().clamp(0.0, 255.0) as u8)
            .collect();
        Self::new(height, width, mode, buf)
    }
}

/// Normalize CHW tensor: `(x - mean) / std` per channel (torchvision Normalize).
pub fn normalize_tensor(t: &Tensor, mean: &[f32], std: &[f32]) -> VisionResult<Tensor> {
    let data = t.to_cpu()?;
    if t.shape.len() != 3 {
        return Err(VisionError::Shape("normalize expects CHW tensor".into()));
    }
    let c = t.shape[0];
    if mean.len() != c || std.len() != c {
        return Err(VisionError::Shape(format!(
            "mean/std length {}/{} != channels {c}",
            mean.len(),
            std.len()
        )));
    }
    let hw = t.shape[1] * t.shape[2];
    let mut out = data;
    for ch in 0..c {
        if std[ch].abs() < 1e-12 {
            return Err(VisionError::Error("normalize: std near zero".into()));
        }
        let base = ch * hw;
        for i in 0..hw {
            out[base + i] = (out[base + i] - mean[ch]) / std[ch];
        }
    }
    Ok(Tensor::from_cpu_data(&t.shape, out, t.device)?)
}
