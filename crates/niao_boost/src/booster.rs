//! Gradient boosting engine: fit, predict, early stopping.

use crate::binning::BinnedMatrix;
use crate::error::{BoostError, BoostResult};
use crate::objective::{Objective, TaskKind};
use crate::params::BoosterParams;
use crate::tree::{build_tree, Tree};
use std::sync::Arc;

/// Training dataset with pre-binned features.
#[derive(Clone, Debug)]
pub struct Dataset {
    pub binned: BinnedMatrix,
    pub raw_x: Arc<Vec<f64>>,
    pub n_rows: usize,
    pub n_features: usize,
}

impl Dataset {
    pub fn from_matrix(x: &[f64], n_rows: usize, n_features: usize, max_bins: usize) -> BoostResult<Self> {
        if x.len() != n_rows * n_features {
            return Err(BoostError::Shape(format!(
                "X length {} != {} * {}",
                x.len(),
                n_rows,
                n_features
            )));
        }
        let binned = BinnedMatrix::from_matrix(x, n_rows, n_features, max_bins)?;
        Ok(Self {
            binned,
            raw_x: Arc::new(x.to_vec()),
            n_rows,
            n_features,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportanceKind {
    Gain,
    Split,
    Cover,
}

/// Trained gradient boosting model.
#[derive(Clone, Debug)]
pub struct Booster {
    pub trees: Vec<Tree>,
    pub params: BoosterParams,
    pub task: TaskKind,
    pub num_class: usize,
    pub base_score: f64,
    pub feature_importance_gain: Vec<f64>,
    pub feature_importance_split: Vec<u32>,
    pub feature_importance_cover: Vec<u32>,
    pub best_iteration: usize,
    pub eval_log: Vec<(usize, f64)>,
    pub fitted: bool,
    pub binned_template: BinnedMatrix,
}

impl Booster {
    pub fn new(params: BoosterParams, task: TaskKind, num_class: usize) -> BoostResult<Self> {
        params.validate()?;
        Ok(Self {
            trees: Vec::new(),
            params,
            task,
            num_class,
            base_score: 0.0,
            feature_importance_gain: Vec::new(),
            feature_importance_split: Vec::new(),
            feature_importance_cover: Vec::new(),
            best_iteration: 0,
            eval_log: Vec::new(),
            fitted: false,
            binned_template: BinnedMatrix {
                n_rows: 0,
                n_features: 0,
                max_bins: 0,
                bins: Vec::new(),
                thresholds: Vec::new(),
                missing: Vec::new(),
            },
        })
    }

    pub fn fit<O: Objective>(
        &mut self,
        objective: &O,
        train: &Dataset,
        labels: &[f64],
        eval_set: Option<(&Dataset, &[f64])>,
    ) -> BoostResult<()> {
        if labels.len() != train.n_rows {
            return Err(BoostError::Shape(format!(
                "y length {} != X rows {}",
                labels.len(),
                train.n_rows
            )));
        }
        self.params.validate()?;
        self.task = objective.kind();
        self.num_class = match self.task {
            TaskKind::Multiclass => objective.num_output_trees(),
            _ => 1,
        };

        let n_rows = train.n_rows;
        let n_features = train.n_features;
        self.feature_importance_gain = vec![0.0; n_features];
        self.feature_importance_split = vec![0; n_features];
        self.feature_importance_cover = vec![0; n_features];
        self.binned_template = train.binned.clone();
        self.trees.clear();

        let pred_len = match self.task {
            TaskKind::Multiclass => n_rows * self.num_class,
            _ => n_rows,
        };
        let mut preds = vec![0.0; pred_len];
        objective.init_predictions(labels, n_rows, &mut preds)?;

        let mut grad = vec![0.0; pred_len];
        let mut hess = vec![0.0; pred_len];
        let mut best_score = f64::INFINITY;
        let mut rounds_without_improve = 0usize;
        let mut best_iter = 0usize;

        let mut rng = self.params.seed;

        for round in 0..self.params.n_estimators {
            objective.gradients(labels, &preds, &mut grad, &mut hess)?;

            let feature_subset = sample_features(n_features, self.params.colsample, &mut rng);
            let row_mask = sample_rows(n_rows, self.params.subsample, &mut rng);

            for class_idx in 0..objective.num_output_trees() {
                let (g, h) = match self.task {
                    TaskKind::Multiclass => {
                        let mut gc = vec![0.0; n_rows];
                        let mut hc = vec![0.0; n_rows];
                        for r in 0..n_rows {
                            if row_mask[r] {
                                gc[r] = grad[r * self.num_class + class_idx];
                                hc[r] = hess[r * self.num_class + class_idx];
                            } else {
                                gc[r] = 0.0;
                                hc[r] = 0.0;
                            }
                        }
                        (gc, hc)
                    }
                    _ => {
                        let mut gc = grad.clone();
                        let mut hc = hess.clone();
                        for r in 0..n_rows {
                            if !row_mask[r] {
                                gc[r] = 0.0;
                                hc[r] = 0.0;
                            }
                        }
                        (gc, hc)
                    }
                };

                let tree = build_tree(
                    &train.binned,
                    &g,
                    &h,
                    &self.params,
                    &feature_subset,
                    &mut self.feature_importance_gain,
                    &mut self.feature_importance_split,
                    &mut self.feature_importance_cover,
                );
                self.trees.push(tree);
            }

            let round_start = round * objective.num_output_trees();
            let round_trees = &self.trees[round_start..];
            match self.task {
                TaskKind::Multiclass => {
                    for (ci, tree) in round_trees.iter().enumerate() {
                        for r in 0..n_rows {
                            preds[r * self.num_class + ci] += self.params.learning_rate
                                * tree.predict_one(&train.binned, r);
                        }
                    }
                }
                _ => {
                    let tree = round_trees.last().unwrap();
                    for r in 0..n_rows {
                        preds[r] += self.params.learning_rate * tree.predict_one(&train.binned, r);
                    }
                }
            }

            if let Some((eval_data, eval_y)) = eval_set {
                let score = eval_metric(
                    self.task,
                    eval_data,
                    eval_y,
                    &self.trees,
                    round + 1,
                    objective.num_output_trees(),
                    &self.params,
                )?;
                self.eval_log.push((round, score));
                if score < best_score {
                    best_score = score;
                    best_iter = round;
                    rounds_without_improve = 0;
                } else {
                    rounds_without_improve += 1;
                }
                if let Some(patience) = self.params.early_stopping_rounds {
                    if rounds_without_improve >= patience {
                        self.best_iteration = best_iter;
                        self.trees.truncate((best_iter + 1) * objective.num_output_trees());
                        self.fitted = true;
                        return Ok(());
                    }
                }
            }
        }

        self.best_iteration = if eval_set.is_some() {
            best_iter
        } else {
            self.params.n_estimators.saturating_sub(1)
        };
        self.fitted = true;
        Ok(())
    }

    pub fn predict(&self, data: &Dataset) -> BoostResult<Vec<f64>> {
        if !self.fitted {
            return Err(BoostError::NotFitted);
        }
        if data.n_features != self.binned_template.n_features {
            return Err(BoostError::Shape(format!(
                "X features {} != model features {}",
                data.n_features,
                self.binned_template.n_features
            )));
        }

        let n = data.n_rows;
        match self.task {
            TaskKind::Multiclass => {
                let mut out = vec![0.0; n * self.num_class];
                for tree in &self.trees {
                    // trees stored sequentially per class per round — apply all
                    let _ = tree;
                }
                apply_all_trees(
                    &self.trees,
                    &data.binned,
                    self.params.learning_rate,
                    self.num_class,
                    &mut out,
                );
                Ok(out)
            }
            TaskKind::Binary | TaskKind::Regression => {
                let mut out = vec![0.0; n];
                for tree in &self.trees {
                    for r in 0..n {
                        out[r] += self.params.learning_rate * tree.predict_one(&data.binned, r);
                    }
                }
                Ok(out)
            }
        }
    }

    pub fn predict_proba(&self, data: &Dataset) -> BoostResult<Vec<f64>> {
        let raw = self.predict(data)?;
        match self.task {
            TaskKind::Binary => {
                let mut out = vec![0.0; data.n_rows * 2];
                for (i, &logit) in raw.iter().enumerate() {
                    let p = sigmoid(logit);
                    out[i * 2] = 1.0 - p;
                    out[i * 2 + 1] = p;
                }
                Ok(out)
            }
            TaskKind::Multiclass => {
                let n = data.n_rows;
                let nc = self.num_class;
                let mut out = vec![0.0; n * nc];
                for i in 0..n {
                    let base = i * nc;
                    let mut maxv = f64::NEG_INFINITY;
                    for c in 0..nc {
                        maxv = maxv.max(raw[base + c]);
                    }
                    let mut sum = 0.0;
                    for c in 0..nc {
                        let v = (raw[base + c] - maxv).exp();
                        out[base + c] = v;
                        sum += v;
                    }
                    for c in 0..nc {
                        out[base + c] /= sum;
                    }
                }
                Ok(out)
            }
            TaskKind::Regression => Err(BoostError::Type(
                "predict_proba requires a classifier".into(),
            )),
        }
    }

    pub fn feature_importance(&self, kind: ImportanceKind) -> BoostResult<Vec<f64>> {
        if !self.fitted {
            return Err(BoostError::NotFitted);
        }
        let raw = match kind {
            ImportanceKind::Gain => self.feature_importance_gain.clone(),
            ImportanceKind::Split => self
                .feature_importance_split
                .iter()
                .map(|&x| x as f64)
                .collect(),
            ImportanceKind::Cover => self
                .feature_importance_cover
                .iter()
                .map(|&x| x as f64)
                .collect(),
        };
        let sum: f64 = raw.iter().sum();
        if sum <= 0.0 {
            return Ok(vec![0.0; raw.len()]);
        }
        Ok(raw.iter().map(|v| v / sum).collect())
    }
}

fn apply_all_trees(
    trees: &[Tree],
    data: &BinnedMatrix,
    eta: f64,
    num_class: usize,
    out: &mut [f64],
) {
    let n = data.n_rows;
    out.fill(0.0);
    let trees_per_round = num_class;
    if trees_per_round == 0 {
        return;
    }
    for chunk in trees.chunks(trees_per_round) {
        for (c, tree) in chunk.iter().enumerate() {
            for r in 0..n {
                out[r * num_class + c] += eta * tree.predict_one(data, r);
            }
        }
    }
}

fn eval_metric(
    task: TaskKind,
    data: &Dataset,
    labels: &[f64],
    trees: &[Tree],
    num_rounds: usize,
    trees_per_round: usize,
    params: &BoosterParams,
) -> BoostResult<f64> {
    let n = data.n_rows;
    let end = num_rounds * trees_per_round;
    let active = &trees[..end.min(trees.len())];
    let mut preds = vec![0.0; n];
    for tree in active {
        for r in 0..n {
            preds[r] += params.learning_rate * tree.predict_one(&data.binned, r);
        }
    }
    Ok(match task {
        TaskKind::Regression => rmse(&preds, labels),
        TaskKind::Binary => logloss_binary(&preds, labels),
        TaskKind::Multiclass => {
            let nc = trees_per_round.max(1);
            let mut full = vec![0.0; n * nc];
            apply_all_trees(active, &data.binned, params.learning_rate, nc, &mut full);
            logloss_multiclass(&full, labels, nc)
        }
    })
}

pub fn rmse(preds: &[f64], labels: &[f64]) -> f64 {
    let n = preds.len().min(labels.len()) as f64;
    if n == 0.0 {
        return 0.0;
    }
    let mse: f64 = preds
        .iter()
        .zip(labels)
        .map(|(p, y)| (p - y).powi(2))
        .sum::<f64>()
        / n;
    mse.sqrt()
}

pub fn logloss_binary(logits: &[f64], labels: &[f64]) -> f64 {
    let n = logits.len().min(labels.len()) as f64;
    if n == 0.0 {
        return 0.0;
    }
    logits
        .iter()
        .zip(labels)
        .map(|(logit, y)| {
            let p = sigmoid(*logit);
            let p = p.clamp(1e-15, 1.0 - 1e-15);
            -(*y * p.ln() + (1.0 - y) * (1.0 - p).ln())
        })
        .sum::<f64>()
        / n
}

pub fn logloss_multiclass(logits: &[f64], labels: &[f64], num_class: usize) -> f64 {
    let n = labels.len() as f64;
    if n == 0.0 {
        return 0.0;
    }
    labels
        .iter()
        .enumerate()
        .map(|(i, y)| {
            let yi = y.round() as usize;
            let base = i * num_class;
            let mut maxv = f64::NEG_INFINITY;
            for c in 0..num_class {
                maxv = maxv.max(logits[base + c]);
            }
            let mut sum = 0.0;
            for c in 0..num_class {
                sum += (logits[base + c] - maxv).exp();
            }
            let log_z = maxv + sum.ln();
            log_z - logits[base + yi.min(num_class - 1)]
        })
        .sum::<f64>()
        / n
}

pub fn auc_binary(logits: &[f64], labels: &[f64]) -> f64 {
    let mut pairs: Vec<(f64, f64)> = logits
        .iter()
        .zip(labels)
        .map(|(l, y)| (sigmoid(*l), *y))
        .collect();
    pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let n_pos = pairs.iter().filter(|(_, y)| *y > 0.5).count() as f64;
    let n_neg = pairs.len() as f64 - n_pos;
    if n_pos == 0.0 || n_neg == 0.0 {
        return 0.5;
    }
    let mut rank_sum = 0.0;
    for (i, (_, y)) in pairs.iter().enumerate() {
        if *y > 0.5 {
            rank_sum += (i + 1) as f64;
        }
    }
    (rank_sum - n_pos * (n_pos + 1.0) / 2.0) / (n_pos * n_neg)
}

#[inline]
pub fn sigmoid_pub(x: f64) -> f64 {
    sigmoid(x)
}

#[inline]
fn sigmoid(x: f64) -> f64 {
    if x >= 0.0 {
        1.0 / (1.0 + (-x).exp())
    } else {
        let z = x.exp();
        z / (1.0 + z)
    }
}

fn sample_features(n_features: usize, frac: f64, seed: &mut u64) -> Vec<usize> {
    if (frac - 1.0).abs() < 1e-12 {
        return (0..n_features).collect();
    }
    let k = ((n_features as f64 * frac).round() as usize).clamp(1, n_features);
    let mut idx: Vec<usize> = (0..n_features).collect();
    for i in (1..n_features).rev() {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let j = (*seed as usize) % (i + 1);
        idx.swap(i, j);
    }
    idx.truncate(k);
    idx.sort_unstable();
    idx
}

fn sample_rows(n_rows: usize, frac: f64, seed: &mut u64) -> Vec<bool> {
    if (frac - 1.0).abs() < 1e-12 {
        return vec![true; n_rows];
    }
    let mut mask = vec![false; n_rows];
    for i in 0..n_rows {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        mask[i] = (*seed as f64 / u64::MAX as f64) < frac;
    }
    if !mask.iter().any(|&m| m) {
        mask[0] = true;
    }
    mask
}
