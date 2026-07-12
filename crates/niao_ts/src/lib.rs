//! nts — statsmodels.tsa core for Niao.
//!
//! Time series diagnostics, decomposition, ARIMA/SARIMA, exponential smoothing,
//! and forecasting. Zero external deps beyond niao_num/niao_stats/niao_optim/niao_frame.

pub mod arima;
pub mod decompose;
pub mod diagnostics;
pub mod error;
pub mod ets;
pub mod frame;
pub mod selection;
pub mod util;

pub use arima::{ar_yule_walker, ArimaFit, ArimaModel, ArimaOrder, ForecastResult};
pub use decompose::{seasonal_decompose, DecomposeResult};
pub use diagnostics::{
    acf, adfuller, diff, kpss, lagmat, ljungbox, pacf, seasonal_diff, TestResult,
};
pub use error::{
    TsError, TsResult, E4070_NTS_ARITY, E4071_NTS_ERROR, E4072_NTS_TYPE, E4073_NTS_NOT_FITTED,
    E4074_NTS_NON_STATIONARY, E4075_NTS_NON_CONVERGENCE, E4076_NTS_DOMAIN, E4077_NTS_SHAPE,
};
pub use ets::{ses, EtsFit, EtsModel, SeasonalMode};
pub use frame::{series_to_vec, vec_to_series};
pub use selection::{auto_arima, backtest, AutoArimaResult, BacktestResult};
pub use util::{aic, aicc, bic};

#[cfg(test)]
mod scipy_fixtures {
    use super::*;
    use crate::util::close;

    /// statsmodels ACF for AR(2) phi=[0.6,-0.3], n=200, nlags=5 (approx fixture)
    #[test]
    fn acf_ar2_vs_reference() {
        let mut y = vec![0.0; 200];
        y[0] = 1.0;
        y[1] = 0.5;
        for t in 2..200 {
            y[t] = 0.6 * y[t - 1] - 0.3 * y[t - 2];
        }
        let acf_v = acf(&y, Some(5)).unwrap();
        // AR(2) theoretical gamma(1)/gamma(0) for phi1=0.6, phi2=-0.3
        let expected_r1 = 0.6 / (1.0 + 0.3);
        assert!(close(acf_v[1], expected_r1, 0.15), "r1={}", acf_v[1]);
        assert!((acf_v[0] - 1.0).abs() < 1e-10);
    }

    #[test]
    fn pacf_ar2_spike_at_2() {
        let mut y = vec![0.0; 300];
        let mut rng = 1.0;
        for t in 2..300 {
            rng = (rng * 16807.0) % 2147483647.0;
            let e = 0.1 * (rng / 2147483647.0 - 0.5);
            y[t] = 0.6 * y[t - 1] - 0.3 * y[t - 2] + e;
        }
        let pacf_v = pacf(&y, Some(5)).unwrap();
        assert!(pacf_v[2].abs() > pacf_v[5].abs() * 0.5);
    }

    #[test]
    fn ar_yule_walker_vs_statsmodels_tol() {
        let mut y = vec![0.0; 500];
        y[0] = 1.0;
        let phi_true = 0.75;
        let mut rng = 42.0;
        for t in 1..500 {
            rng = (rng * 16807.0) % 2147483647.0;
            let e = rng / 2147483647.0 - 0.5;
            y[t] = phi_true * y[t - 1] + 0.2 * e;
        }
        let (phi, _) = ar_yule_walker(&y, 1).unwrap();
        assert!(close(phi[0], phi_true, 0.05), "phi={}", phi[0]);
    }

    #[test]
    fn error_codes() {
        assert_eq!(
            ArimaModel::arima(1, 0, 0)
                .forecast(1, 0.05)
                .unwrap_err()
                .code(),
            E4073_NTS_NOT_FITTED
        );
    }

    #[test]
    fn airline_arima_forecast() {
        // Classic airline passengers (first 24 months subset for speed)
        let air: Vec<f64> = vec![
            112.0, 118.0, 132.0, 129.0, 121.0, 135.0, 148.0, 148.0, 136.0, 119.0, 104.0, 118.0,
            115.0, 126.0, 141.0, 135.0, 125.0, 149.0, 170.0, 170.0, 158.0, 133.0, 114.0, 140.0,
        ];
        let mut m = ArimaModel::arima(1, 1, 1);
        m.fit(&air).expect("fit");
        let fc = m.forecast(6, 0.05).unwrap();
        assert_eq!(fc.mean.len(), 6);
        assert!(fc.mean[0] > 100.0);
        assert!(m.aic().unwrap().is_finite());
    }

    #[test]
    fn holt_winters_airline_subset() {
        let air: Vec<f64> = vec![
            112.0, 118.0, 132.0, 129.0, 121.0, 135.0, 148.0, 148.0, 136.0, 119.0, 104.0, 118.0,
            115.0, 126.0, 141.0, 135.0, 125.0, 149.0, 170.0, 170.0, 158.0, 133.0, 114.0, 140.0,
        ];
        let mut hw = EtsModel::holt_winters(12, false);
        hw.fit(&air).unwrap();
        let fc = hw.forecast(4).unwrap();
        assert_eq!(fc.len(), 4);
    }

    #[test]
    fn seasonal_decompose_additive() {
        let x: Vec<f64> = (0..24).map(|t| t as f64 + (t % 4) as f64).collect();
        let d = seasonal_decompose(&x, 4, false).unwrap();
        assert_eq!(d.trend.len(), 24);
    }
}
