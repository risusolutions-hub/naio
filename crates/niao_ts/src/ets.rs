//! Exponential smoothing: SES, Holt, Holt–Winters.

use crate::error::{TsError, TsResult};
use crate::util::mean;
use niao_optim::{minimize, MinimizeMethod, MinimizeOptions};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeasonalMode {
    None,
    Additive,
    Multiplicative,
}

#[derive(Debug, Clone)]
pub struct EtsFit {
    pub alpha: f64,
    pub beta: f64,
    pub gamma: f64,
    pub level: f64,
    pub trend: f64,
    pub seasonal: Vec<f64>,
    pub fitted: Vec<f64>,
    pub residuals: Vec<f64>,
    pub sse: f64,
    pub period: usize,
    pub mode: SeasonalMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EtsKind {
    Ses,
    Holt,
    HoltWinters,
}

#[derive(Debug, Clone)]
pub struct EtsModel {
    kind: EtsKind,
    mode: SeasonalMode,
    period: usize,
    fit: Option<EtsFit>,
}

impl EtsModel {
    pub fn ses() -> Self {
        Self {
            kind: EtsKind::Ses,
            mode: SeasonalMode::None,
            period: 0,
            fit: None,
        }
    }

    pub fn holt() -> Self {
        Self {
            kind: EtsKind::Holt,
            mode: SeasonalMode::None,
            period: 0,
            fit: None,
        }
    }

    pub fn holt_winters(period: usize, multiplicative: bool) -> Self {
        Self {
            kind: EtsKind::HoltWinters,
            mode: if multiplicative {
                SeasonalMode::Multiplicative
            } else {
                SeasonalMode::Additive
            },
            period,
            fit: None,
        }
    }

    pub fn fit(&mut self, y: &[f64]) -> TsResult<&EtsFit> {
        let fit = match self.kind {
            EtsKind::Ses => fit_ses_or_holt(y, false)?,
            EtsKind::Holt => fit_ses_or_holt(y, true)?,
            EtsKind::HoltWinters => {
                fit_holt_winters(y, self.period, self.mode == SeasonalMode::Multiplicative)?
            }
        };
        self.fit = Some(fit);
        Ok(self.fit.as_ref().unwrap())
    }

    pub fn forecast(&self, h: usize) -> TsResult<Vec<f64>> {
        let fit = self
            .fit
            .as_ref()
            .ok_or_else(|| TsError::NotFitted("call fit() first".into()))?;
        forecast_ets(fit, h)
    }

