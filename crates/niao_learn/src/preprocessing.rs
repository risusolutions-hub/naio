//! Preprocessing transformers.

use crate::error::{LearnError, LearnResult};
use crate::traits::{Estimator, Transformer};
use crate::utils::{check_2d, matrix_from, mean_axis0, unique_labels, vector_from, y_as_vec};
use niao_num::NdArray;

#[derive(Clone, Debug, Default)]
pub struct StandardScaler {
    pub mean: Option<Vec<f64>>,
    pub scale: Option<Vec<f64>>,
}

impl StandardScaler {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Estimator for StandardScaler {
    fn fit(&mut self, x: &NdArray, _y: Option<&NdArray>) -> LearnResult<()> {
        let (n, d) = check_2d(x, "X")?;
        if n == 0 {
            return Err(LearnError::Shape("empty X".into()));
        }
        let data = x.to_vec();
        let mean = mean_axis0(&data, n, d);
        // sklearn StandardScaler uses ddof=0 for scale (population)
        let mut scale = vec![0.0; d];
        for i in 0..n {
            for j in 0..d {
                let diff = data[i * d + j] - mean[j];
                scale[j] += diff * diff;
            }
        }
        for j in 0..d {
            scale[j] = (scale[j] / n as f64).sqrt();
            if scale[j] < 1e-12 {
                scale[j] = 1.0;
            }
        }
        self.mean = Some(mean);
        self.scale = Some(scale);
        Ok(())
    }
}

impl Transformer for StandardScaler {
    fn transform(&self, x: &NdArray) -> LearnResult<NdArray> {
        let mean = self
            .mean
            .as_ref()
            .ok_or_else(|| LearnError::NotFitted("StandardScaler not fitted".into()))?;
        let scale = self.scale.as_ref().unwrap();
        let (n, d) = check_2d(x, "X")?;
        if d != mean.len() {
            return Err(LearnError::Shape("feature count mismatch".into()));
        }
        let data = x.to_vec();
        let mut out = vec![0.0; n * d];
        for i in 0..n {
            for j in 0..d {
                out[i * d + j] = (data[i * d + j] - mean[j]) / scale[j];
            }
        }
        matrix_from((n, d), out)
    }
}

#[derive(Clone, Debug, Default)]
pub struct MinMaxScaler {
    pub data_min: Option<Vec<f64>>,
    pub data_max: Option<Vec<f64>>,
    pub feature_range: (f64, f64),
}

impl MinMaxScaler {
    pub fn new() -> Self {
        Self {
            feature_range: (0.0, 1.0),
            ..Default::default()
        }
    }
}

impl Estimator for MinMaxScaler {
    fn fit(&mut self, x: &NdArray, _y: Option<&NdArray>) -> LearnResult<()> {
        let (n, d) = check_2d(x, "X")?;
        let data = x.to_vec();
        let mut mn = vec![f64::INFINITY; d];
        let mut mx = vec![f64::NEG_INFINITY; d];
        for i in 0..n {
            for j in 0..d {
                let v = data[i * d + j];
                if v < mn[j] {
                    mn[j] = v;
                }
                if v > mx[j] {
                    mx[j] = v;
                }
            }
        }
        self.data_min = Some(mn);
        self.data_max = Some(mx);
        Ok(())
    }
}

impl Transformer for MinMaxScaler {
    fn transform(&self, x: &NdArray) -> LearnResult<NdArray> {
        let mn = self
            .data_min
            .as_ref()
            .ok_or_else(|| LearnError::NotFitted("MinMaxScaler not fitted".into()))?;
        let mx = self.data_max.as_ref().unwrap();
        let (n, d) = check_2d(x, "X")?;
        let (lo, hi) = self.feature_range;
        let data = x.to_vec();
        let mut out = vec![0.0; n * d];
        for i in 0..n {
            for j in 0..d {
                let denom = mx[j] - mn[j];
                let scaled = if denom.abs() < 1e-15 {
                    0.0
                } else {
                    (data[i * d + j] - mn[j]) / denom
                };
                out[i * d + j] = scaled * (hi - lo) + lo;
            }
        }
        matrix_from((n, d), out)
    }
}

#[derive(Clone, Debug, Default)]
pub struct RobustScaler {
    pub center: Option<Vec<f64>>,
    pub scale: Option<Vec<f64>>,
}

impl RobustScaler {
    pub fn new() -> Self {
        Self::default()
    }
}

fn percentile_sorted(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let n = sorted.len();
    let pos = q * (n - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        let w = pos - lo as f64;
        sorted[lo] * (1.0 - w) + sorted[hi] * w
    }
}

impl Estimator for RobustScaler {
    fn fit(&mut self, x: &NdArray, _y: Option<&NdArray>) -> LearnResult<()> {
        let (n, d) = check_2d(x, "X")?;
        let data = x.to_vec();
        let mut center = vec![0.0; d];
        let mut scale = vec![0.0; d];
        for j in 0..d {
            let mut col: Vec<f64> = (0..n).map(|i| data[i * d + j]).collect();
            col.sort_by(|a, b| a.partial_cmp(b).unwrap());
            center[j] = percentile_sorted(&col, 0.5);
            let q1 = percentile_sorted(&col, 0.25);
            let q3 = percentile_sorted(&col, 0.75);
            scale[j] = (q3 - q1).max(1e-12);
        }
        self.center = Some(center);
        self.scale = Some(scale);
        Ok(())
    }
}

impl Transformer for RobustScaler {
    fn transform(&self, x: &NdArray) -> LearnResult<NdArray> {
        let c = self
            .center
            .as_ref()
            .ok_or_else(|| LearnError::NotFitted("RobustScaler not fitted".into()))?;
        let s = self.scale.as_ref().unwrap();
        let (n, d) = check_2d(x, "X")?;
        let data = x.to_vec();
        let mut out = vec![0.0; n * d];
        for i in 0..n {
            for j in 0..d {
                out[i * d + j] = (data[i * d + j] - c[j]) / s[j];
            }
        }
        matrix_from((n, d), out)
    }
}

#[derive(Clone, Debug)]
pub struct Normalizer {
    pub norm: NormKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NormKind {
    L1,
    L2,
    Max,
}

impl Default for Normalizer {
    fn default() -> Self {
        Self { norm: NormKind::L2 }
    }
}

impl Normalizer {
    pub fn new(norm: NormKind) -> Self {
        Self { norm }
    }
}

impl Estimator for Normalizer {
    fn fit(&mut self, _x: &NdArray, _y: Option<&NdArray>) -> LearnResult<()> {
        Ok(())
    }
}

impl Transformer for Normalizer {
    fn transform(&self, x: &NdArray) -> LearnResult<NdArray> {
        let (n, d) = check_2d(x, "X")?;
        let data = x.to_vec();
        let mut out = data.clone();
        for i in 0..n {
            let row = &mut out[i * d..(i + 1) * d];
            let denom = match self.norm {
                NormKind::L1 => row.iter().map(|v| v.abs()).sum::<f64>(),
                NormKind::L2 => row.iter().map(|v| v * v).sum::<f64>().sqrt(),
                NormKind::Max => row.iter().map(|v| v.abs()).fold(0.0, f64::max),
            }
            .max(1e-15);
            for v in row.iter_mut() {
                *v /= denom;
            }
        }
        matrix_from((n, d), out)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Binarizer {
    pub threshold: f64,
}

impl Binarizer {
    pub fn new(threshold: f64) -> Self {
        Self { threshold }
    }
}

impl Estimator for Binarizer {
    fn fit(&mut self, _x: &NdArray, _y: Option<&NdArray>) -> LearnResult<()> {
        Ok(())
    }
}

impl Transformer for Binarizer {
    fn transform(&self, x: &NdArray) -> LearnResult<NdArray> {
        let (n, d) = check_2d(x, "X")?;
        let mut out = x.to_vec();
        for v in out.iter_mut() {
            *v = if *v > self.threshold { 1.0 } else { 0.0 };
        }
        matrix_from((n, d), out)
    }
}

#[derive(Clone, Debug, Default)]
pub struct SimpleImputer {
    pub strategy: ImputeStrategy,
    pub statistics: Option<Vec<f64>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ImputeStrategy {
    #[default]
    Mean,
    Median,
    MostFrequent,
    Constant,
}

impl SimpleImputer {
    pub fn new(strategy: ImputeStrategy) -> Self {
        Self {
            strategy,
            statistics: None,
        }
    }
}

impl Estimator for SimpleImputer {
    fn fit(&mut self, x: &NdArray, _y: Option<&NdArray>) -> LearnResult<()> {
        let (n, d) = check_2d(x, "X")?;
        let data = x.to_vec();
        let mut stats = vec![0.0; d];
        for j in 0..d {
            let mut vals: Vec<f64> = (0..n)
                .map(|i| data[i * d + j])
                .filter(|v| v.is_finite())
                .collect();
            if vals.is_empty() {
                stats[j] = 0.0;
                continue;
            }
            match self.strategy {
                ImputeStrategy::Mean => {
                    stats[j] = vals.iter().sum::<f64>() / vals.len() as f64;
                }
                ImputeStrategy::Median => {
                    vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
                    stats[j] = percentile_sorted(&vals, 0.5);
                }
                ImputeStrategy::MostFrequent => {
                    vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
                    let mut best = vals[0];
                    let mut best_c = 1usize;
                    let mut cur = vals[0];
                    let mut cur_c = 1usize;
                    for &v in vals.iter().skip(1) {
                        if (v - cur).abs() < 1e-12 {
                            cur_c += 1;
                        } else {
                            if cur_c > best_c {
                                best_c = cur_c;
                                best = cur;
                            }
                            cur = v;
                            cur_c = 1;
                        }
                    }
                    if cur_c > best_c {
                        best = cur;
                    }
                    stats[j] = best;
                }
                ImputeStrategy::Constant => stats[j] = 0.0,
            }
        }
        self.statistics = Some(stats);
        Ok(())
    }
}

impl Transformer for SimpleImputer {
    fn transform(&self, x: &NdArray) -> LearnResult<NdArray> {
        let stats = self
            .statistics
            .as_ref()
            .ok_or_else(|| LearnError::NotFitted("SimpleImputer not fitted".into()))?;
        let (n, d) = check_2d(x, "X")?;
        let mut out = x.to_vec();
        for i in 0..n {
            for j in 0..d {
                if !out[i * d + j].is_finite() {
                    out[i * d + j] = stats[j];
                }
            }
        }
        matrix_from((n, d), out)
    }
}

#[derive(Clone, Debug, Default)]
pub struct LabelEncoder {
    pub classes: Option<Vec<f64>>,
}

impl LabelEncoder {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Estimator for LabelEncoder {
    fn fit(&mut self, x: &NdArray, y: Option<&NdArray>) -> LearnResult<()> {
        let src = y.unwrap_or(x);
        let vals = y_as_vec(src)?;
        self.classes = Some(unique_labels(&vals));
        Ok(())
    }
}

impl Transformer for LabelEncoder {
    fn transform(&self, x: &NdArray) -> LearnResult<NdArray> {
        let classes = self
            .classes
            .as_ref()
            .ok_or_else(|| LearnError::NotFitted("LabelEncoder not fitted".into()))?;
        let vals = y_as_vec(x)?;
        let mut out = Vec::with_capacity(vals.len());
        for v in vals {
            let idx = classes
                .iter()
                .position(|&c| (c - v).abs() < 1e-12)
                .ok_or_else(|| LearnError::Error(format!("unseen label {v}")))?;
            out.push(idx as f64);
        }
        vector_from(out)
    }
}

#[derive(Clone, Debug, Default)]
pub struct OrdinalEncoder {
    pub categories: Option<Vec<Vec<f64>>>,
}

impl OrdinalEncoder {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Estimator for OrdinalEncoder {
    fn fit(&mut self, x: &NdArray, _y: Option<&NdArray>) -> LearnResult<()> {
        let (n, d) = check_2d(x, "X")?;
        let data = x.to_vec();
        let mut cats = Vec::with_capacity(d);
        for j in 0..d {
            let col: Vec<f64> = (0..n).map(|i| data[i * d + j]).collect();
            cats.push(unique_labels(&col));
        }
        self.categories = Some(cats);
        Ok(())
    }
}

impl Transformer for OrdinalEncoder {
    fn transform(&self, x: &NdArray) -> LearnResult<NdArray> {
        let cats = self
            .categories
            .as_ref()
            .ok_or_else(|| LearnError::NotFitted("OrdinalEncoder not fitted".into()))?;
        let (n, d) = check_2d(x, "X")?;
        let data = x.to_vec();
        let mut out = vec![0.0; n * d];
        for i in 0..n {
            for j in 0..d {
                let v = data[i * d + j];
                let idx = cats[j]
                    .iter()
                    .position(|&c| (c - v).abs() < 1e-12)
                    .ok_or_else(|| LearnError::Error(format!("unseen category {v}")))?;
                out[i * d + j] = idx as f64;
            }
        }
        matrix_from((n, d), out)
    }
}

#[derive(Clone, Debug, Default)]
pub struct OneHotEncoder {
    pub categories: Option<Vec<Vec<f64>>>,
}

impl OneHotEncoder {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Estimator for OneHotEncoder {
    fn fit(&mut self, x: &NdArray, _y: Option<&NdArray>) -> LearnResult<()> {
        let (n, d) = check_2d(x, "X")?;
        let data = x.to_vec();
        let mut cats = Vec::with_capacity(d);
        for j in 0..d {
            let col: Vec<f64> = (0..n).map(|i| data[i * d + j]).collect();
            cats.push(unique_labels(&col));
        }
        self.categories = Some(cats);
        Ok(())
    }
}

impl Transformer for OneHotEncoder {
    fn transform(&self, x: &NdArray) -> LearnResult<NdArray> {
        let cats = self
            .categories
            .as_ref()
            .ok_or_else(|| LearnError::NotFitted("OneHotEncoder not fitted".into()))?;
        let (n, d) = check_2d(x, "X")?;
        let data = x.to_vec();
        let out_d: usize = cats.iter().map(|c| c.len()).sum();
        let mut out = vec![0.0; n * out_d];
        for i in 0..n {
            let mut col = 0usize;
            for j in 0..d {
                let v = data[i * d + j];
                let idx = cats[j]
                    .iter()
                    .position(|&c| (c - v).abs() < 1e-12)
                    .ok_or_else(|| LearnError::Error(format!("unseen category {v}")))?;
                out[i * out_d + col + idx] = 1.0;
                col += cats[j].len();
            }
        }
        matrix_from((n, out_d), out)
    }
}

#[derive(Clone, Debug)]
pub struct PolynomialFeatures {
    pub degree: usize,
    pub include_bias: bool,
    n_features_in: Option<usize>,
}

impl Default for PolynomialFeatures {
    fn default() -> Self {
        Self {
            degree: 2,
            include_bias: true,
            n_features_in: None,
        }
    }
}

impl PolynomialFeatures {
    pub fn new(degree: usize, include_bias: bool) -> Self {
        Self {
            degree,
            include_bias,
            n_features_in: None,
        }
    }

    fn combos(d: usize, degree: usize) -> Vec<Vec<usize>> {
        let mut out = Vec::new();
        fn rec(
            start: usize,
            d: usize,
            left: usize,
            cur: &mut Vec<usize>,
            out: &mut Vec<Vec<usize>>,
        ) {
            if left == 0 {
                out.push(cur.clone());
                return;
            }
            for i in start..d {
                cur.push(i);
                rec(i, d, left - 1, cur, out);
                cur.pop();
            }
        }
        for deg in 1..=degree {
            let mut cur = Vec::new();
            rec(0, d, deg, &mut cur, &mut out);
        }
        out
    }
}

impl Estimator for PolynomialFeatures {
    fn fit(&mut self, x: &NdArray, _y: Option<&NdArray>) -> LearnResult<()> {
        let (_, d) = check_2d(x, "X")?;
        self.n_features_in = Some(d);
        Ok(())
    }
}

impl Transformer for PolynomialFeatures {
    fn transform(&self, x: &NdArray) -> LearnResult<NdArray> {
        let d_in = self
            .n_features_in
            .ok_or_else(|| LearnError::NotFitted("PolynomialFeatures not fitted".into()))?;
        let (n, d) = check_2d(x, "X")?;
        if d != d_in {
            return Err(LearnError::Shape("feature count mismatch".into()));
        }
        let combos = Self::combos(d, self.degree);
        let out_d = combos.len() + if self.include_bias { 1 } else { 0 };
        let data = x.to_vec();
        let mut out = vec![0.0; n * out_d];
        for i in 0..n {
            let mut c = 0usize;
            if self.include_bias {
                out[i * out_d] = 1.0;
                c = 1;
            }
            for comb in &combos {
                let mut v = 1.0;
                for &j in comb {
                    v *= data[i * d + j];
                }
                out[i * out_d + c] = v;
                c += 1;
            }
        }
        matrix_from((n, out_d), out)
    }
}
