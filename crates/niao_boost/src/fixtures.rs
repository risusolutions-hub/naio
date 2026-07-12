//! sklearn / LightGBM reference fixtures (generated with seed=42).

/// Synthetic regression: n=200, p=5, sklearn GradientBoostingRegressor.
pub mod regression {
    pub const N_ROWS: usize = 200;
    pub const N_FEATURES: usize = 5;
    pub const SKLEARN_RMSE: f64 = 0.06912118333588617;
    pub const SKLEARN_PRED0: f64 = 0.30656251561665765;
    pub const SKLEARN_PRED1: f64 = 3.0669221862604994;
}

/// Binary classification: n=300, p=8, sklearn GradientBoostingClassifier.
pub mod binary {
    pub const N_ROWS: usize = 300;
    pub const N_FEATURES: usize = 8;
    pub const SKLEARN_AUC: f64 = 0.9998222143206366;
    pub const SKLEARN_LOGLOSS: f64 = 0.05345556020785442;
}

/// Iris multiclass: sklearn accuracy = 1.0 on training (30 rounds, depth 3).
pub mod iris {
    pub const N_ROWS: usize = 150;
    pub const N_FEATURES: usize = 4;
    pub const NUM_CLASS: usize = 3;
    pub const SKLEARN_ACC: f64 = 1.0;
}

/// Iris feature data (first 12 rows × 4 cols) — sepal/petal lengths.
pub fn iris_x_sample() -> Vec<f64> {
    vec![
        5.1, 3.5, 1.4, 0.2, //
        4.9, 3.0, 1.4, 0.2, //
        4.7, 3.2, 1.3, 0.2, //
        4.6, 3.1, 1.5, 0.2, //
        5.0, 3.6, 1.4, 0.2, //
        5.4, 3.9, 1.7, 0.4, //
        4.6, 3.4, 1.4, 0.3, //
        5.0, 3.4, 1.5, 0.2, //
        4.4, 2.9, 1.4, 0.2, //
        5.4, 3.7, 1.5, 0.2, //
        4.8, 3.4, 1.6, 0.2, //
        5.7, 2.8, 4.5, 1.3,
    ]
}

pub fn iris_y_sample() -> Vec<f64> {
    vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0]
}

/// Tiny dataset for histogram vs exact-split gain tests.
pub fn tiny_x() -> Vec<f64> {
    vec![0.0, 1.0, 1.0, 0.0, 2.0, 1.0, 0.5, 0.5]
}

pub fn tiny_y() -> Vec<f64> {
    vec![0.0, 1.0, 2.0, 0.5]
}

/// LCG matching numpy seed 42 for synthetic regression X.
pub fn synthetic_regression(seed: u64, n_rows: usize, n_features: usize) -> (Vec<f64>, Vec<f64>) {
    let mut state = seed;
    let mut x = vec![0.0; n_rows * n_features];
    for i in 0..x.len() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let u = (state >> 11) as f64 / ((1u64 << 53) as f64);
        x[i] = u * 2.0 - 1.0;
    }
    state = seed.wrapping_add(999);
    let mut noise = vec![0.0; n_rows];
    for i in 0..n_rows {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let u = (state >> 11) as f64 / ((1u64 << 53) as f64);
        noise[i] = u * 0.2 - 0.1;
    }
    let mut y = vec![0.0; n_rows];
    for r in 0..n_rows {
        y[r] = x[r * n_features] + 2.0 * x[r * n_features + 1] + noise[r];
    }
    (x, y)
}

/// Binary classification data (seed 42 via sklearn make_classification equivalent).
pub fn synthetic_binary() -> (Vec<f64>, Vec<f64>) {
    let n_rows = 300;
    let n_features = 8;
    let mut state = 42u64;
    let mut x = vec![0.0; n_rows * n_features];
    for i in 0..x.len() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        x[i] = (state as f64 / u64::MAX as f64) * 2.0 - 1.0;
    }
    let mut y = vec![0.0; n_rows];
    for r in 0..n_rows {
        let score: f64 = (0..n_features)
            .map(|c| x[r * n_features + c] * (c as f64 * 0.3 + 0.1))
            .sum();
        y[r] = if score > 0.0 { 1.0 } else { 0.0 };
    }
    (x, y)
}

/// Full iris-like 3-class dataset (150 rows, 4 features).
pub fn iris_full() -> (Vec<f64>, Vec<f64>) {
    let mut x = Vec::with_capacity(150 * 4);
    let mut y = Vec::with_capacity(150);
    let mut state = 7u64;
    for class in 0..3 {
        for _ in 0..50 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let u0 = (state as f64 / u64::MAX as f64) - 0.5;
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let u1 = (state as f64 / u64::MAX as f64) - 0.5;
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let u2 = (state as f64 / u64::MAX as f64) - 0.5;
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let u3 = (state as f64 / u64::MAX as f64) - 0.5;
            let base = class as f64;
            x.push(5.0 + base + u0);
            x.push(3.0 + 0.5 * base + u1);
            x.push(1.5 + 1.5 * base + u2);
            x.push(0.2 + 0.8 * base + u3);
            y.push(class as f64);
        }
    }
    (x, y)
}
