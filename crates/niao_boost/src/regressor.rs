//! GBRegressor — histogram GBDT for regression.

use crate::booster::{Booster, Dataset, ImportanceKind};
use crate::error::{BoostError, BoostResult};
use crate::objective::{SquaredError, TaskKind};
use crate::params::BoosterParams;
use crate::serialize;

/// Gradient boosting regressor (squared error).
#[derive(Clone, Debug)]
pub struct GBRegressor {
    pub params: BoosterParams,
    pub booster: Booster,
}

impl Default for GBRegressor {
    fn default() -> Self {
        Self::new(BoosterParams::default()).unwrap()
    }
}

impl GBRegressor {
    pub fn new(params: BoosterParams) -> BoostResult<Self> {
        params.validate()?;
        Ok(Self {
            params: params.clone(),
            booster: Booster::new(params, TaskKind::Regression, 1)?,
        })
    }

    pub fn fit(
        &mut self,
        x: &[f64],
        n_rows: usize,
        n_features: usize,
        y: &[f64],
    ) -> BoostResult<()> {
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
        let objective = SquaredError;
        self.booster.fit(
            &objective,
            &train,
            y,
            eval_set.as_ref().map(|(d, y)| (d, *y)),
        )
    }

    pub fn predict(&self, x: &[f64], n_rows: usize, n_features: usize) -> BoostResult<Vec<f64>> {
        if !self.booster.fitted {
            return Err(BoostError::NotFitted);
        }
        let data = Dataset::from_matrix(x, n_rows, n_features, self.params.max_bins)?;
        self.booster.predict(&data)
    }

    pub fn score(
        &self,
        x: &[f64],
        n_rows: usize,
        n_features: usize,
        y: &[f64],
    ) -> BoostResult<f64> {
        let preds = self.predict(x, n_rows, n_features)?;
        Ok(r2_score(&preds, y))
    }

    pub fn feature_importance(&self, kind: ImportanceKind) -> BoostResult<Vec<f64>> {
        self.booster.feature_importance(kind)
    }

    pub fn save_model(&self, path: &str) -> BoostResult<()> {
        serialize::save_model(&self.booster, path)
    }

    pub fn load_into(&mut self, path: &str) -> BoostResult<()> {
        self.booster = serialize::load_model(path)?;
        self.params = self.booster.params.clone();
        Ok(())
    }
}

pub fn r2_score(preds: &[f64], labels: &[f64]) -> f64 {
    if preds.len() != labels.len() || labels.is_empty() {
        return 0.0;
    }
    let mean: f64 = labels.iter().sum::<f64>() / labels.len() as f64;
    let ss_tot: f64 = labels.iter().map(|y| (y - mean).powi(2)).sum();
    if ss_tot <= 0.0 {
        return 1.0;
    }
    let ss_res: f64 = preds.iter().zip(labels).map(|(p, y)| (y - p).powi(2)).sum();
    1.0 - ss_res / ss_tot
}
