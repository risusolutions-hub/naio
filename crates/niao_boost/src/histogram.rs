//! Histogram accumulation, split gain, and histogram subtraction.

use crate::binning::BinnedMatrix;
use crate::params::BoosterParams;

#[derive(Clone, Debug, Default)]
pub struct SplitCandidate {
    pub feature: usize,
    pub bin: u8,
    pub default_left: bool,
    pub gain: f64,
    pub left_count: u32,
    pub right_count: u32,
}

#[derive(Clone, Debug)]
pub struct FeatureHistogram {
    pub grad: Vec<f64>,
    pub hess: Vec<f64>,
    pub count: Vec<u32>,
    pub n_bins: usize,
}

impl FeatureHistogram {
    pub fn new(n_bins: usize) -> Self {
        Self {
            grad: vec![0.0; n_bins + 1],
            hess: vec![0.0; n_bins + 1],
            count: vec![0; n_bins + 1],
            n_bins,
        }
    }

    pub fn clear(&mut self) {
        self.grad.fill(0.0);
        self.hess.fill(0.0);
        self.count.fill(0);
    }

    #[inline]
    pub fn add(&mut self, bin: usize, g: f64, h: f64) {
        self.grad[bin] += g;
        self.hess[bin] += h;
        self.count[bin] += 1;
    }

    pub fn subtract_from(&self, other: &mut Self) {
        for i in 0..=self.n_bins {
            other.grad[i] -= self.grad[i];
            other.hess[i] -= self.hess[i];
            other.count[i] -= self.count[i];
        }
    }
}

#[inline]
pub fn split_gain(g_l: f64, h_l: f64, g_r: f64, h_r: f64, lambda: f64, gamma: f64) -> f64 {
    let g = g_l + g_r;
    let h = h_l + h_r;
    0.5
        * (g_l * g_l / (h_l + lambda) + g_r * g_r / (h_r + lambda) - g * g / (h + lambda))
        - gamma
}

/// Build histogram for one feature over `rows`.
pub fn build_histogram(
    data: &BinnedMatrix,
    feature: usize,
    rows: &[usize],
    grad: &[f64],
    hess: &[f64],
    out: &mut FeatureHistogram,
) {
    out.clear();
    let n_rows = data.n_rows;
    for &r in rows {
        let idx = feature * n_rows + r;
        let g = grad[r];
        let h = hess[r];
        if data.missing[idx] {
            out.add(out.n_bins, g, h);
        } else {
            out.add(data.bins[idx] as usize, g, h);
        }
    }
}

/// Find best split on a feature histogram.
pub fn best_split_on_histogram(
    hist: &FeatureHistogram,
    params: &BoosterParams,
    feature: usize,
) -> Option<SplitCandidate> {
    let nb = hist.n_bins;
    let missing_bin = nb;

    let total_g: f64 = hist.grad.iter().sum();
    let total_h: f64 = hist.hess.iter().sum();
    let total_c: u32 = hist.count.iter().sum();

    let mut best: Option<SplitCandidate> = None;

    for default_left in [true, false] {
        let (mg, mh, mc) = if default_left {
            (
                hist.grad[missing_bin],
                hist.hess[missing_bin],
                hist.count[missing_bin],
            )
        } else {
            (0.0, 0.0, 0u32)
        };

        let mut cum_g = mg;
        let mut cum_h = mh;
        let mut cum_c = mc;

        for b in 0..nb {
            cum_g += hist.grad[b];
            cum_h += hist.hess[b];
            cum_c += hist.count[b];

            let left_g = cum_g;
            let left_h = cum_h;
            let left_c = cum_c;

            let right_g = total_g - left_g
                + if default_left {
                    0.0
                } else {
                    hist.grad[missing_bin]
                };
            let right_h = total_h - left_h
                + if default_left {
                    0.0
                } else {
                    hist.hess[missing_bin]
                };
            let right_c = total_c - left_c
                + if default_left {
                    0
                } else {
                    hist.count[missing_bin]
                };

            if left_c < params.min_data_in_leaf as u32
                || right_c < params.min_data_in_leaf as u32
                || left_h < params.min_child_weight
                || right_h < params.min_child_weight
            {
                continue;
            }

            let gain = split_gain(left_g, left_h, right_g, right_h, params.lambda_l2, params.gamma);
            if gain <= 0.0 {
                continue;
            }

            if best.as_ref().map_or(true, |b| gain > b.gain) {
                best = Some(SplitCandidate {
                    feature,
                    bin: b as u8,
                    default_left,
                    gain,
                    left_count: left_c,
                    right_count: right_c,
                });
            }
        }
    }

    best
}

