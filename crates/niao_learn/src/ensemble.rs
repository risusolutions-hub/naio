//! Ensemble methods: RandomForest.

use crate::error::{LearnError, LearnResult};
use crate::metrics::{accuracy, r2_score};
use crate::traits::{Estimator, Predictor, Scorer};
use crate::tree::{DecisionTreeClassifier, DecisionTreeRegressor};
use crate::utils::{check_2d, check_xy, unique_labels, vector_from, y_as_vec};
use niao_num::NdArray;
use niao_rand::{Rng, SeedableRng, SliceRandom, StdRng};
use std::sync::Mutex;
use std::thread;

#[derive(Clone, Debug)]
pub struct RandomForestClassifier {
    pub n_estimators: usize,
    pub max_depth: usize,
    pub min_samples_split: usize,
    pub max_features: Option<usize>,
    pub random_state: u64,
    trees: Vec<DecisionTreeClassifier>,
    classes: Option<Vec<f64>>,
    n_features: usize,
}

impl Default for RandomForestClassifier {
    fn default() -> Self {
        Self {
            n_estimators: 10,
            max_depth: usize::MAX,
            min_samples_split: 2,
            max_features: None,
            random_state: 42,
            trees: Vec::new(),
            classes: None,
            n_features: 0,
        }
    }
}

impl RandomForestClassifier {
    pub fn new(n_estimators: usize, max_depth: usize, random_state: u64) -> Self {
        Self {
            n_estimators,
            max_depth,
            random_state,
            ..Default::default()
        }
    }
}

impl Estimator for RandomForestClassifier {
    fn fit(&mut self, x: &NdArray, y: Option<&NdArray>) -> LearnResult<()> {
        let y = y.ok_or_else(|| LearnError::Error("y required".into()))?;
        let (n, d) = check_xy(x, y)?;
        let xv = x.to_vec();
        let yv = y_as_vec(y)?;
        let classes = unique_labels(&yv);
        let mf = self
            .max_features
            .unwrap_or_else(|| ((d as f64).sqrt() as usize).max(1));

        let results: Mutex<Result<Vec<(usize, DecisionTreeClassifier)>, LearnError>> =
            Mutex::new(Ok(Vec::new()));
        let n_est = self.n_estimators;
        let max_depth = self.max_depth;
        let min_split = self.min_samples_split;
        let seed0 = self.random_state;

        let workers = thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4)
            .min(n_est)
            .max(1);
        let chunk = (n_est + workers - 1) / workers;
        thread::scope(|scope| {
            for w in 0..workers {
                let start = w * chunk;
                let end = (start + chunk).min(n_est);
                if start >= end {
                    continue;
                }
                let xv = &xv;
                let yv = &yv;
                let results = &results;
                scope.spawn(move || {
                    for t in start..end {
                        let mut rng = StdRng::seed_from_u64(seed0.wrapping_add(t as u64 * 9973));
                        let sample: Vec<usize> =
                            (0..n).map(|_| rng.gen_range_usize(0, n)).collect();
                        let mut feats: Vec<usize> = (0..d).collect();
                        feats.shuffle(&mut rng);
                        feats.truncate(mf);
                        let n_feat = feats.len();
                        let mut sub_x = vec![0.0; n * n_feat];
                        let mut sub_y = vec![0.0; n];
                        for (i, &si) in sample.iter().enumerate() {
                            sub_y[i] = yv[si];
                            for (fi, &f) in feats.iter().enumerate() {
                                sub_x[i * n_feat + fi] = xv[si * d + f];
                            }
                        }
                        let Ok(x_arr) = NdArray::from_vec(vec![n, n_feat], sub_x) else {
                            *results.lock().unwrap() =
                                Err(LearnError::Error("RF x shape".into()));
                            return;
                        };
                        let Ok(y_arr) = NdArray::from_vec(vec![n], sub_y) else {
                            *results.lock().unwrap() =
                                Err(LearnError::Error("RF y shape".into()));
                            return;
                        };
                        let mut tree = DecisionTreeClassifier::new(max_depth);
                        tree.min_samples_split = min_split;
                        if let Err(e) = tree.fit(&x_arr, Some(&y_arr)) {
                            *results.lock().unwrap() = Err(e);
                            return;
                        }
                        tree.remap_features(&feats);
                        tree.n_features = d;
                        if let Ok(ref mut v) = *results.lock().unwrap() {
                            v.push((t, tree));
                        }
                    }
                });
            }
        });

        let mut pairs = results.into_inner().unwrap()?;
        pairs.sort_by_key(|(i, _)| *i);
        self.trees = pairs.into_iter().map(|(_, t)| t).collect();
        self.classes = Some(classes);
        self.n_features = d;
        Ok(())
    }
}

