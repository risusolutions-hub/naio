//! Technical indicators (~TA-Lib common subset).

use crate::error::{FinError, FinResult};

const NAN: f64 = f64::NAN;

fn validate_period(period: usize, name: &str) -> FinResult<()> {
    if period == 0 {
        return Err(FinError::Param(format!("{name} period must be positive")));
    }
    Ok(())
}

/// Simple moving average (NaN until first full window).
///
/// >>> sma([1.0, 2.0, 3.0, 4.0, 5.0], 3)[2] == 2.0
pub fn sma(values: &[f64], period: usize) -> FinResult<Vec<f64>> {
    validate_period(period, "sma")?;
    if values.is_empty() {
        return Err(FinError::Empty);
    }
    if values.len() < period {
        return Ok(vec![NAN; values.len()]);
    }
    let mut out = vec![NAN; period - 1];
    let mut sum = values[..period].iter().sum::<f64>();
    out.push(sum / period as f64);
    for i in period..values.len() {
        sum += values[i] - values[i - period];
        out.push(sum / period as f64);
    }
    Ok(out)
}

/// Exponential moving average (Wilder/standard alpha = 2/(period+1)).
pub fn ema(values: &[f64], period: usize) -> FinResult<Vec<f64>> {
    validate_period(period, "ema")?;
    if values.is_empty() {
        return Err(FinError::Empty);
    }
    if values.len() < period {
        return Ok(vec![NAN; values.len()]);
    }
    let alpha = 2.0 / (period as f64 + 1.0);
    let seed = values[..period].iter().sum::<f64>() / period as f64;
    let mut out = vec![NAN; period - 1];
    let mut prev = seed;
    out.push(prev);
    for &v in &values[period..] {
        prev = alpha * v + (1.0 - alpha) * prev;
        out.push(prev);
    }
    Ok(out)
}

/// Relative strength index (Wilder smoothing, default period 14).
pub fn rsi(values: &[f64], period: usize) -> FinResult<Vec<f64>> {
    validate_period(period, "rsi")?;
    if values.len() < period + 1 {
        return Ok(vec![NAN; values.len()]);
    }
    let mut out = vec![NAN; period];
    let mut gains = 0.0;
    let mut losses = 0.0;
    for i in 1..=period {
        let diff = values[i] - values[i - 1];
        if diff >= 0.0 {
            gains += diff;
        } else {
            losses -= diff;
        }
    }
    let mut avg_gain = gains / period as f64;
    let mut avg_loss = losses / period as f64;
    out.push(100.0 - 100.0 / (1.0 + avg_gain / avg_loss.max(1e-15)));
    for i in (period + 1)..values.len() {
        let diff = values[i] - values[i - 1];
        let gain = diff.max(0.0);
        let loss = (-diff).max(0.0);
        avg_gain = (avg_gain * (period as f64 - 1.0) + gain) / period as f64;
        avg_loss = (avg_loss * (period as f64 - 1.0) + loss) / period as f64;
        let rs = avg_gain / avg_loss.max(1e-15);
        out.push(100.0 - 100.0 / (1.0 + rs));
    }
    Ok(out)
}

/// MACD line, signal, and histogram.
#[derive(Debug, Clone, PartialEq)]
pub struct MacdResult {
    pub macd: Vec<f64>,
    pub signal: Vec<f64>,
    pub histogram: Vec<f64>,
}

pub fn macd(
    values: &[f64],
    fast: usize,
    slow: usize,
    signal_period: usize,
) -> FinResult<MacdResult> {
    if fast == 0 || slow == 0 || signal_period == 0 {
        return Err(FinError::Param("macd periods must be positive".into()));
    }
    if fast >= slow {
        return Err(FinError::Param(
            "macd fast period must be less than slow".into(),
        ));
    }
    if values.is_empty() {
        return Err(FinError::Empty);
    }
    let ema_fast = ema(values, fast)?;
    let ema_slow = ema(values, slow)?;
    let macd_line: Vec<f64> = ema_fast
        .iter()
        .zip(&ema_slow)
        .map(|(f, s)| if f.is_nan() || s.is_nan() { NAN } else { f - s })
        .collect();
    let valid: Vec<f64> = macd_line.iter().copied().filter(|v| !v.is_nan()).collect();
    if valid.len() < signal_period {
        return Ok(MacdResult {
            macd: macd_line.clone(),
            signal: vec![NAN; macd_line.len()],
            histogram: vec![NAN; macd_line.len()],
        });
    }
    let sig = ema(&valid, signal_period)?;
    let offset = macd_line.len() - valid.len();
    let mut signal = vec![NAN; offset];
    signal.extend(sig);
    let histogram: Vec<f64> = macd_line
        .iter()
        .zip(&signal)
        .map(|(m, s)| if m.is_nan() || s.is_nan() { NAN } else { m - s })
        .collect();
    Ok(MacdResult {
        macd: macd_line,
        signal,
        histogram,
    })
}

/// Bollinger bands (middle = SMA, bands at ± nbdev * rolling std).
#[derive(Debug, Clone, PartialEq)]
pub struct BBandsResult {
    pub upper: Vec<f64>,
    pub middle: Vec<f64>,
    pub lower: Vec<f64>,
}

