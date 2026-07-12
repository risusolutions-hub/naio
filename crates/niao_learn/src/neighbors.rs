//! k-nearest neighbors (brute force).

use crate::error::{LearnError, LearnResult};
use crate::metrics::{accuracy, r2_score};
use crate::traits::{Estimator, Predictor, Scorer};
use crate::utils::{check_2d, check_xy, squared_dist, unique_labels, vector_from, y_as_vec};
use niao_num::NdArray;

#[derive(Clone, Debug)]
pub struct KNeighborsClassifier {
    pub n_neighbors: usize,
    x_train: Option<Vec<f64>>,
    y_train: Option<Vec<f64>>,
    n_features: usize,
}

impl Default for KNeighborsClassifier {
    fn default() -> Self {
        Self {
            n_neighbors: 5,
            x_train: None,
            y_train: None,
            n_features: 0,
        }
    }
}

impl KNeighborsClassifier {
    pub fn new(n_neighbors: usize) -> Self {
        Self {
            n_neighbors,
            ..Default::default()
        }
    }
}

impl Estimator for KNeighborsClassifier {
    fn fit(&mut self, x: &NdArray, y: Option<&NdArray>) -> LearnResult<()> {
        let y = y.ok_or_else(|| LearnError::Error("y required".into()))?;
        let (n, d) = check_xy(x, y)?;
        if self.n_neighbors == 0 || self.n_neighbors > n {
            return Err(LearnError::Error("invalid n_neighbors".into()));
        }
        self.x_train = Some(x.to_vec());
        self.y_train = Some(y_as_vec(y)?);
        self.n_features = d;
        let _ = n;
        Ok(())
    }
}

fn knn_vote(dists: &[(f64, f64)], k: usize) -> f64 {
    let top: Vec<(f64, f64)> = dists.iter().copied().take(k).collect();
    // majority vote
    let labels = unique_labels(&top.iter().map(|(_, y)| *y).collect::<Vec<_>>());
    let mut best_y = top[0].1;
    let mut best_c = 0usize;
    for &lab in &labels {
        let c = top.iter().filter(|(_, y)| (*y - lab).abs() < 1e-12).count();
        if c > best_c {
            best_c = c;
            best_y = lab;
        }
    }
    let _ = best_c;
    best_y
}

impl Predictor for KNeighborsClassifier {
    fn predict(&self, x: &NdArray) -> LearnResult<NdArray> {
        let xt = self
            .x_train
            .as_ref()
            .ok_or_else(|| LearnError::NotFitted("KNeighborsClassifier not fitted".into()))?;
        let yt = self.y_train.as_ref().unwrap();
        let (n, d) = check_2d(x, "X")?;
        if d != self.n_features {
            return Err(LearnError::Shape("feature count mismatch".into()));
        }
        let xv = x.to_vec();
        let n_train = yt.len();
        let mut out = vec![0.0; n];
        let mut dists = Vec::with_capacity(n_train);
        for i in 0..n {
            dists.clear();
            let qi = &xv[i * d..(i + 1) * d];
            for t in 0..n_train {
                let dist = squared_dist(qi, &xt[t * d..(t + 1) * d]);
                dists.push((dist, yt[t]));
            }
            dists.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            out[i] = knn_vote(&dists, self.n_neighbors);
        }
        vector_from(out)
    }
}

impl Scorer for KNeighborsClassifier {
    fn score(&self, x: &NdArray, y: &NdArray) -> LearnResult<f64> {
        accuracy(y, &self.predict(x)?)
    }
}

#[derive(Clone, Debug)]
pub struct KNeighborsRegressor {
    pub n_neighbors: usize,
    x_train: Option<Vec<f64>>,
    y_train: Option<Vec<f64>>,
    n_features: usize,
}

impl Default for KNeighborsRegressor {
    fn default() -> Self {
        Self {
            n_neighbors: 5,
            x_train: None,
            y_train: None,
            n_features: 0,
        }
    }
}

impl KNeighborsRegressor {
    pub fn new(n_neighbors: usize) -> Self {
        Self {
            n_neighbors,
            ..Default::default()
        }
    }
}

impl Estimator for KNeighborsRegressor {
    fn fit(&mut self, x: &NdArray, y: Option<&NdArray>) -> LearnResult<()> {
        let y = y.ok_or_else(|| LearnError::Error("y required".into()))?;
        let (_, d) = check_xy(x, y)?;
        self.x_train = Some(x.to_vec());
        self.y_train = Some(y_as_vec(y)?);
        self.n_features = d;
        Ok(())
    }
}

impl Predictor for KNeighborsRegressor {
    fn predict(&self, x: &NdArray) -> LearnResult<NdArray> {
        let xt = self
            .x_train
            .as_ref()
            .ok_or_else(|| LearnError::NotFitted("KNeighborsRegressor not fitted".into()))?;
        let yt = self.y_train.as_ref().unwrap();
        let (n, d) = check_2d(x, "X")?;
        let xv = x.to_vec();
        let n_train = yt.len();
        let mut out = vec![0.0; n];
        let mut dists = Vec::with_capacity(n_train);
        for i in 0..n {
            dists.clear();
            let qi = &xv[i * d..(i + 1) * d];
            for t in 0..n_train {
                dists.push((squared_dist(qi, &xt[t * d..(t + 1) * d]), yt[t]));
            }
            dists.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            let s: f64 = dists.iter().take(self.n_neighbors).map(|(_, y)| *y).sum();
            out[i] = s / self.n_neighbors as f64;
        }
        vector_from(out)
    }
}

impl Scorer for KNeighborsRegressor {
    fn score(&self, x: &NdArray, y: &NdArray) -> LearnResult<f64> {
        r2_score(y, &self.predict(x)?)
    }
}