impl Predictor for RandomForestClassifier {
    fn predict(&self, x: &NdArray) -> LearnResult<NdArray> {
        if self.trees.is_empty() {
            return Err(LearnError::NotFitted(
                "RandomForestClassifier not fitted".into(),
            ));
        }
        let classes = self.classes.as_ref().unwrap();
        let (n, d) = check_2d(x, "X")?;
        if d != self.n_features {
            return Err(LearnError::Shape("feature count mismatch".into()));
        }
        let mut votes = vec![0usize; n * classes.len()];
        for tree in &self.trees {
            let pred = tree.predict(x)?;
            let pv = pred.to_vec();
            for i in 0..n {
                if let Some(ci) = classes.iter().position(|&c| (c - pv[i]).abs() < 1e-12) {
                    votes[i * classes.len() + ci] += 1;
                }
            }
        }
        let mut out = vec![0.0; n];
        for i in 0..n {
            let mut best = 0;
            let mut best_v = 0usize;
            for c in 0..classes.len() {
                let v = votes[i * classes.len() + c];
                if v > best_v {
                    best_v = v;
                    best = c;
                }
            }
            out[i] = classes[best];
        }
        vector_from(out)
    }
}

impl Scorer for RandomForestClassifier {
    fn score(&self, x: &NdArray, y: &NdArray) -> LearnResult<f64> {
        accuracy(y, &self.predict(x)?)
    }
}

#[derive(Clone, Debug)]
pub struct RandomForestRegressor {
    pub n_estimators: usize,
    pub max_depth: usize,
    pub random_state: u64,
    trees: Vec<DecisionTreeRegressor>,
    n_features: usize,
}

impl Default for RandomForestRegressor {
    fn default() -> Self {
        Self {
            n_estimators: 10,
            max_depth: usize::MAX,
            random_state: 42,
            trees: Vec::new(),
            n_features: 0,
        }
    }
}

impl RandomForestRegressor {
    pub fn new(n_estimators: usize, max_depth: usize, random_state: u64) -> Self {
        Self {
            n_estimators,
            max_depth,
            random_state,
            ..Default::default()
        }
    }
}

impl Estimator for RandomForestRegressor {
    fn fit(&mut self, x: &NdArray, y: Option<&NdArray>) -> LearnResult<()> {
        let y = y.ok_or_else(|| LearnError::Error("y required".into()))?;
        let (n, d) = check_xy(x, y)?;
        let xv = x.to_vec();
        let yv = y_as_vec(y)?;
        let mf = ((d as f64).sqrt() as usize).max(1);
        self.trees.clear();
        for t in 0..self.n_estimators {
            let mut rng = StdRng::seed_from_u64(self.random_state.wrapping_add(t as u64 * 9973));
            let sample: Vec<usize> = (0..n).map(|_| rng.gen_range_usize(0, n)).collect();
            let mut feats: Vec<usize> = (0..d).collect();
            feats.shuffle(&mut rng);
            feats.truncate(mf);
            let nf = feats.len();
            let mut sub_x = vec![0.0; n * nf];
            let mut sub_y = vec![0.0; n];
            for (i, &si) in sample.iter().enumerate() {
                sub_y[i] = yv[si];
                for (fi, &f) in feats.iter().enumerate() {
                    sub_x[i * nf + fi] = xv[si * d + f];
                }
            }
            let x_arr =
                NdArray::from_vec(vec![n, nf], sub_x).map_err(|e| LearnError::Error(e.to_string()))?;
            let y_arr =
                NdArray::from_vec(vec![n], sub_y).map_err(|e| LearnError::Error(e.to_string()))?;
            let mut tree = DecisionTreeRegressor::new(self.max_depth);
            tree.fit(&x_arr, Some(&y_arr))?;
            tree.remap_features(&feats);
            tree.n_features = d;
            self.trees.push(tree);
        }
        self.n_features = d;
        Ok(())
    }
}

impl Predictor for RandomForestRegressor {
    fn predict(&self, x: &NdArray) -> LearnResult<NdArray> {
        if self.trees.is_empty() {
            return Err(LearnError::NotFitted(
                "RandomForestRegressor not fitted".into(),
            ));
        }
        let (n, _) = check_2d(x, "X")?;
        let mut acc = vec![0.0; n];
        for tree in &self.trees {
            let p = tree.predict(x)?.to_vec();
            for i in 0..n {
                acc[i] += p[i];
            }
        }
        let m = self.trees.len() as f64;
        for v in acc.iter_mut() {
            *v /= m;
        }
        vector_from(acc)
    }
}

impl Scorer for RandomForestRegressor {
    fn score(&self, x: &NdArray, y: &NdArray) -> LearnResult<f64> {
        r2_score(y, &self.predict(x)?)
    }
}
