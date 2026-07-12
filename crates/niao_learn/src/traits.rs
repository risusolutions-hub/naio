//! Estimator / Predictor / Transformer contracts.

use crate::error::LearnResult;
use niao_num::NdArray;

/// Fit an estimator on features `x` and optional targets `y`.
pub trait Estimator {
    fn fit(&mut self, x: &NdArray, y: Option<&NdArray>) -> LearnResult<()>;
}

/// Predict targets (or labels) from features.
pub trait Predictor {
    fn predict(&self, x: &NdArray) -> LearnResult<NdArray>;

    /// Class probabilities when applicable; default returns an error.
    fn predict_proba(&self, x: &NdArray) -> LearnResult<NdArray> {
        let _ = x;
        Err(crate::error::LearnError::Error(
            "predict_proba not implemented for this estimator".into(),
        ))
    }
}

/// Transform features (scalers, encoders, PCA, …).
pub trait Transformer {
    fn transform(&self, x: &NdArray) -> LearnResult<NdArray>;

    fn fit_transform(&mut self, x: &NdArray, y: Option<&NdArray>) -> LearnResult<NdArray>
    where
        Self: Estimator,
    {
        self.fit(x, y)?;
        self.transform(x)
    }
}

/// Score predictions against ground truth (delegates to metrics helpers).
pub trait Scorer {
    fn score(&self, x: &NdArray, y: &NdArray) -> LearnResult<f64>;
}
