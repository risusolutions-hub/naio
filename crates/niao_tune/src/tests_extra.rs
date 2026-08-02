//! Additional integration tests for search edge cases.

use crate::{
    grid_size, kfold_indices, run_random, sample_random, train_test_split_indices, ParamValue,
    SearchOpts, SpaceDim, TuneError,
};
use std::collections::BTreeMap;

#[test]
fn grid_size_empty_dimension_errors() {
    let mut s = BTreeMap::new();
    s.insert("x".into(), SpaceDim::Grid(vec![]));
    assert!(matches!(grid_size(&s), Err(TuneError::InvalidSpace(_))));
}

#[test]
fn random_zero_trials() {
    let mut s = BTreeMap::new();
    s.insert(
        "x".into(),
        SpaceDim::Float {
            low: 0.0,
            high: 1.0,
            log: false,
        },
    );
    assert!(sample_random(&s, 0, 0).unwrap().is_empty());
}

#[test]
fn invalid_test_size() {
    assert!(matches!(
        train_test_split_indices(10, 0.0, 0),
        Err(TuneError::InvalidSplit(_))
    ));
    assert!(matches!(
        train_test_split_indices(10, 1.0, 0),
        Err(TuneError::InvalidSplit(_))
    ));
}

#[test]
fn kfold_invalid_splits() {
    assert!(matches!(
        kfold_indices(5, 1, false, 0),
        Err(TuneError::InvalidSplit(_))
    ));
    assert!(matches!(
        kfold_indices(5, 10, false, 0),
        Err(TuneError::InvalidSplit(_))
    ));
}

#[test]
fn random_search_requires_trials() {
    let mut s = BTreeMap::new();
    s.insert("x".into(), SpaceDim::Grid(vec![ParamValue::Int(1)]));
    let r = run_random(&s, 0, |_| Ok(0.0), &SearchOpts::default());
    assert!(matches!(r, Err(TuneError::InvalidConfig(_))));
}

#[test]
fn log_float_sampling_in_range() {
    let mut s = BTreeMap::new();
    s.insert(
        "lr".into(),
        SpaceDim::Float {
            low: 0.001,
            high: 1.0,
            log: true,
        },
    );
    let samples = sample_random(&s, 50, 7).unwrap();
    for pt in samples {
        let v = pt["lr"].as_f64().unwrap();
        assert!(v >= 0.001 && v <= 1.0);
    }
}
