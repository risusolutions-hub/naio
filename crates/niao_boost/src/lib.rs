//! nboost — histogram gradient-boosted decision trees for Niao (XGBoost / LightGBM subset).
//!
//! Error block: 4060–4069.

pub mod binning;
pub mod booster;
pub mod classifier;
pub mod error;
pub mod fixture_loader;
pub mod fixtures;
pub mod histogram;
pub mod objective;
pub mod params;
pub mod regressor;
pub mod serialize;
pub mod tree;

pub use binning::{value_to_bin, BinnedMatrix, MISSING_BIN};
pub use booster::{
    auc_binary, logloss_binary, logloss_multiclass, rmse, Booster, Dataset, ImportanceKind,
};
pub use classifier::{accuracy, GBClassifier};
pub use error::{
    BoostError, BoostResult, E4060_NBOOST_ARITY, E4061_NBOOST_ERROR, E4062_NBOOST_TYPE,
    E4063_NBOOST_NOT_FITTED, E4064_NBOOST_BAD_PARAM, E4065_NBOOST_SHAPE, E4066_NBOOST_IO,
    E4067_NBOOST_NON_CONVERGENCE,
};
pub use objective::{Logistic, Objective, SoftmaxMulticlass, SquaredError, TaskKind};
pub use params::{BoosterParams, GrowPolicy};
pub use regressor::{r2_score, GBRegressor};
pub use serialize::{load_model, model_from_json, model_to_json, save_model};
pub use tree::{build_tree, Tree, TreeNode};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::histogram::{best_split_on_histogram, build_histogram, exact_best_split, split_gain, FeatureHistogram};
    use crate::params::GrowPolicy;

    fn assert_close(a: f64, b: f64, rtol: f64) {
        let diff = (a - b).abs();
        let tol = rtol * b.abs().max(1.0);
        assert!(diff <= tol, "assert_close: {a} vs {b} (diff={diff}, tol={tol})");
    }

    fn assert_within_pct(a: f64, b: f64, pct: f64) {
        let diff = (a - b).abs();
        let tol = pct * b.abs().max(1e-9);
        assert!(diff <= tol, "within {pct}: {a} vs {b} (diff={diff}, tol={tol})");
    }

    fn reg_params() -> BoosterParams {
        BoosterParams {
            learning_rate: 0.1,
            n_estimators: 50,
            max_depth: 4,
            max_leaves: 16,
            max_bins: 256,
            lambda_l2: 0.0,
            min_data_in_leaf: 5,
            min_child_weight: 1e-3,
            subsample: 1.0,
            colsample: 1.0,
            grow_policy: GrowPolicy::DepthWise,
            seed: 42,
            early_stopping_rounds: None,
            ..BoosterParams::default()
        }
    }

    fn load_sklearn_fixtures() -> fixture_loader::SklearnFixtures {
        fixture_loader::load_sklearn_fixtures().expect("sklearn fixtures")
    }

    #[test]
    fn error_not_fitted() {
        let model = GBRegressor::new(BoosterParams::default()).unwrap();
        let err = model
            .predict(&[1.0, 2.0], 1, 2)
            .unwrap_err();
        assert_eq!(err.code(), E4063_NBOOST_NOT_FITTED);
    }

    #[test]
    fn error_bad_param() {
        let mut p = BoosterParams::default();
        p.max_bins = 1;
        let err = GBRegressor::new(p).unwrap_err();
        assert_eq!(err.code(), E4064_NBOOST_BAD_PARAM);
        p = BoosterParams::default();
        p.learning_rate = 0.0;
        let err = GBRegressor::new(p).unwrap_err();
        assert_eq!(err.code(), E4064_NBOOST_BAD_PARAM);
    }

    #[test]
    fn error_shape_mismatch() {
        let mut m = GBRegressor::new(reg_params()).unwrap();
        let err = m.fit(&[1.0, 2.0], 2, 1, &[1.0]).unwrap_err();
        assert_eq!(err.code(), E4065_NBOOST_SHAPE);
    }

    #[test]
    fn histogram_gain_vs_exact_split() {
        let x = vec![0.0, 1.0, 2.0, 3.0];
        let y = vec![0.0, 1.0, 2.0, 1.5];
        let n = 4;
        let p = 1;
        let params = BoosterParams {
            max_bins: 4,
            min_data_in_leaf: 1,
            min_child_weight: 1e-6,
            lambda_l2: 0.0,
            gamma: 0.0,
            ..BoosterParams::default()
        };
        let bm = BinnedMatrix::from_matrix(&x, n, p, params.max_bins).unwrap();
        let mut preds = vec![0.0; n];
        SquaredError.init_predictions(&y, n, &mut preds).unwrap();
        let mut grad = vec![0.0; n];
        let mut hess = vec![0.0; n];
        SquaredError
            .gradients(&y, &preds, &mut grad, &mut hess)
            .unwrap();
        let rows: Vec<usize> = (0..n).collect();
        let col = x.clone();
        let exact = exact_best_split(&col, &rows, &grad, &hess, &params, 0);
        let mut hist = FeatureHistogram::new(bm.thresholds[0].len().max(1));
        build_histogram(&bm, 0, &rows, &grad, &hess, &mut hist);
        let hist_split = best_split_on_histogram(&hist, &params, 0);
        match (exact, hist_split) {
            (Some(e), Some(h)) => {
                assert_close(e.gain, h.gain, 1e-9);
            }
            (None, None) => {}
            (e, h) => panic!("exact={e:?} hist={h:?}"),
        }
    }

    #[test]
    fn split_gain_formula() {
        let g = split_gain(2.0, 4.0, -2.0, 4.0, 1.0, 0.0);
        let expected = 0.5 * (4.0 / 5.0 + 4.0 / 5.0 - 0.0 / 9.0);
        assert_close(g, expected, 1e-12);
    }

    #[test]
    fn regression_vs_sklearn_fixture() {
        let fx = load_sklearn_fixtures();
        let mut model = GBRegressor::new(reg_params()).unwrap();
        model.fit(&fx.reg_x, 200, 5, &fx.reg_y).unwrap();
        let preds = model.predict(&fx.reg_x, 200, 5).unwrap();
        let train_rmse = rmse(&preds, &fx.reg_y);
        assert_within_pct(train_rmse, fx.sk_rmse, 0.10);
    }

    #[test]
    fn binary_vs_sklearn_fixture() {
        let fx = load_sklearn_fixtures();
        let params = BoosterParams {
            n_estimators: 40,
            max_depth: 3,
            max_leaves: 8,
            min_data_in_leaf: 5,
            learning_rate: 0.1,
            max_bins: 256,
            lambda_l2: 0.0,
            grow_policy: GrowPolicy::DepthWise,
            seed: 42,
            ..BoosterParams::default()
        };
        let mut model = GBClassifier::new_binary(params).unwrap();
        model.fit(&fx.bin_x, 300, 8, &fx.bin_y).unwrap();
        let logits = model.predict_logits(&fx.bin_x, 300, 8).unwrap();
        let auc = auc_binary(&logits, &fx.bin_y);
        let ll = logloss_binary(&logits, &fx.bin_y);
        assert_within_pct(auc, fx.sk_auc, 0.02);
        assert_within_pct(ll, fx.sk_logloss, 0.04);
        let proba = model.predict_proba(&fx.bin_x, 300, 8).unwrap();
        assert!(proba.iter().all(|&p| p >= 0.0 && p <= 1.0));
    }

    #[test]
    fn iris_multiclass_fixture() {
        let (x, y) = fixtures::iris_full();
        let params = BoosterParams {
            n_estimators: 30,
            max_depth: 3,
            max_leaves: 8,
            learning_rate: 0.1,
            min_data_in_leaf: 1,
            max_bins: 32,
            seed: 42,
            ..BoosterParams::default()
        };
        let mut model = GBClassifier::new_multiclass(params, 3).unwrap();
        model.fit(&x, 150, 4, &y).unwrap();
        let pred = model.predict(&x, 150, 4).unwrap();
        let acc = accuracy(&pred, &y);
        assert!(acc >= 0.95);
    }

    #[test]
    fn early_stopping_halts() {
        let (x, y) = fixtures::synthetic_regression(42, 200, 5);
        let n_tr = 150;
        let (xtr, ytr) = (&x[..n_tr * 5], &y[..n_tr]);
        let (xva, yva) = (&x[n_tr * 5..], &y[n_tr..]);
        let params = BoosterParams {
            n_estimators: 100,
            early_stopping_rounds: Some(10),
            ..reg_params()
        };
        let mut model = GBRegressor::new(params).unwrap();
        model
            .fit_with_eval(xtr, n_tr, 5, ytr, Some((xva, 50, 5, yva)))
            .unwrap();
        assert!(model.booster.best_iteration <= 99);
        assert!(model.booster.trees.len() <= 100);
    }

    #[test]
    fn missing_value_routing() {
        let mut x = fixtures::tiny_x();
        x[3] = f64::NAN; // row 1, feature 0
        let y = fixtures::tiny_y();
        let params = BoosterParams {
            n_estimators: 10,
            min_data_in_leaf: 1,
            max_bins: 8,
            max_depth: 3,
            ..BoosterParams::default()
        };
        let mut model = GBRegressor::new(params).unwrap();
        model.fit(&x, 4, 2, &y).unwrap();
        let preds = model.predict(&x, 4, 2).unwrap();
        assert!(preds[1].is_finite());
    }

    #[test]
    fn save_load_predict_identical() {
        let (x, y) = fixtures::synthetic_regression(42, 80, 3);
        let mut model = GBRegressor::new(reg_params()).unwrap();
        model.fit(&x, 80, 3, &y).unwrap();
        let before = model.predict(&x, 80, 3).unwrap();
        let path = std::env::temp_dir().join("niao_boost_test_model.json");
        save_model(&model.booster, path.to_str().unwrap()).unwrap();
        let loaded = load_model(path.to_str().unwrap()).unwrap();
        let data = Dataset::from_matrix(&x, 80, 3, model.params.max_bins).unwrap();
        let after = loaded.predict(&data).unwrap();
        for (a, b) in before.iter().zip(&after) {
            assert_close(*a, *b, 1e-12);
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn feature_importance_sums() {
        let (x, y) = fixtures::synthetic_regression(42, 100, 4);
        let mut model = GBRegressor::new(reg_params()).unwrap();
        model.fit(&x, 100, 4, &y).unwrap();
        let imp = model.feature_importance(ImportanceKind::Gain).unwrap();
        let s: f64 = imp.iter().sum();
        assert!((s - 1.0).abs() < 1e-9 || s == 0.0);
    }

    #[test]
    fn histogram_subtraction_child_equals_parent_minus_sibling() {
        let x = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        let bm = BinnedMatrix::from_matrix(&x, 4, 2, 8).unwrap();
        let grad = vec![1.0, -1.0, 0.5, -0.5, 0.2, -0.2, 0.3, -0.3];
        let hess = vec![1.0; 8];
        let parent_rows = vec![0usize, 1, 2, 3];
        let left = vec![0usize, 1];
        let right = vec![2usize, 3];
        let mut parent_h = FeatureHistogram::new(8);
        let mut left_h = FeatureHistogram::new(8);
        let mut right_h = FeatureHistogram::new(8);
        build_histogram(&bm, 0, &parent_rows, &grad, &hess, &mut parent_h);
        build_histogram(&bm, 0, &left, &grad, &hess, &mut left_h);
        build_histogram(&bm, 0, &right, &grad, &hess, &mut right_h);
        let mut derived = parent_h.clone();
        left_h.subtract_from(&mut derived);
        for i in 0..=8 {
            assert_close(derived.grad[i], right_h.grad[i], 1e-12);
            assert_close(derived.hess[i], right_h.hess[i], 1e-12);
            assert_eq!(derived.count[i], right_h.count[i]);
        }
    }
}
