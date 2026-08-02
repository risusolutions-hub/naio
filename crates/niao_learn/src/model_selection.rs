//! Model selection helpers (local until `ntune` is wired).

use crate::error::{LearnError, LearnResult};
use crate::utils::vector_from;
use niao_num::NdArray;
use niao_rand::{Rng, SeedableRng, StdRng};

pub fn train_test_split(
    x: &NdArray,
    y: &NdArray,
    test_size: f64,
    random_state: u64,
) -> LearnResult<(NdArray, NdArray, NdArray, NdArray)> {
    if x.ndim() != 2 || y.ndim() < 1 {
        return Err(LearnError::Shape("train_test_split shape".into()));
    }
    let n = x.shape[0];
    let d = x.shape[1];
    if y.shape[0] != n {
        return Err(LearnError::Shape("X/y row mismatch".into()));
    }
    let n_test = ((n as f64) * test_size).round() as usize;
    let n_test = n_test.clamp(1, n.saturating_sub(1));
    let mut idx: Vec<usize> = (0..n).collect();
    let mut rng = StdRng::seed_from_u64(random_state);
    // Fisher–Yates
    for i in (1..n).rev() {
        let j = rng.gen_range_usize(0, i + 1);
        idx.swap(i, j);
    }
    let test_idx = &idx[..n_test];
    let train_idx = &idx[n_test..];
    let xv = x.to_vec();
    let yv = y.to_vec();
    let y_is_1d = y.ndim() == 1;
    let mut x_train = Vec::with_capacity(train_idx.len() * d);
    let mut x_test = Vec::with_capacity(test_idx.len() * d);
    let mut y_train = Vec::with_capacity(train_idx.len());
    let mut y_test = Vec::with_capacity(test_idx.len());
    for &i in train_idx {
        x_train.extend_from_slice(&xv[i * d..(i + 1) * d]);
        if y_is_1d {
            y_train.push(yv[i]);
        } else {
            // flatten row
            let yd = y.shape[1];
            y_train.extend_from_slice(&yv[i * yd..(i + 1) * yd]);
        }
    }
    for &i in test_idx {
        x_test.extend_from_slice(&xv[i * d..(i + 1) * d]);
        if y_is_1d {
            y_test.push(yv[i]);
        } else {
            let yd = y.shape[1];
            y_test.extend_from_slice(&yv[i * yd..(i + 1) * yd]);
        }
    }
    let xt = NdArray::from_vec(vec![train_idx.len(), d], x_train)
        .map_err(|e| LearnError::Error(e.to_string()))?;
    let xv = NdArray::from_vec(vec![test_idx.len(), d], x_test)
        .map_err(|e| LearnError::Error(e.to_string()))?;
    let yt = if y_is_1d {
        vector_from(y_train)?
    } else {
        NdArray::from_vec(vec![train_idx.len(), y.shape[1]], y_train)
            .map_err(|e| LearnError::Error(e.to_string()))?
    };
    let yv = if y_is_1d {
        vector_from(y_test)?
    } else {
        NdArray::from_vec(vec![test_idx.len(), y.shape[1]], y_test)
            .map_err(|e| LearnError::Error(e.to_string()))?
    };
    Ok((xt, xv, yt, yv))
}

#[derive(Clone, Debug)]
pub struct KFold {
    pub n_splits: usize,
    pub shuffle: bool,
    pub random_state: u64,
}

impl KFold {
    pub fn new(n_splits: usize) -> Self {
        Self {
            n_splits,
            shuffle: false,
            random_state: 0,
        }
    }

    pub fn split(&self, n_samples: usize) -> LearnResult<Vec<(Vec<usize>, Vec<usize>)>> {
        if self.n_splits < 2 || self.n_splits > n_samples {
            return Err(LearnError::Error("invalid n_splits".into()));
        }
        let mut idx: Vec<usize> = (0..n_samples).collect();
        if self.shuffle {
            let mut rng = StdRng::seed_from_u64(self.random_state);
            for i in (1..n_samples).rev() {
                let j = rng.gen_range_usize(0, i + 1);
                idx.swap(i, j);
            }
        }
        let fold_sizes = {
            let base = n_samples / self.n_splits;
            let rem = n_samples % self.n_splits;
            (0..self.n_splits)
                .map(|i| base + if i < rem { 1 } else { 0 })
                .collect::<Vec<_>>()
        };
        let mut folds = Vec::new();
        let mut start = 0usize;
        for &fs in &fold_sizes {
            let test: Vec<usize> = idx[start..start + fs].to_vec();
            let mut train = Vec::with_capacity(n_samples - fs);
            train.extend_from_slice(&idx[..start]);
            train.extend_from_slice(&idx[start + fs..]);
            folds.push((train, test));
            start += fs;
        }
        Ok(folds)
    }
}
