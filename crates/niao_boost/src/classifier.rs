//! GBClassifier — histogram GBDT for binary and multiclass classification.

use crate::booster::{auc_binary, Booster, Dataset, ImportanceKind};
use crate::error::{BoostError, BoostResult};
use crate::objective::{Logistic, SoftmaxMulticlass, TaskKind};
use crate::params::BoosterParams;
use crate::serialize;

/// Gradient boosting classifier (logistic / softmax).
#[derive(Clone, Debug)]
pub struct GBClassifier {
    pub params: BoosterParams,
    pub num_class: usize,
    pub booster: Booster,
}

impl GBClassifier {
    pub fn new_binary(params: BoosterParams) -> BoostResult<Self> {
        params.validate()?;
        Ok(Self {
            params: params.clone(),
            num_class: 2,
            booster: Booster::new(params, TaskKind::Binary, 2)?,
        })
    }

    pub fn new_multiclass(params: BoosterParams, num_class: usize) -> BoostResult<Self> {
        params.validate()?;
        if num_class < 2 {
            return Err(BoostError::BadParam("num_class must be >= 2".into()));
        }
        Ok(Self {
            params: params.clone(),
            num_class,
            booster: Booster::new(params, TaskKind::Multiclass, num_class)?,
        })
    }

    pub fn fit(&mut self, x: &[f64], n_rows: usize, n_features: usize, y: &[f64]) -> BoostResult<()> {
        self.fit_with_eval(x, n_rows, n_features, y, None)
    }

    pub fn fit_with_eval(
        &mut self,
        x: &[f64],
        n_rows: usize,
        n_features: usize,
        y: &[f64],
        eval: Option<(&[f64], usize, usize, &[f64])>,
    ) -> BoostResult<()> {
        if y.len() != n_rows {
            return Err(BoostError::Shape(format!(
                "y length {} != n_rows {n_rows}",
                y.len()
            )));
        }
        let train = Dataset::from_matrix(x, n_rows, n_features, self.params.max_bins)?;
        self.booster.params = self.params.clone();
        let eval_set = eval.map(|(ex, er, ef, ey)| {
            let ds = Dataset::from_matrix(ex, er, ef, self.params.max_bins).unwrap();
            (ds, ey)
        });

        if self.num_class == 2 {
            let objective = Logistic;
            self.booster
                .fit(&objective, &train, y, eval_set.as_ref().map(|(d, y)| (d, *y)))
        } else {
            let objective = SoftmaxMulticlass::new(self.num_class)?;
            self.booster.num_class = self.num_class;
            self.booster
                .fit(&objective, &train, y, eval_set.as_ref().map(|(d, y)| (d, *y)))
        }
    }

    pub fn predict(&self, x: &[f64], n_rows: usize, n_features: usize) -> BoostResult<Vec<f64>> {
        if self.num_class == 2 {
            let logits = self.predict_logits(x, n_rows, n_features)?;
            Ok(logits
                .into_iter()
                .map(|l| if crate::booster::sigmoid_pub(l) > 0.5 { 1.0 } else { 0.0 })
                .collect())
        } else {
            let proba = self.predict_proba(x, n_rows, n_features)?;
            let nc = self.num_class;
            Ok((0..n_rows)
                .map(|i| {
                    let base = i * nc;
                    (0..nc)
                        .max_by(|&a, &b| {
                            proba[base + a]
                                .partial_cmp(&proba[base + b])
                                .unwrap_or(std::cmp::Ordering::Equal)
                        })
                        .unwrap_or(0) as f64
                })
                .collect())
        }
    }

    pub fn predict_logits(&self, x: &[f64], n_rows: usize, n_features: usize) -> BoostResult<Vec<f64>> {
        let data = Dataset::from_matrix(x, n_rows, n_features, self.params.max_bins)?;
        self.booster.predict(&data)
    }

    pub fn predict_proba(&self, x: &[f64], n_rows: usize, n_features: usize) -> BoostResult<Vec<f64>> {
        let data = Dataset::from_matrix(x, n_rows, n_features, self.params.max_bins)?;
        self.booster.predict_proba(&data)
    }

    pub fn score(&self, x: &[f64], n_rows: usize, n_features: usize, y: &[f64]) -> BoostResult<f64> {
        if self.num_class == 2 {
            let logits = self.predict_logits(x, n_rows, n_features)?;
            Ok(auc_binary(&logits, y))
        } else {
            let pred = self.predict(x, n_rows, n_features)?;
            Ok(accuracy(&pred, y))
        }
    }

    pub fn feature_importance(&self, kind: ImportanceKind) -> BoostResult<Vec<f64>> {
        self.booster.feature_importance(kind)
    }

    pub fn is_fitted(&self) -> bool {
        self.booster.fitted
    }

    pub fn save_model(&self, path: &str) -> BoostResult<()> {
        serialize::save_model(&self.booster, path)
    }
}

pub fn accuracy(preds: &[f64], labels: &[f64]) -> f64 {
    if preds.is_empty() {
        return 0.0;
    }
    preds
        .iter()
        .zip(labels)
        .filter(|(p, y)| (p.round() as i64) == (y.round() as i64))
        .count() as f64
        / preds.len() as f64
}