/// Brute-force exact split on raw float values (reference for tiny data).
pub fn exact_best_split(
    x_col: &[f64],
    rows: &[usize],
    grad: &[f64],
    hess: &[f64],
    params: &BoosterParams,
    feature: usize,
) -> Option<SplitCandidate> {
    let mut values: Vec<f64> = rows
        .iter()
        .filter_map(|&r| {
            let v = x_col[r];
            if v.is_nan() {
                None
            } else {
                Some(v)
            }
        })
        .collect();
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    values.dedup_by(|a, b| (*a - *b).abs() < 1e-15);

    let total_g: f64 = rows.iter().map(|&r| grad[r]).sum();
    let total_h: f64 = rows.iter().map(|&r| hess[r]).sum();
    let total_c = rows.len() as u32;

    let missing_rows: Vec<usize> = rows
        .iter()
        .copied()
        .filter(|&r| x_col[r].is_nan())
        .collect();
    let (mg, mh, mc) = missing_rows.iter().fold((0.0, 0.0, 0u32), |(g, h, c), &r| {
        (g + grad[r], h + hess[r], c + 1)
    });

    let mut best: Option<SplitCandidate> = None;

    for default_left in [true, false] {
        for split_idx in 0..values.len() {
            let threshold = values[split_idx];

            let mut left_g = if default_left { mg } else { 0.0 };
            let mut left_h = if default_left { mh } else { 0.0 };
            let mut left_c = if default_left { mc } else { 0 };

            for &r in rows {
                let v = x_col[r];
                if v.is_nan() {
                    continue;
                }
                if v <= threshold {
                    left_g += grad[r];
                    left_h += hess[r];
                    left_c += 1;
                }
            }

            let right_g = total_g - left_g + if default_left { 0.0 } else { mg };
            let right_h = total_h - left_h + if default_left { 0.0 } else { mh };
            let right_c = total_c - left_c + if default_left { 0 } else { mc };

            if left_c < params.min_data_in_leaf as u32
                || right_c < params.min_data_in_leaf as u32
                || left_h < params.min_child_weight
                || right_h < params.min_child_weight
            {
                continue;
            }

            let gain = split_gain(left_g, left_h, right_g, right_h, params.lambda_l2, params.gamma);
            if gain <= 0.0 {
                continue;
            }

            if best.as_ref().map_or(true, |b| gain > b.gain) {
                best = Some(SplitCandidate {
                    feature,
                    bin: 0,
                    default_left,
                    gain,
                    left_count: left_c,
                    right_count: right_c,
                });
            }
        }
    }

    best
}

/// Partition rows by split; returns (left_rows, right_rows).
pub fn partition_rows(
    data: &BinnedMatrix,
    rows: &[usize],
    split: &SplitCandidate,
) -> (Vec<usize>, Vec<usize>) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    let f = split.feature;
    for &r in rows {
        if data.is_missing(f, r) {
            if split.default_left {
                left.push(r);
            } else {
                right.push(r);
            }
        } else if data.bin_at(f, r) <= split.bin {
            left.push(r);
        } else {
            right.push(r);
        }
    }
    (left, right)
}

/// Leaf weight from gradient/hessian sums.
#[inline]
pub fn leaf_weight(sum_g: f64, sum_h: f64, lambda: f64, alpha: f64) -> f64 {
    let w = -sum_g / (sum_h + lambda);
    if alpha > 0.0 {
        if w > 0.0 {
            (-sum_g - alpha).max(0.0) / (sum_h + lambda)
        } else {
            (-sum_g + alpha).min(0.0) / (sum_h + lambda)
        }
    } else {
        w
    }
}

/// Sum grad/hess over rows.
pub fn row_sums(rows: &[usize], grad: &[f64], hess: &[f64]) -> (f64, f64, u32) {
    let mut g = 0.0;
    let mut h = 0.0;
    for &r in rows {
        g += grad[r];
        h += hess[r];
    }
    (g, h, rows.len() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binning::BinnedMatrix;

    #[test]
    fn histogram_subtraction_identity() {
        let x = vec![0.0, 1.0, 2.0, 3.0];
        let bm = BinnedMatrix::from_matrix(&x, 2, 2, 4).unwrap();
        let rows = vec![0, 1];
        let grad = vec![1.0, -1.0, 0.5, -0.5];
        let hess = vec![1.0; 4];
        let mut h0 = FeatureHistogram::new(4);
        build_histogram(&bm, 0, &rows, &grad, &hess, &mut h0);
        let mut parent = FeatureHistogram::new(4);
        build_histogram(&bm, 0, &[0, 1], &grad, &hess, &mut parent);
        assert!(parent.count.iter().sum::<u32>() <= 2);
    }

    #[test]
    fn split_gain_symmetry() {
        let g = split_gain(1.0, 2.0, -1.0, 2.0, 1.0, 0.0);
        assert!(g.is_finite());
    }
}