    pub fn fitted_values(&self) -> TsResult<&[f64]> {
        self.fit
            .as_ref()
            .map(|f| f.fitted.as_slice())
            .ok_or_else(|| TsError::NotFitted("call fit() first".into()))
    }
}

fn fit_ses_or_holt(y: &[f64], with_trend: bool) -> TsResult<EtsFit> {
    let n = y.len();
    if n < 3 {
        return Err(TsError::Domain("ETS: need at least 3 points".into()));
    }
    let n_params = if with_trend { 2 } else { 1 };
    let yv = y.to_vec();

    let mut obj = |u: &[f64], _grad: &mut [f64]| -> f64 {
        let alpha = u[0].clamp(0.001, 0.999);
        let beta = if with_trend {
            u[1].clamp(0.001, 0.999)
        } else {
            0.0
        };
        let (fitted, _) = hw_smooth(&yv, alpha, beta, 0.0, 0, false);
        fitted.iter().zip(&yv).map(|(&f, &o)| (o - f).powi(2)).sum()
    };

    let u0 = if with_trend {
        vec![0.3, 0.1]
    } else {
        vec![0.3]
    };
    let lo: Vec<f64> = vec![0.0; n_params];
    let hi: Vec<f64> = vec![1.0; n_params];
    let res = minimize(
        &mut obj,
        &u0,
        MinimizeMethod::NelderMead,
        None::<fn(&[f64], &mut [f64]) -> ()>,
        MinimizeOptions {
            max_iter: 300,
            bounds_lo: Some(lo),
            bounds_hi: Some(hi),
            ..Default::default()
        },
    );
    if !res.success {
        return Err(TsError::NonConvergence(format!("ETS fit: {}", res.message)));
    }
    let alpha = res.x[0].clamp(0.001, 0.999);
    let beta = if with_trend {
        res.x[1].clamp(0.001, 0.999)
    } else {
        0.0
    };
    let (fitted, state) = hw_smooth(y, alpha, beta, 0.0, 0, false);
    let residuals: Vec<f64> = y.iter().zip(&fitted).map(|(&o, &f)| o - f).collect();
    let sse = residuals.iter().map(|r| r * r).sum();
    Ok(EtsFit {
        alpha,
        beta,
        gamma: 0.0,
        level: state.0,
        trend: state.1,
        seasonal: Vec::new(),
        fitted,
        residuals,
        sse,
        period: 0,
        mode: SeasonalMode::None,
    })
}

fn fit_holt_winters(y: &[f64], period: usize, mult: bool) -> TsResult<EtsFit> {
    let n = y.len();
    if period < 2 || n < 2 * period {
        return Err(TsError::Domain("Holt-Winters: need 2 full seasons".into()));
    }
    let yv = y.to_vec();
    let mut obj = |u: &[f64], _grad: &mut [f64]| -> f64 {
        let alpha = u[0].clamp(0.001, 0.999);
        let beta = u[1].clamp(0.001, 0.999);
        let gamma = u[2].clamp(0.001, 0.999);
        let (fitted, _) = hw_smooth(&yv, alpha, beta, gamma, period, mult);
        fitted.iter().zip(&yv).map(|(&f, &o)| (o - f).powi(2)).sum()
    };
    let res = minimize(
        &mut obj,
        &[0.2, 0.1, 0.1],
        MinimizeMethod::NelderMead,
        None::<fn(&[f64], &mut [f64]) -> ()>,
        MinimizeOptions {
            max_iter: 500,
            bounds_lo: Some(vec![0.0, 0.0, 0.0]),
            bounds_hi: Some(vec![1.0, 1.0, 1.0]),
            ..Default::default()
        },
    );
    if !res.success {
        return Err(TsError::NonConvergence(format!(
            "Holt-Winters: {}",
            res.message
        )));
    }
    let alpha = res.x[0].clamp(0.001, 0.999);
    let beta = res.x[1].clamp(0.001, 0.999);
    let gamma = res.x[2].clamp(0.001, 0.999);
    let (fitted, state) = hw_smooth(y, alpha, beta, gamma, period, mult);
    let residuals: Vec<f64> = y.iter().zip(&fitted).map(|(&o, &f)| o - f).collect();
    let sse = residuals.iter().map(|r| r * r).sum();
    Ok(EtsFit {
        alpha,
        beta,
        gamma,
        level: state.0,
        trend: state.1,
        seasonal: state.2,
        fitted,
        residuals,
        sse,
        period,
        mode: if mult {
            SeasonalMode::Multiplicative
        } else {
            SeasonalMode::Additive
        },
    })
}

type HwState = (f64, f64, Vec<f64>);

#[inline]
fn hw_smooth(
    y: &[f64],
    alpha: f64,
    beta: f64,
    gamma: f64,
    period: usize,
    mult: bool,
) -> (Vec<f64>, HwState) {
    let n = y.len();
    let mut fitted = vec![0.0; n];
    let mut level = y[0];
    let mut trend = if n > 1 { y[1] - y[0] } else { 0.0 };
    let mut seasonal = vec![1.0; period.max(1)];
    if period > 0 && !mult {
        let m = mean(y).unwrap_or(y[0]);
        seasonal = vec![0.0; period];
        for t in 0..n {
            seasonal[t % period] = y[t] - m;
        }
    }

    for t in 0..n {
        let season = if period > 0 {
            seasonal[t % period]
        } else {
            0.0
        };
        let prev_level = level;
        let prev_trend = trend;
        let prev_season = season;

        if period == 0 {
            fitted[t] = level + trend;
            level = alpha * y[t] + (1.0 - alpha) * (level + trend);
            trend = beta * (level - prev_level) + (1.0 - beta) * trend;
        } else if mult {
            fitted[t] = (level + trend) * prev_season;
            level =
                alpha * (y[t] / prev_season.max(1e-12)) + (1.0 - alpha) * (prev_level + prev_trend);
            trend = beta * (level - prev_level) + (1.0 - beta) * prev_trend;
            seasonal[t % period] = gamma * (y[t] / (level * (prev_level + prev_trend).max(1e-12)))
                + (1.0 - gamma) * prev_season;
        } else {
            fitted[t] = level + trend + prev_season;
            level = alpha * (y[t] - prev_season) + (1.0 - alpha) * (prev_level + prev_trend);
            trend = beta * (level - prev_level) + (1.0 - beta) * prev_trend;
            seasonal[t % period] = gamma * (y[t] - level) + (1.0 - gamma) * prev_season;
        }
    }
    (fitted, (level, trend, seasonal))
}

fn forecast_ets(fit: &EtsFit, h: usize) -> TsResult<Vec<f64>> {
    let mut out = Vec::with_capacity(h);
    let mut level = fit.level;
    let trend = fit.trend;
    for i in 0..h {
        let season = if fit.period > 0 {
            fit.seasonal[(fit.fitted.len() + i) % fit.period]
        } else {
            0.0
        };
        let m = i + 1;
        let fc = match fit.mode {
            SeasonalMode::Multiplicative => (level + m as f64 * trend) * season,
            SeasonalMode::Additive if fit.period > 0 => level + m as f64 * trend + season,
            _ => level + m as f64 * trend,
        };
        out.push(fc);
    }
    Ok(out)
}

/// Simple exponential smoothing (convenience).
pub fn ses(y: &[f64], alpha: f64) -> TsResult<Vec<f64>> {
    if !(0.0..=1.0).contains(&alpha) {
        return Err(TsError::Domain("alpha must be in [0,1]".into()));
    }
    let (fitted, _) = hw_smooth(y, alpha, 0.0, 0.0, 0, false);
    Ok(fitted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holt_winters_seasonal() {
        let n = 36;
        let y: Vec<f64> = (0..n)
            .map(|t| {
                let trend = t as f64 * 0.2;
                let season = (t % 12) as f64 * 0.5;
                trend + season + 10.0
            })
            .collect();
        let mut m = EtsModel::holt_winters(12, false);
        m.fit(&y).unwrap();
        let fc = m.forecast(6).unwrap();
        assert_eq!(fc.len(), 6);
        assert!(fc[0] > 10.0);
    }

    #[test]
    fn ses_basic() {
        let y = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let f = ses(&y, 0.5).unwrap();
        assert_eq!(f.len(), 5);
    }
}
