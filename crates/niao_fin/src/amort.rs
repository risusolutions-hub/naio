//! Amortization schedule generation.

use crate::error::{FinError, FinResult};
use crate::tvm::{fv, pmt};

/// One row of an amortization schedule.
#[derive(Debug, Clone, PartialEq)]
pub struct AmortRow {
    pub period: usize,
    pub payment: f64,
    pub interest: f64,
    pub principal: f64,
    pub balance: f64,
}

/// Full amortization schedule for a loan.
///
/// >>> schedule = amortization(0.05 / 12.0, 360, 100000.0, 0); len(schedule) == 360
pub fn amortization(rate: f64, nper: usize, pv: f64, when: i32) -> FinResult<Vec<AmortRow>> {
    if nper == 0 {
        return Ok(vec![]);
    }
    if pv <= 0.0 {
        return Err(FinError::Param(
            "pv must be positive for amortization".into(),
        ));
    }
    let payment = pmt(rate, nper as f64, pv, 0.0, when)?;
    let mut schedule = Vec::with_capacity(nper);
    let mut balance = pv;
    for per in 1..=nper {
        let interest = if rate.abs() < 1e-15 {
            0.0
        } else {
            balance * rate
        };
        let principal = payment - interest;
        balance = -fv(rate, 1.0, payment, balance, when)?;
        schedule.push(AmortRow {
            period: per,
            payment,
            interest,
            principal,
            balance: balance.max(0.0),
        });
    }
    Ok(schedule)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_length() {
        let s = amortization(0.05 / 12.0, 360, 100_000.0, 0).unwrap();
        assert_eq!(s.len(), 360);
        assert!((s[0].payment + 536.82).abs() < 0.01);
        assert!(s.last().unwrap().balance < 1.0);
    }

    #[test]
    fn zero_nper() {
        assert!(amortization(0.05, 0, 1000.0, 0).unwrap().is_empty());
    }
}
