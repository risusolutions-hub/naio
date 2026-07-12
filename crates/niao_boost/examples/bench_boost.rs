//! Release benchmark harness for nboost (invoked by benchmarks/benchmark_nboost.py).

use niao_boost::fixtures;
use niao_boost::{BoosterParams, GBRegressor, GrowPolicy};
use std::time::Instant;

fn main() {
    let (x, y) = fixtures::synthetic_regression(42, 10_000, 50);
    let params = BoosterParams {
        n_estimators: 100,
        max_depth: 6,
        max_leaves: 31,
        max_bins: 256,
        learning_rate: 0.1,
        min_data_in_leaf: 20,
        grow_policy: GrowPolicy::LeafWise,
        seed: 42,
        ..BoosterParams::default()
    };
    let mut model = GBRegressor::new(params).expect("params");
    let t0 = Instant::now();
    model.fit(&x, 10_000, 50, &y).expect("fit");
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!("{ms:.3}");
}
