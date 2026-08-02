//! Return metrics: simple/log returns, CAGR, Sharpe, drawdown.

use crate::error::{FinError, FinResult};

const EPS: f64 = 1e-15;

/// Period simple returns from price series (length n-1).
pub fn simple_return(prices: &[f64]) -> FinResult<Vec<f64>> {
    if prices.len() < 2 {
        return Err(FinError::Length(
            "simple_return requires at least 2 prices".into(),
        ));
    }
    let mut out = Vec::with_capacity(prices.len() - 1);
    for w in prices.windows(2) {
        let prev = w[0];
        if prev.abs() < EPS {
            return Err(FinError::Domain("zero price in series".into()));
        }
        out.push((w[1] - prev) / prev);
    }
    Ok(out)
}

/// Log returns (length n-1).
pub fn log_return(prices: &[f64]) -> FinResult<Vec<f64>> {
    if prices.len() < 2 {
        return Err(FinError::Length(
            "log_return requires at least 2 prices".into(),
        ));
    }
    let mut out = Vec::with_capacity(prices.len() - 1);
    for w in prices.windows(2) {
        if w[0] <= 0.0 || w[1] <= 0.0 {
            return Err(FinError::Domain(
                "log_return requires strictly positive prices".into(),
            ));
        }
        out.push((w[1] / w[0]).ln());
    }
    Ok(out)
}

/// Cumulative return from period returns (same length as input).
pub fn cumulative_return(returns: &[f64]) -> FinResult<Vec<f64>> {
    if returns.is_empty() {
        return Err(FinError::Empty);
    }
    let mut out = Vec::with_capacity(returns.len());
    let mut acc = 1.0;
    for &r in returns {
        acc *= 1.0 + r;
        out.push(acc - 1.0);
    }
    Ok(out)
}

/// Compound annual growth rate.
pub fn cagr(start: f64, end: f64, periods: f64) -> FinResult<f64> {
    if start <= 0.0 || end <= 0.0 {
        return Err(FinError::Param("start and end must be positive".into()));
    }
    if periods <= 0.0 {
        return Err(FinError::Param("periods must be positive".into()));
    }
    Ok((end / start).powf(1.0 / periods) - 1.0)
}

/// Sharpe ratio (annualized when `periods_per_year` > 0).
pub fn sharpe(returns: &[f64], risk_free: f64, periods_per_year: f64) -> FinResult<f64> {
    if returns.len() < 2 {
        return Err(FinError::Length(
            "sharpe requires at least 2 returns".into(),
        ));
    }
    let n = returns.len() as f64;
    let mean = returns.iter().sum::<f64>() / n;
    let var = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0);
    let std = var.sqrt();
    if std < EPS {
        return Err(FinError::Domain("zero volatility in returns".into()));
    }
    let excess = mean - risk_free;
    let scale = if periods_per_year > 0.0 {
        periods_per_year.sqrt()
    } else {
        1.0
    };
    Ok(excess / std * scale)
}

/// Maximum drawdown result.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawdownResult {
    pub max_drawdown: f64,
    pub peak_idx: usize,
    pub trough_idx: usize,
}

/// Maximum peak-to-trough drawdown on a price/equity curve.
pub fn max_drawdown(prices: &[f64]) -> FinResult<DrawdownResult> {
    if prices.is_empty() {
        return Err(FinError::Empty);
    }
    let mut peak = prices[0];
    let mut peak_idx = 0usize;
    let mut max_dd = 0.0;
    let mut best_peak = 0usize;
    let mut best_trough = 0usize;
    for (i, &p) in prices.iter().enumerate() {
        if p > peak {
            peak = p;
            peak_idx = i;
        }
        if peak.abs() < EPS {
            continue;
        }
        let dd = (peak - p) / peak;
        if dd > max_dd {
            max_dd = dd;
            best_peak = peak_idx;
            best_trough = i;
        }
    }
    Ok(DrawdownResult {
        max_drawdown: max_dd,
        peak_idx: best_peak,
        trough_idx: best_trough,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_ret() {
        let r = simple_return(&[100.0, 110.0, 99.0]).unwrap();
        assert!((r[0] - 0.1).abs() < 1e-10);
        assert!((r[1] + 0.1).abs() < 1e-10);
    }

    #[test]
    fn cagr_ten_years() {
        let r = cagr(100.0, 200.0, 10.0).unwrap();
        assert!((r - 0.07177).abs() < 1e-3);
    }

    #[test]
    fn drawdown() {
        let d = max_drawdown(&[100.0, 120.0, 90.0, 95.0]).unwrap();
        assert!((d.max_drawdown - 0.25).abs() < 1e-10);
        assert_eq!(d.peak_idx, 1);
        assert_eq!(d.trough_idx, 2);
    }
}
