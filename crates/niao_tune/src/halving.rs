//! Successive halving over a resource budget (nlearn / neval steps).

use crate::error::{TuneError, TuneResult};
use crate::search::{pick_best, SearchDirection, TrialRecord, TrialStatus};
use crate::space::{sample_random, ParamValue, SearchSpace};
use std::collections::BTreeMap;

/// Configuration for successive halving (Hyperband-style bracket).
#[derive(Clone, Debug)]
pub struct HalvingConfig {
    pub n_trials: usize,
    pub min_resource: u64,
    pub max_resource: u64,
    pub reduction_factor: u64,
    pub direction: SearchDirection,
    pub seed: u64,
}

impl Default for HalvingConfig {
    fn default() -> Self {
        Self {
            n_trials: 27,
            min_resource: 1,
            max_resource: 81,
            reduction_factor: 3,
            direction: SearchDirection::Minimize,
            seed: 0,
        }
    }
}

impl HalvingConfig {
    pub fn validate(&self) -> TuneResult<()> {
        if self.n_trials == 0 {
            return Err(TuneError::InvalidConfig("n_trials must be > 0".into()));
        }
        if self.min_resource == 0 {
            return Err(TuneError::InvalidConfig("min_resource must be > 0".into()));
        }
        if self.max_resource < self.min_resource {
            return Err(TuneError::InvalidConfig(
                "max_resource must be >= min_resource".into(),
            ));
        }
        if self.reduction_factor < 2 {
            return Err(TuneError::InvalidConfig(
                "reduction_factor must be >= 2".into(),
            ));
        }
        Ok(())
    }
}

/// Run successive halving: allocate increasing budget only to top performers.
pub fn run_halving<F>(
    space: &SearchSpace,
    config: &HalvingConfig,
    mut objective: F,
) -> TuneResult<crate::search::SearchResult>
where
    F: FnMut(&BTreeMap<String, ParamValue>, u64) -> Result<f64, TuneError>,
{
    config.validate()?;

    let mut candidates = sample_random(space, config.n_trials, config.seed)?;
    let mut budget = config.min_resource;
    let eta = config.reduction_factor;

    let mut all_trials: Vec<TrialRecord> = Vec::new();
    let mut trial_id = 0usize;

    while candidates.len() > 1 && budget <= config.max_resource {
        let mut scored: Vec<(BTreeMap<String, ParamValue>, f64)> =
            Vec::with_capacity(candidates.len());

        for params in candidates.drain(..) {
            let value = objective(&params, budget)?;
            if !value.is_finite() {
                return Err(TuneError::InvalidConfig(format!(
                    "objective returned non-finite value at trial {trial_id}"
                )));
            }
            all_trials.push(TrialRecord {
                trial: trial_id,
                params: params.clone(),
                value,
                budget: Some(budget),
                status: TrialStatus::Complete,
            });
            trial_id += 1;
            scored.push((params, value));
        }

        scored.sort_by(|a, b| {
            let ord = a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal);
            if config.direction == SearchDirection::Minimize {
                ord
            } else {
                ord.reverse()
            }
        });

        let keep = (scored.len() / eta as usize).max(1);
        if budget >= config.max_resource {
            candidates = scored.into_iter().take(keep).map(|(p, _)| p).collect();
            break;
        }

        candidates = scored.into_iter().take(keep).map(|(p, _)| p).collect();
        budget = (budget * eta).min(config.max_resource);
    }

    // Final evaluation at max budget for remaining candidates.
    if !candidates.is_empty() {
        let final_budget = config.max_resource;
        for params in candidates {
            let value = objective(&params, final_budget)?;
            if !value.is_finite() {
                return Err(TuneError::InvalidConfig(format!(
                    "objective returned non-finite value at trial {trial_id}"
                )));
            }
            all_trials.push(TrialRecord {
                trial: trial_id,
                params,
                value,
                budget: Some(final_budget),
                status: TrialStatus::Complete,
            });
            trial_id += 1;
        }
    }

    let best = all_trials
        .iter()
        .filter(|t| t.budget == Some(config.max_resource))
        .fold(None, |acc: Option<TrialRecord>, t| {
            pick_best(acc, t, config.direction)
        })
        .or_else(|| {
            all_trials.iter().fold(None, |acc: Option<TrialRecord>, t| {
                pick_best(acc, t, config.direction)
            })
        });

    Ok(crate::search::SearchResult {
        trials: all_trials,
        best,
        direction: config.direction,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::SpaceDim;

    #[test]
    fn halving_reduces_trials() {
        let mut space = BTreeMap::new();
        space.insert(
            "x".into(),
            SpaceDim::Float {
                low: 0.0,
                high: 1.0,
                log: false,
            },
        );
        let cfg = HalvingConfig {
            n_trials: 9,
            min_resource: 1,
            max_resource: 9,
            reduction_factor: 3,
            ..Default::default()
        };
        let mut eval_count = 0usize;
        let result = run_halving(&space, &cfg, |params, _budget| {
            eval_count += 1;
            let x = match params.get("x").unwrap() {
                ParamValue::Float(f) => *f,
                _ => 0.0,
            };
            Ok(x)
        })
        .unwrap();
        assert!(eval_count >= 9);
        assert!(result.best.is_some());
    }
}
