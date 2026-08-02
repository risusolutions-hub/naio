//! Time value of money: fv, pv, pmt, ipmt, ppmt, nper, rate.

use crate::error::{FinError, FinResult};

const EPS: f64 = 1e-10;
const MAX_ITER: usize = 100;

fn when_end(when: i32) -> FinResult<f64> {
    match when {
        0 => Ok(0.0),
        1 => Ok(1.0),
        _ => Err(FinError::Param(format!(
            "when must be 0 (end) or 1 (begin), got {when}"
        ))),
    }
}

/// Future value of an annuity / cash-flow series.
///
/// >>> fv(0.075 / 12.0, 20.0 * 12.0, -2000.0, 0.0, 0) > 800000.0
pub fn fv(rate: f64, nper: f64, pmt: f64, pv: f64, when: i32) -> FinResult<f64> {
    if nper < 0.0 {
        return Err(FinError::Param("nper must be non-negative".into()));
    }
    let w = when_end(when)?;
    if rate.abs() < EPS {
        return Ok(-(pv + pmt * nper));
    }
    let temp = (1.0 + rate).powf(nper);
    let fact = (1.0 + rate * w) * (temp - 1.0) / rate;
    Ok(-(pv * temp + pmt * fact))
}

/// Present value.
///
/// >>> pv(0.05 / 12.0, 10.0 * 12.0, -100.0, 0.0, 0) > 9000.0
pub fn pv(rate: f64, nper: f64, pmt: f64, fv_val: f64, when: i32) -> FinResult<f64> {
    if nper < 0.0 {
        return Err(FinError::Param("nper must be non-negative".into()));
    }
    let w = when_end(when)?;
    if rate.abs() < EPS {
        return Ok(-(fv_val + pmt * nper));
    }
    let temp = (1.0 + rate).powf(nper);
    let fact = (1.0 + rate * w) * (temp - 1.0) / rate;
    Ok(-(fv_val + pmt * fact) / temp)
}

/// Periodic payment.
///
/// >>> pmt(0.05 / 12.0, 30.0 * 12.0, 100000.0, 0.0, 0) < 0.0
pub fn pmt(rate: f64, nper: f64, pv_val: f64, fv_val: f64, when: i32) -> FinResult<f64> {
    if nper <= 0.0 {
        return Err(FinError::Param("nper must be positive".into()));
    }
    let w = when_end(when)?;
    if rate.abs() < EPS {
        return Ok(-(fv_val + pv_val) / nper);
    }
    let temp = (1.0 + rate).powf(nper);
    let fact = (1.0 + rate * w) * (temp - 1.0) / rate;
    Ok(-(fv_val + pv_val * temp) / fact)
}

/// Interest portion of payment for period `per` (1-based).
pub fn ipmt(rate: f64, per: f64, nper: f64, pv_val: f64, fv_val: f64, when: i32) -> FinResult<f64> {
    if per < 1.0 || per > nper {
        return Err(FinError::Param(format!(
            "per must be in 1..=nper ({nper}), got {per}"
        )));
    }
    when_end(when)?;
    if rate.abs() < EPS {
        return Ok(0.0);
    }
    let payment = pmt(rate, nper, pv_val, fv_val, when)?;
    let balance = fv(rate, per - 1.0, payment, pv_val, when)?;
    Ok(balance * rate)
}

/// Principal portion of payment for period `per` (1-based).
pub fn ppmt(rate: f64, per: f64, nper: f64, pv_val: f64, fv_val: f64, when: i32) -> FinResult<f64> {
    let payment = pmt(rate, nper, pv_val, fv_val, when)?;
    let interest = ipmt(rate, per, nper, pv_val, fv_val, when)?;
    Ok(payment - interest)
}

