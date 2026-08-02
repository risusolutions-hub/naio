//! Quantile binning of feature columns into u8 codes.

use crate::error::{BoostError, BoostResult};

pub const MISSING_BIN: u8 = 255;

/// Column-major binned feature matrix plus split thresholds.
#[derive(Clone, Debug)]
pub struct BinnedMatrix {
    pub n_rows: usize,
    pub n_features: usize,
    pub max_bins: usize,
    /// Column-major bins: index = feature * n_rows + row.
    pub bins: Vec<u8>,
    /// Per-feature upper bounds for each bin (excluding missing).
    pub thresholds: Vec<Vec<f64>>,
    /// Missing mask per (feature, row).
    pub missing: Vec<bool>,
}

impl BinnedMatrix {
    pub fn from_matrix(
        x: &[f64],
        n_rows: usize,
        n_features: usize,
        max_bins: usize,
    ) -> BoostResult<Self> {
        if x.len() != n_rows * n_features {
            return Err(BoostError::Shape(format!(
                "X length {} != {} * {}",
                x.len(),
                n_rows,
                n_features
            )));
        }
        if max_bins < 2 || max_bins > 256 {
            return Err(BoostError::BadParam("max_bins must be in [2, 256]".into()));
        }
        let mut bins = vec![0u8; n_rows * n_features];
        let mut missing = vec![false; n_rows * n_features];
        let mut thresholds = Vec::with_capacity(n_features);

        for f in 0..n_features {
            let mut values: Vec<(f64, usize)> = Vec::new();
            for r in 0..n_rows {
                let v = x[r * n_features + f];
                if v.is_nan() {
                    missing[f * n_rows + r] = true;
                    bins[f * n_rows + r] = MISSING_BIN;
                } else {
                    values.push((v, r));
                }
            }
            values.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

            let n_unique = values
                .iter()
                .map(|(v, _)| (v * 1e15).round() as i64)
                .collect::<Vec<_>>()
                .windows(2)
                .filter(|w| w[0] != w[1])
                .count()
                + if values.is_empty() { 0 } else { 1 };

            let n_bins = n_unique.min(max_bins).max(1);
            let mut upper = Vec::with_capacity(n_bins);
            if values.is_empty() {
                thresholds.push(upper);
                continue;
            }
            if n_bins == 1 {
                upper.push(values.last().unwrap().0);
            } else {
                for b in 1..=n_bins {
                    let q = (b as f64) / (n_bins as f64);
                    let idx = ((values.len() as f64 * q) - 1.0).round() as isize;
                    let idx = idx.clamp(0, values.len() as isize - 1) as usize;
                    upper.push(values[idx].0);
                }
            }
            thresholds.push(upper.clone());

            for r in 0..n_rows {
                let idx = f * n_rows + r;
                if missing[idx] {
                    continue;
                }
                let v = x[r * n_features + f];
                let mut bin = 0usize;
                while bin < upper.len() && v > upper[bin] {
                    bin += 1;
                }
                bins[idx] = bin.min(255) as u8;
            }
        }

        Ok(Self {
            n_rows,
            n_features,
            max_bins,
            bins,
            thresholds,
            missing,
        })
    }

    #[inline]
    pub fn bin_at(&self, feature: usize, row: usize) -> u8 {
        self.bins[feature * self.n_rows + row]
    }

    #[inline]
    pub fn is_missing(&self, feature: usize, row: usize) -> bool {
        self.missing[feature * self.n_rows + row]
    }
}

/// Map a raw feature value to its bin (for prediction on unseen data).
pub fn value_to_bin(thresholds: &[f64], v: f64) -> u8 {
    if v.is_nan() {
        return MISSING_BIN;
    }
    let mut bin = 0usize;
    while bin < thresholds.len() && v > thresholds[bin] {
        bin += 1;
    }
    bin.min(255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binning_monotone() {
        let x = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let bm = BinnedMatrix::from_matrix(&x, 3, 2, 4).unwrap();
        assert_eq!(bm.n_rows, 3);
        assert_eq!(bm.n_features, 2);
        assert!(bm.bin_at(0, 0) <= bm.bin_at(0, 1));
    }
}