pub fn bbands(values: &[f64], period: usize, nbdev: f64) -> FinResult<BBandsResult> {
    validate_period(period, "bbands")?;
    if nbdev <= 0.0 {
        return Err(FinError::Param("nbdev must be positive".into()));
    }
    if values.is_empty() {
        return Err(FinError::Empty);
    }
    let middle = sma(values, period)?;
    let mut upper = vec![NAN; values.len()];
    let mut lower = vec![NAN; values.len()];
    if values.len() >= period {
        for i in (period - 1)..values.len() {
            let slice = &values[i + 1 - period..=i];
            let m = middle[i];
            if m.is_nan() {
                continue;
            }
            let var = slice.iter().map(|x| (x - m).powi(2)).sum::<f64>() / period as f64;
            let std = var.sqrt();
            upper[i] = m + nbdev * std;
            lower[i] = m - nbdev * std;
        }
    }
    Ok(BBandsResult {
        upper,
        middle,
        lower,
    })
}

/// Average true range (Wilder smoothing).
pub fn atr(high: &[f64], low: &[f64], close: &[f64], period: usize) -> FinResult<Vec<f64>> {
    validate_period(period, "atr")?;
    let n = high.len();
    if low.len() != n || close.len() != n {
        return Err(FinError::Length(
            "atr high/low/close must have equal length".into(),
        ));
    }
    if n == 0 {
        return Err(FinError::Empty);
    }
    let mut tr = vec![0.0; n];
    tr[0] = high[0] - low[0];
    for i in 1..n {
        let hl = high[i] - low[i];
        let hc = (high[i] - close[i - 1]).abs();
        let lc = (low[i] - close[i - 1]).abs();
        tr[i] = hl.max(hc).max(lc);
    }
    let mut out = vec![NAN; period - 1];
    if n < period {
        out.extend(vec![NAN; n.saturating_sub(period - 1)]);
        return Ok(out);
    }
    let seed = tr[..period].iter().sum::<f64>() / period as f64;
    out.push(seed);
    let mut prev = seed;
    for i in period..n {
        prev = (prev * (period as f64 - 1.0) + tr[i]) / period as f64;
        out.push(prev);
    }
    Ok(out)
}

/// Stochastic oscillator (%K smoothed to %D).
#[derive(Debug, Clone, PartialEq)]
pub struct StochResult {
    pub k: Vec<f64>,
    pub d: Vec<f64>,
}

pub fn stoch(
    high: &[f64],
    low: &[f64],
    close: &[f64],
    k_period: usize,
    d_period: usize,
) -> FinResult<StochResult> {
    validate_period(k_period, "stoch k_period")?;
    validate_period(d_period, "stoch d_period")?;
    let n = close.len();
    if high.len() != n || low.len() != n {
        return Err(FinError::Length(
            "stoch high/low/close must have equal length".into(),
        ));
    }
    if n == 0 {
        return Err(FinError::Empty);
    }
    let mut raw_k = vec![NAN; n];
    if n >= k_period {
        for i in (k_period - 1)..n {
            let h_slice = &high[i + 1 - k_period..=i];
            let l_slice = &low[i + 1 - k_period..=i];
            let highest = h_slice.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let lowest = l_slice.iter().copied().fold(f64::INFINITY, f64::min);
            let denom = highest - lowest;
            if denom.abs() < 1e-15 {
                raw_k[i] = 50.0;
            } else {
                raw_k[i] = 100.0 * (close[i] - lowest) / denom;
            }
        }
    }
    let valid: Vec<f64> = raw_k.iter().copied().filter(|v| !v.is_nan()).collect();
    let d_vals = if valid.len() >= d_period {
        sma(&valid, d_period)?
    } else {
        vec![NAN; valid.len()]
    };
    let offset = n - valid.len();
    let mut d = vec![NAN; offset];
    d.extend(d_vals);
    Ok(StochResult { k: raw_k, d })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sma_basic() {
        let v = sma(&[1.0, 2.0, 3.0, 4.0, 5.0], 3).unwrap();
        assert!(v[0].is_nan());
        assert!((v[2] - 2.0).abs() < 1e-10);
        assert!((v[4] - 4.0).abs() < 1e-10);
    }

    #[test]
    fn ema_length() {
        let v = ema(&[1.0, 2.0, 3.0, 4.0, 5.0], 3).unwrap();
        assert_eq!(v.len(), 5);
        assert!(!v[4].is_nan());
    }

    #[test]
    fn rsi_range() {
        let prices: Vec<f64> = (0..30).map(|i| 100.0 + (i as f64).sin() * 5.0).collect();
        let r = rsi(&prices, 14).unwrap();
        let last = r.last().unwrap();
        assert!(*last >= 0.0 && *last <= 100.0);
    }

    #[test]
    fn macd_shape() {
        let prices: Vec<f64> = (0..50).map(|i| 100.0 + i as f64 * 0.5).collect();
        let m = macd(&prices, 12, 26, 9).unwrap();
        assert_eq!(m.macd.len(), prices.len());
    }

    #[test]
    fn bbands_order() {
        let v: Vec<f64> = (0..20).map(|i| 100.0 + i as f64).collect();
        let b = bbands(&v, 5, 2.0).unwrap();
        let i = 10;
        assert!(b.upper[i] > b.middle[i]);
        assert!(b.middle[i] > b.lower[i]);
    }
}
