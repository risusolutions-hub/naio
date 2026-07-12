//! Hyperparameters for histogram GBDT.

use crate::error::{BoostError, BoostResult};

/// Tree growth strategy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrowPolicy {
    /// Expand the leaf with the largest split gain (LightGBM-style).
    LeafWise,
    /// Level-by-level expansion (depth-wise).
    DepthWise,
}

/// Gradient boosting hyperparameters.
#[derive(Clone, Debug)]
pub struct BoosterParams {
    pub learning_rate: f64,
    pub n_estimators: usize,
    pub max_depth: usize,
    pub max_leaves: usize,
    pub max_bins: usize,
    pub lambda_l2: f64,
    pub alpha_l1: f64,
    pub gamma: f64,
    pub min_child_weight: f64,
    pub min_data_in_leaf: usize,
    pub subsample: f64,
    pub colsample: f64,
    pub grow_policy: GrowPolicy,
    pub seed: u64,
    pub early_stopping_rounds: Option<usize>,
}

impl Default for BoosterParams {
    fn default() -> Self {
        Self {
            learning_rate: 0.1,
            n_estimators: 100,
            max_depth: 6,
            max_leaves: 31,
            max_bins: 256,
            lambda_l2: 1.0,
            alpha_l1: 0.0,
            gamma: 0.0,
            min_child_weight: 1e-3,
            min_data_in_leaf: 20,
            subsample: 1.0,
            colsample: 1.0,
            grow_policy: GrowPolicy::LeafWise,
            seed: 42,
            early_stopping_rounds: None,
        }
    }
}

impl BoosterParams {
    pub fn validate(&self) -> BoostResult<()> {
        if self.max_bins < 2 {
            return Err(BoostError::BadParam("max_bins must be >= 2".into()));
        }
        if self.learning_rate <= 0.0 {
            return Err(BoostError::BadParam("learning_rate must be > 0".into()));
        }
        if !(0.0 < self.subsample && self.subsample <= 1.0) {
            return Err(BoostError::BadParam("subsample must be in (0, 1]".into()));
        }
        if !(0.0 < self.colsample && self.colsample <= 1.0) {
            return Err(BoostError::BadParam("colsample must be in (0, 1]".into()));
        }
        if self.max_depth == 0 || self.max_leaves == 0 {
            return Err(BoostError::BadParam("max_depth and max_leaves must be >= 1".into()));
        }
        Ok(())
    }
}