/// Number of periods.
pub fn nper(rate: f64, pmt: f64, pv_val: f64, fv_val: f64, when: i32) -> FinResult<f64> {
    let w = when_end(when)?;
    if rate.abs() < EPS {
        if pmt.abs() < EPS {
            return Err(FinError::Domain(
                "cannot solve nper with zero rate and zero payment".into(),
            ));
        }
        return Ok(-(fv_val + pv_val) / pmt);
    }
    let z = pmt * (1.0 + rate * w) / rate;
    let numer = -fv_val + z;
    let denom = pv_val + z;
    let ratio = numer / denom;
    if ratio <= 0.0 {
        return Err(FinError::Domain("no real solution for nper".into()));
    }
    Ok(ratio.ln() / (1.0 + rate).ln())
}

fn rate_fn(y: f64, nper: f64, pmt: f64, pv_val: f64, fv_val: f64, w: f64) -> (f64, f64) {
    let t1 = (1.0 + y).powf(nper);
    let t2 = (1.0 + y).powf(nper - 1.0);
    if y.abs() < EPS {
        let g = fv_val + pv_val + pmt * nper;
        let gp = nper * pv_val - pmt * nper * (nper - 1.0) / 2.0;
        return (g, gp);
    }
    let t1r = y;
    let g = fv_val + t1 * pv_val + pmt * (1.0 + y * w) / t1r * (t1 - 1.0);
    let gp = nper * t2 * pv_val - pmt / (y * y) * (1.0 + y * w) * (t1 - 1.0)
        + pmt / t1r * (nper * t2 * (1.0 + y * w) + t1 * w);
    (g, gp)
}

/// Interest rate per period (Newton–Raphson).
pub fn rate(
    nper: f64,
    pmt: f64,
    pv_val: f64,
    fv_val: f64,
    when: i32,
    guess: f64,
) -> FinResult<f64> {
    if nper <= 0.0 {
        return Err(FinError::Param("nper must be positive".into()));
    }
    let w = when_end(when)?;
    if pmt.abs() < EPS && pv_val.abs() < EPS && fv_val.abs() < EPS {
        return Err(FinError::Domain("all cash flows are zero".into()));
    }
    let mut y = guess;
    for _ in 0..MAX_ITER {
        if y.abs() < EPS {
            y = EPS;
        }
        let (g, df) = rate_fn(y, nper, pmt, pv_val, fv_val, w);
        if df.abs() < EPS {
            break;
        }
        let y_new = y - g / df;
        if (y_new - y).abs() < EPS {
            return Ok(y_new);
        }
        y = y_new;
    }
    Err(FinError::NonConvergence(format!(
        "rate did not converge within {MAX_ITER} iterations"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fv_pv_roundtrip() {
        let rate = 0.05 / 12.0;
        let nper = 360.0;
        let pv0 = 100_000.0;
        let payment = pmt(rate, nper, pv0, 0.0, 0).unwrap();
        let fv0 = fv(rate, nper, payment, pv0, 0).unwrap();
        assert!(fv0.abs() < 0.01);
    }

    #[test]
    fn fv_monthly_savings() {
        let v = fv(0.075 / 12.0, 20.0 * 12.0, -2000.0, 0.0, 1).unwrap();
        assert!(v > 1_000_000.0);
    }

    #[test]
    fn pv_annuity() {
        let v = pv(0.05 / 12.0, 10.0 * 12.0, -100.0, 0.0, 0).unwrap();
        assert!(v > 9400.0 && v < 9450.0);
    }

    #[test]
    fn pmt_mortgage() {
        let v = pmt(0.05 / 12.0, 30.0 * 12.0, 100_000.0, 0.0, 0).unwrap();
        assert!((v + 536.82).abs() < 0.01);
    }

    #[test]
    fn ipmt_first_period() {
        let r = 0.05 / 12.0;
        let n = 360.0;
        let pv0 = 100_000.0;
        let i = ipmt(r, 1.0, n, pv0, 0.0, 0).unwrap();
        assert!((i + 416.67).abs() < 0.1);
    }

    #[test]
    fn rate_roundtrip() {
        let r = 0.05 / 12.0;
        let n = 360.0;
        let pv0 = 100_000.0;
        let payment = pmt(r, n, pv0, 0.0, 0).unwrap();
        let solved = rate(n, payment, pv0, 0.0, 0, 0.01).unwrap();
        assert!((solved - r).abs() < 1e-8);
    }
}
