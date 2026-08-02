//! NPV, IRR, MIRR.

use crate::error::{FinError, FinResult};

const EPS: f64 = 1e-10;
const MAX_ITER: usize = 100;

/// Net present value of cash flows (first flow at t=0).
///
/// >>> npv(0.05, [-15000.0, 1500.0, 2500.0, 3500.0, 4500.0, 6000.0]) > 100.0
pub fn npv(rate: f64, values: &[f64]) -> FinResult<f64> {
    if values.is_empty() {
        return Err(FinError::Empty);
    }
    if rate <= -1.0 {
        return Err(FinError::Param("rate must be greater than -1".into()));
    }
    let mut total = 0.0;
    let mut denom = 1.0;
    for &v in values {
        total += v / denom;
        denom *= 1.0 + rate;
    }
    Ok(total)
}

fn npv_at(rate: f64, values: &[f64]) -> f64 {
    let mut total = 0.0;
    let mut denom = 1.0;
    for &v in values {
        total += v / denom;
        denom *= 1.0 + rate;
    }
    total
}

fn npv_deriv(rate: f64, values: &[f64]) -> f64 {
    let mut total = 0.0;
    let mut denom = (1.0 + rate).powi(2);
    for (i, &v) in values.iter().enumerate().skip(1) {
        let t = (i + 1) as f64;
        total -= t * v / denom;
        denom *= 1.0 + rate;
    }
    total
}

/// Internal rate of return (Newton–Raphson on NPV).
///
/// >>> irr([-100.0, 39.0, 59.0, 55.0, 20.0]) > 0.27
pub fn irr(values: &[f64], guess: f64) -> FinResult<f64> {
    if values.len() < 2 {
        return Err(FinError::Length(
            "irr requires at least 2 cash flows".into(),
        ));
    }
    let has_pos = values.iter().any(|&v| v > 0.0);
    let has_neg = values.iter().any(|&v| v < 0.0);
    if !has_pos || !has_neg {
        return Err(FinError::Domain(
            "irr requires both positive and negative cash flows".into(),
        ));
    }
    let mut rate = guess;
    for _ in 0..MAX_ITER {
        let f = npv_at(rate, values);
        if f.abs() < EPS {
            return Ok(rate);
        }
        let df = npv_deriv(rate, values);
        if df.abs() < EPS {
            break;
        }
        let next = rate - f / df;
        if (next - rate).abs() < EPS {
            return Ok(next);
        }
        rate = next;
    }
    Err(FinError::NonConvergence(format!(
        "irr did not converge within {MAX_ITER} iterations"
    )))
}

/// Modified internal rate of return.
pub fn mirr(values: &[f64], finance_rate: f64, reinvest_rate: f64) -> FinResult<f64> {
    if values.len() < 2 {
        return Err(FinError::Length(
            "mirr requires at least 2 cash flows".into(),
        ));
    }
    let has_pos = values.iter().any(|&v| v > 0.0);
    let has_neg = values.iter().any(|&v| v < 0.0);
    if !has_pos || !has_neg {
        return Err(FinError::Domain(
            "mirr requires both positive and negative cash flows".into(),
        ));
    }
    let n = values.len();
    let mut numer = 0.0;
    let mut denom = 0.0;
    for (i, &v) in values.iter().enumerate() {
        if v > 0.0 {
            let exp = (n - 1 - i) as f64;
            numer += v * (1.0 + reinvest_rate).powf(exp);
        } else if v < 0.0 {
            denom += v.abs() / (1.0 + finance_rate).powi(i as i32);
        }
    }
    if denom.abs() < EPS {
        return Err(FinError::Domain("mirr denominator is zero".into()));
    }
    Ok((numer / denom).powf(1.0 / (n as f64 - 1.0)) - 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npv_known() {
        let cf = [-15000.0, 1500.0, 2500.0, 3500.0, 4500.0, 6000.0];
        let v = npv(0.05, &cf).unwrap();
        assert!((v - 122.894).abs() < 0.01);
    }

    #[test]
    fn irr_known() {
        let cf = [-100.0, 39.0, 59.0, 55.0, 20.0];
        let r = irr(&cf, 0.1).unwrap();
        assert!((r - 0.280948).abs() < 1e-4);
    }

    #[test]
    fn mirr_basic() {
        let cf = [-1000.0, 300.0, 400.0, 500.0];
        let r = mirr(&cf, 0.05, 0.08).unwrap();
        assert!(r.is_finite() && r > 0.0);
    }

    #[test]
    fn npv_empty() {
        assert!(npv(0.1, &[]).is_err());
    }
}
