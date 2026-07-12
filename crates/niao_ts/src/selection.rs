//! Model selection: auto_arima grid search, backtesting.

use crate::arima::{ArimaModel, ArimaOrder};
use crate::error::{TsError, TsResult};

#[derive(Debug, Clone)]
pub struct AutoArimaResult {
    pub order: ArimaOrder,
    pub aicc: f64,
    pub model: ArimaModel,
}

/// Small grid search over (p,d,q) by AICc.
pub fn auto_arima(
    y: &[f64],
    max_p: usize,
    max_d: usize,
    max_q: usize,
    seasonal_period: usize,
) -> TsResult<AutoArimaResult> {
    if y.len() < 10 {
        return Err(TsError::Domain("auto_arima: series too short".into()));
    }
    let mut best_aicc = f64::INFINITY;
    let mut best: Option<AutoArimaResult> = None;

    for d in 0..=max_d {
        for p in 0..=max_p {
            for q in 0..=max_q {
                if p == 0 && q == 0 && d == 0 {
                    continue;
                }
                let order = if seasonal_period > 1 {
                    ArimaOrder::sarima(p, d, q, 0, 0, 0, seasonal_period)
                } else {
                    ArimaOrder::arima(p, d, q)
                };
                let mut m = ArimaModel::new(order);
                if m.fit(y).is_err() {
                    continue;
                }
                let fit = m.fit_result()?;
                if fit.aicc < best_aicc {
                    best_aicc = fit.aicc;
                    best = Some(AutoArimaResult {
                        order,
                        aicc: fit.aicc,
                        model: m,
                    });
                }
            }
        }
    }
    best.ok_or_else(|| TsError::NonConvergence("auto_arima: no model converged".into()))
}

#[derive(Debug, Clone)]
pub struct BacktestResult {
    pub mae: f64,
    pub rmse: f64,
    pub mape: f64,
    pub forecasts: Vec<f64>,
    pub actuals: Vec<f64>,
}

/// Rolling-origin backtest with ARIMA refit each window (simplified: fixed order).
pub fn backtest(
    y: &[f64],
    order: ArimaOrder,
    train_size: usize,
    horizon: usize,
) -> TsResult<BacktestResult> {
    if train_size + horizon > y.len() {
        return Err(TsError::Domain("backtest: insufficient data".into()));
    }
    let mut forecasts = Vec::new();
    let mut actuals = Vec::new();
    let mut start = train_size;
    while start + horizon <= y.len() {
        let train = &y[..start];
        let mut m = ArimaModel::new(order);
        match m.fit(train) {
            Ok(_) => {
                if let Ok(fc) = m.forecast(horizon, 0.05) {
                    for i in 0..horizon {
                        forecasts.push(fc.mean[i]);
                        actuals.push(y[start + i]);
                    }
                }
            }
            Err(_) => {}
        }
        start += horizon;
    }
    if forecasts.is_empty() {
        return Err(TsError::Error("backtest: no windows completed".into()));
    }
    let n = forecasts.len() as f64;
    let mae: f64 = forecasts
        .iter()
        .zip(&actuals)
        .map(|(&f, &a)| (a - f).abs())
        .sum::<f64>()
        / n;
    let rmse: f64 = (forecasts
        .iter()
        .zip(&actuals)
        .map(|(&f, &a)| (a - f).powi(2))
        .sum::<f64>()
        / n)
        .sqrt();
    let mape: f64 = forecasts
        .iter()
        .zip(&actuals)
        .filter(|(_, &a)| a.abs() > 1e-12)
        .map(|(&f, &a)| ((a - f) / a).abs())
        .sum::<f64>()
        / n
        * 100.0;
    Ok(BacktestResult {
        mae,
        rmse,
        mape,
        forecasts,
        actuals,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::close;

    fn ar1(n: usize, phi: f64) -> Vec<f64> {
        let mut y = vec![0.0; n];
        y[0] = 1.0;
        for t in 1..n {
            y[t] = phi * y[t - 1] + 0.05 * (t as f64).sin();
        }
        y
    }

    #[test]
    fn auto_arima_picks_ar1() {
        let y = ar1(150, 0.7);
        let res = auto_arima(&y, 2, 1, 2, 0).unwrap();
        assert!(res.order.p >= 1 || res.order.q >= 1);
        assert!(res.aicc.is_finite());
    }

    #[test]
    fn backtest_runs() {
        let y = ar1(100, 0.5);
        let bt = backtest(&y, ArimaOrder::arima(1, 0, 0), 40, 5).unwrap();
        assert!(!bt.forecasts.is_empty());
        assert!(bt.rmse.is_finite());
    }
}
