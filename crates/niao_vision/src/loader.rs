//! Vision DataLoader — batch/shuffle into niao_tensor; delegates collate to niao_ml when possible.

use crate::datasets::{Sample, VisionDataset};
use crate::error::{VisionError, VisionResult};
use crate::transform::{normalize, to_tensor, Transform};
use niao_ml::DataLoader as MlDataLoader;
use niao_rand::{Rng, SeedableRng, SliceRandom, StdRng};
use niao_tensor::{Device, Tensor};

pub struct VisionDataLoader<'a, D: VisionDataset> {
    pub dataset: &'a D,
    pub batch_size: usize,
    pub shuffle: bool,
    pub seed: u64,
    pub transform: Option<&'a dyn Transform>,
    order: Vec<usize>,
    cursor: usize,
}

impl<'a, D: VisionDataset> VisionDataLoader<'a, D> {
    pub fn new(dataset: &'a D, batch_size: usize, shuffle: bool, seed: u64) -> Self {
        let n = dataset.len();
        Self {
            dataset,
            batch_size: batch_size.max(1),
            shuffle,
            seed,
            transform: None,
            order: (0..n).collect(),
            cursor: 0,
        }
    }

    pub fn with_transform(mut self, t: &'a dyn Transform) -> Self {
        self.transform = Some(t);
        self
    }

    pub fn reset(&mut self) {
        self.cursor = 0;
        self.order = (0..self.dataset.len()).collect();
        if self.shuffle {
            let mut rng = StdRng::seed_from_u64(self.seed);
            self.order.shuffle(&mut rng);
        }
    }

    pub fn next_batch(&mut self) -> VisionResult<Option<(Tensor, Tensor)>> {
        if self.cursor == 0 && self.order.len() == self.dataset.len() {
            // ensure shuffled on first call if needed
        }
        if self.cursor >= self.order.len() {
            return Ok(None);
        }
        let end = (self.cursor + self.batch_size).min(self.order.len());
        let indices = &self.order[self.cursor..end];
        self.cursor = end;

        let mut feats = Vec::new();
        let mut labels = Vec::new();
        let mut shape_chw = None;
        for &idx in indices {
            let Sample { image, label } = self.dataset.get(idx)?;
            let image = if let Some(t) = self.transform {
                t.apply(&image)?
            } else {
                image
            };
            let t = to_tensor(&image)?;
            if shape_chw.is_none() {
                shape_chw = Some(t.shape.clone());
            } else if shape_chw.as_ref() != Some(&t.shape) {
                return Err(VisionError::Shape(
                    "batch images have mismatched shapes".into(),
                ));
            }
            feats.extend(t.to_cpu()?);
            labels.push(label as f32);
        }
        let b = indices.len();
        let chw = shape_chw.unwrap();
        let mut shape = vec![b];
        shape.extend_from_slice(&chw);
        let x = Tensor::from_cpu_data(&shape, feats, Device::Cpu)?;
        let y = Tensor::from_cpu_data(&[b, 1], labels, Device::Cpu)?;
        Ok(Some((x, y)))
    }

    /// Flatten CHW features and build an `niao_ml::DataLoader` for training loops.
    pub fn to_ml_loader(
        &mut self,
        mean: Option<&[f32]>,
        std: Option<&[f32]>,
    ) -> VisionResult<MlDataLoader> {
        self.reset();
        let mut all_x = Vec::new();
        let mut all_y = Vec::new();
        let mut feat_dim = 0usize;
        let mut n = 0usize;
        while let Some((x, y)) = self.next_batch()? {
            let data = x.to_cpu()?;
            if feat_dim == 0 {
                feat_dim = data.len() / x.shape[0];
            }
            all_x.extend(data);
            all_y.extend(y.to_cpu()?);
            n += x.shape[0];
        }
        if let (Some(m), Some(s)) = (mean, std) {
            // normalize each sample's CHW in the flat buffer if 3-channel known
            if m.len() == 3 && feat_dim % 3 == 0 {
                let hw = feat_dim / 3;
                for i in 0..n {
                    let base = i * feat_dim;
                    for ch in 0..3 {
                        for p in 0..hw {
                            let idx = base + ch * hw + p;
                            all_x[idx] = (all_x[idx] - m[ch]) / s[ch];
                        }
                    }
                }
            }
        }
        Ok(MlDataLoader::new(
            Tensor::from_cpu_data(&[n, feat_dim], all_x, Device::Cpu)?,
            Tensor::from_cpu_data(&[n, 1], all_y, Device::Cpu)?,
            self.batch_size,
        )?)
    }
}

pub fn collate_normalize(batch_x: &Tensor, mean: &[f32], std: &[f32]) -> VisionResult<Tensor> {
    // Expect NCHW
    if batch_x.shape.len() != 4 {
        return Err(VisionError::Shape("collate_normalize expects NCHW".into()));
    }
    let data = batch_x.to_cpu()?;
    let (n, c, h, w) = (
        batch_x.shape[0],
        batch_x.shape[1],
        batch_x.shape[2],
        batch_x.shape[3],
    );
    if mean.len() != c || std.len() != c {
        return Err(VisionError::Shape("mean/std channel mismatch".into()));
    }
    let hw = h * w;
    let mut out = data;
    for bi in 0..n {
        for ch in 0..c {
            let base = bi * c * hw + ch * hw;
            for i in 0..hw {
                out[base + i] = (out[base + i] - mean[ch]) / std[ch];
            }
        }
    }
    Ok(Tensor::from_cpu_data(&batch_x.shape, out, batch_x.device)?)
}

/// Seeded shuffle reproducibility helper used in tests.
pub fn shuffled_indices(n: usize, seed: u64) -> Vec<usize> {
    let mut order: Vec<usize> = (0..n).collect();
    let mut rng = StdRng::seed_from_u64(seed);
    order.shuffle(&mut rng);
    order
}

#[allow(dead_code)]
fn _use_normalize(t: &Tensor) -> VisionResult<Tensor> {
    normalize(t, &[0.5], &[0.5])
}

#[allow(dead_code)]
fn _use_rng() -> f32 {
    let mut r = StdRng::seed_from_u64(1);
    r.gen_f32()
}
