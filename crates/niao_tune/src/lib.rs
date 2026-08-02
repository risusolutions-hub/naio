//! Hyperparameter search: grid, random, successive halving, and CV splits.

mod error;
mod halving;
mod search;
mod space;
mod split;

#[cfg(test)]
mod tests_extra;

pub use error::{TuneError, TuneResult};
pub use halving::{run_halving, HalvingConfig};
pub use search::{
    best_trial, pick_best, run_grid, run_random, SearchDirection, SearchOpts, SearchResult,
    TrialRecord, TrialStatus,
};
pub use space::{
    grid_cartesian, grid_size, sample_random, validate_space, ParamValue, SearchSpace, SpaceDim,
};
pub use split::{kfold_indices, train_test_split_indices, FoldSplit, IndexSplit};
