//! Grid/random search orchestration and trial records.

use crate::error::{TuneError, TuneResult};
use crate::space::{grid_cartesian, sample_random, ParamValue, SearchSpace};
use std::collections::BTreeMap;

/// Optimization direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchDirection {
    Minimize,
    Maximize,
}

impl SearchDirection {
    pub fn from_str(s: &str) -> TuneResult<Self> {
        match s.to_ascii_lowercase().as_str() {
            "minimize" | "min" => Ok(Self::Minimize),
            "maximize" | "max" => Ok(Self::Maximize),
            other => Err(TuneError::InvalidConfig(format!(
                "unknown direction '{other}' (minimize|maximize)"
            ))),
        }
    }

    pub fn is_better(&self, a: f64, b: f64) -> bool {
        match self {
            Self::Minimize => a < b,
            Self::Maximize => a > b,
        }
    }
}

/// Common search options.
#[derive(Clone, Debug)]
pub struct SearchOpts {
    pub direction: SearchDirection,
    pub seed: u64,
}

impl Default for SearchOpts {
    fn default() -> Self {
        Self {
            direction: SearchDirection::Minimize,
            seed: 0,
        }
    }
}

/// One completed (or pruned) trial.
#[derive(Clone, Debug, PartialEq)]
pub struct TrialRecord {
    pub trial: usize,
    pub params: BTreeMap<String, ParamValue>,
    pub value: f64,
    pub budget: Option<u64>,
    pub status: TrialStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrialStatus {
    Complete,
    Pruned,
}

/// Aggregate search outcome.
#[derive(Clone, Debug)]
pub struct SearchResult {
    pub trials: Vec<TrialRecord>,
    pub best: Option<TrialRecord>,
    pub direction: SearchDirection,
}

/// Run grid search over explicit grid lists.
pub fn run_grid<F>(
    space: &SearchSpace,
    mut objective: F,
    opts: &SearchOpts,
) -> TuneResult<SearchResult>
where
    F: FnMut(&BTreeMap<String, ParamValue>) -> Result<f64, TuneError>,
{
    let combos = grid_cartesian(space)?;
    run_trials(combos, None, &mut objective, opts)
}

/// Run random search with `n_trials` independent samples.
pub fn run_random<F>(
    space: &SearchSpace,
    n_trials: usize,
    mut objective: F,
    opts: &SearchOpts,
) -> TuneResult<SearchResult>
where
    F: FnMut(&BTreeMap<String, ParamValue>) -> Result<f64, TuneError>,
{
    if n_trials == 0 {
        return Err(TuneError::InvalidConfig("n_trials must be > 0".into()));
    }
    let combos = sample_random(space, n_trials, opts.seed)?;
    run_trials(combos, None, &mut objective, opts)
}

fn run_trials<F>(
    combos: Vec<BTreeMap<String, ParamValue>>,
    budget: Option<u64>,
    objective: &mut F,
    opts: &SearchOpts,
) -> TuneResult<SearchResult>
where
    F: FnMut(&BTreeMap<String, ParamValue>) -> Result<f64, TuneError>,
{
    if combos.is_empty() {
        return Err(TuneError::NoTrials);
    }
    let mut trials = Vec::with_capacity(combos.len());
    let mut best: Option<TrialRecord> = None;

    for (i, params) in combos.into_iter().enumerate() {
        let value = objective(&params)?;
        if !value.is_finite() {
            return Err(TuneError::InvalidConfig(format!(
                "objective returned non-finite value at trial {i}"
            )));
        }
        let record = TrialRecord {
            trial: i,
            params,
            value,
            budget,
            status: TrialStatus::Complete,
        };
        best = pick_best(best, &record, opts.direction);
        trials.push(record);
    }

    Ok(SearchResult {
        trials,
        best,
        direction: opts.direction,
    })
}

pub fn pick_best(
    current: Option<TrialRecord>,
    candidate: &TrialRecord,
    direction: SearchDirection,
) -> Option<TrialRecord> {
    match current {
        None => Some(candidate.clone()),
        Some(prev) if direction.is_better(candidate.value, prev.value) => Some(candidate.clone()),
        Some(prev) => Some(prev),
    }
}

/// Select the best trial from a slice.
pub fn best_trial(trials: &[TrialRecord], direction: SearchDirection) -> Option<TrialRecord> {
    trials.iter().fold(None, |acc: Option<TrialRecord>, t| {
        pick_best(acc, t, direction)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::{ParamValue, SpaceDim};

    fn loss(params: &BTreeMap<String, ParamValue>) -> Result<f64, TuneError> {
        let lr = match params.get("lr").unwrap() {
            ParamValue::Float(f) => *f,
            _ => 0.0,
        };
        Ok(lr * lr)
    }

    #[test]
    fn grid_picks_minimum() {
        let mut s = BTreeMap::new();
        s.insert(
            "lr".into(),
            SpaceDim::Grid(vec![ParamValue::Float(0.2), ParamValue::Float(0.01)]),
        );
        let r = run_grid(&s, loss, &SearchOpts::default()).unwrap();
        let best = r.best.unwrap();
        assert!((best.value - 0.0001).abs() < 1e-9);
    }
}
