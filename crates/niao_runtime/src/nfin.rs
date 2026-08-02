//! Native nfin standard library — financial math (~numpy-financial + TA-Lib subset).
//!
//! Import with `import "nfin"` (or `import "std/nfin"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_fin::{
    amortization, atr, bbands, cagr, cumulative_return, ema, fv, ipmt, irr, log_return, macd,
    max_drawdown, mirr, nper, npv, pmt, ppmt, pv, rate, rsi, sharpe, simple_return, sma, stoch,
    FinError,
};
use std::collections::HashMap;
use std::rc::Rc;

const E4110: u32 = codes::E4110_NFIN_ARITY;
const E4111: u32 = codes::E4111_NFIN_ERROR;
const E4112: u32 = codes::E4112_NFIN_TYPE;
const E4113: u32 = codes::E4113_NFIN_PARAM;
const E4114: u32 = codes::E4114_NFIN_LENGTH;
const E4115: u32 = codes::E4115_NFIN_NON_CONVERGENCE;

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E4110,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(
    args: &[ValueRef],
    min: usize,
    max: usize,
    name: &str,
    span: Span,
) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E4110,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E4112, msg.into())
}

fn soft_err(span: Span, err: FinError) -> ValueRef {
    let code = match &err {
        FinError::Empty | FinError::Length(_) => E4114,
        FinError::Param(_) => E4113,
        FinError::NonConvergence(_) => E4115,
        FinError::Domain(_) => E4111,
    };
    error_value(code, "nfin_error", err.message(), span)
}

fn num_from(v: &Value, name: &str, span: Span) -> NiaoResult<f64> {
    match v {
        Value::Int(n) => Ok(*n as f64),
        Value::Float(f) => Ok(*f),
        other => Err(type_err(
            span,
            format!("{name}() expects a number, got {}", other.type_name()),
        )),
    }
}

fn float_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<f64> {
    num_from(&*args[idx].borrow(), name, span)
}

fn int_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        Value::Float(f) => Ok(*f as i64),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects int as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn optional_float(args: &[ValueRef], idx: usize, default: f64) -> f64 {
    args.get(idx)
        .map(|v| match &*v.borrow() {
            Value::Float(f) => *f,
            Value::Int(n) => *n as f64,
            Value::Nil => default,
            _ => default,
        })
        .unwrap_or(default)
}

fn optional_int(args: &[ValueRef], idx: usize, default: i32) -> i32 {
    args.get(idx)
        .map(|v| match &*v.borrow() {
            Value::Int(n) => *n as i32,
            Value::Float(f) => *f as i32,
            Value::Nil => default,
            _ => default,
        })
        .unwrap_or(default)
}

fn floats(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<f64>> {
    match &*args[idx].borrow() {
        Value::FloatArray(v) => Ok(v.clone()),
        Value::IntArray(v) => Ok(v.iter().map(|n| *n as f64).collect()),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(num_from(&*item.borrow(), name, span)?);
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() expects array/float_array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn ok_float(v: f64) -> ValueRef {
    Value::Float(v).ref_cell()
}

fn ok_floats(v: Vec<f64>) -> ValueRef {
    Value::FloatArray(v).ref_cell()
}

fn when_arg(args: &[ValueRef], idx: usize) -> i32 {
    optional_int(args, idx, 0)
}

// >>> import "nfin"; nfin.fv(0.05 / 12.0, 12.0, -100.0, 0.0) > 1200.0
fn nfin_fv(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 5, "nfin_fv", span)?;
    let rate = float_arg(args, 0, "nfin_fv", span)?;
    let nper = float_arg(args, 1, "nfin_fv", span)?;
    let pmt = float_arg(args, 2, "nfin_fv", span)?;
    let pv0 = optional_float(args, 3, 0.0);
    let when = when_arg(args, 4);
    match fv(rate, nper, pmt, pv0, when) {
        Ok(v) => Ok(ok_float(v)),
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nfin"; nfin.pv(0.05 / 12.0, 12.0, -100.0) > 1100.0
fn nfin_pv(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 5, "nfin_pv", span)?;
    let rate = float_arg(args, 0, "nfin_pv", span)?;
    let nper = float_arg(args, 1, "nfin_pv", span)?;
    let pmt = float_arg(args, 2, "nfin_pv", span)?;
    let fv0 = optional_float(args, 3, 0.0);
    let when = when_arg(args, 4);
    match pv(rate, nper, pmt, fv0, when) {
        Ok(v) => Ok(ok_float(v)),
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nfin"; nfin.pmt(0.05 / 12.0, 360.0, 100000.0) < 0.0
fn nfin_pmt(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 5, "nfin_pmt", span)?;
    let rate = float_arg(args, 0, "nfin_pmt", span)?;
    let nper = float_arg(args, 1, "nfin_pmt", span)?;
    let pv0 = float_arg(args, 2, "nfin_pmt", span)?;
    let fv0 = optional_float(args, 3, 0.0);
    let when = when_arg(args, 4);
    match pmt(rate, nper, pv0, fv0, when) {
        Ok(v) => Ok(ok_float(v)),
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nfin"; nfin.ipmt(0.05 / 12.0, 1.0, 360.0, 100000.0) < 0.0
fn nfin_ipmt(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 4, 6, "nfin_ipmt", span)?;
    let rate = float_arg(args, 0, "nfin_ipmt", span)?;
    let per = float_arg(args, 1, "nfin_ipmt", span)?;
    let nper = float_arg(args, 2, "nfin_ipmt", span)?;
    let pv0 = float_arg(args, 3, "nfin_ipmt", span)?;
    let fv0 = optional_float(args, 4, 0.0);
    let when = when_arg(args, 5);
    match ipmt(rate, per, nper, pv0, fv0, when) {
        Ok(v) => Ok(ok_float(v)),
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nfin"; nfin.ppmt(0.05 / 12.0, 1.0, 360.0, 100000.0) < 0.0
fn nfin_ppmt(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 4, 6, "nfin_ppmt", span)?;
    let rate = float_arg(args, 0, "nfin_ppmt", span)?;
    let per = float_arg(args, 1, "nfin_ppmt", span)?;
    let nper = float_arg(args, 2, "nfin_ppmt", span)?;
    let pv0 = float_arg(args, 3, "nfin_ppmt", span)?;
    let fv0 = optional_float(args, 4, 0.0);
    let when = when_arg(args, 5);
    match ppmt(rate, per, nper, pv0, fv0, when) {
        Ok(v) => Ok(ok_float(v)),
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nfin"; nfin.nper(0.05 / 12.0, -536.82, 100000.0) > 300.0
fn nfin_nper(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 5, "nfin_nper", span)?;
    let rate = float_arg(args, 0, "nfin_nper", span)?;
    let pmt = float_arg(args, 1, "nfin_nper", span)?;
    let pv0 = float_arg(args, 2, "nfin_nper", span)?;
    let fv0 = optional_float(args, 3, 0.0);
    let when = when_arg(args, 4);
    match nper(rate, pmt, pv0, fv0, when) {
        Ok(v) => Ok(ok_float(v)),
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nfin"; nfin.rate(360.0, -536.82, 100000.0) > 0.0
fn nfin_rate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 6, "nfin_rate", span)?;
    let nper = float_arg(args, 0, "nfin_rate", span)?;
    let pmt = float_arg(args, 1, "nfin_rate", span)?;
    let pv0 = float_arg(args, 2, "nfin_rate", span)?;
    let fv0 = optional_float(args, 3, 0.0);
    let when = when_arg(args, 4);
    let guess = optional_float(args, 5, 0.1);
    match rate(nper, pmt, pv0, fv0, when, guess) {
        Ok(v) => Ok(ok_float(v)),
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nfin"; nfin.npv(0.05, [-100.0, 110.0]) > 0.0
fn nfin_npv(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nfin_npv", span)?;
    let r = float_arg(args, 0, "nfin_npv", span)?;
    let values = floats(args, 1, "nfin_npv", span)?;
    match npv(r, &values) {
        Ok(v) => Ok(ok_float(v)),
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nfin"; nfin.irr([-100.0, 110.0]) > 0.0
fn nfin_irr(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nfin_irr", span)?;
    let values = floats(args, 0, "nfin_irr", span)?;
    let guess = optional_float(args, 1, 0.1);
    match irr(&values, guess) {
        Ok(v) => Ok(ok_float(v)),
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nfin"; nfin.mirr([-1000.0, 300.0, 400.0, 500.0], 0.05, 0.08) > 0.0
fn nfin_mirr(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nfin_mirr", span)?;
    let values = floats(args, 0, "nfin_mirr", span)?;
    let finance_rate = float_arg(args, 1, "nfin_mirr", span)?;
    let reinvest_rate = float_arg(args, 2, "nfin_mirr", span)?;
    match mirr(&values, finance_rate, reinvest_rate) {
        Ok(v) => Ok(ok_float(v)),
        Err(e) => Ok(soft_err(span, e)),
    }
}

fn row_obj(period: usize, payment: f64, interest: f64, principal: f64, balance: f64) -> ValueRef {
    let mut m = HashMap::new();
    m.insert("period".to_string(), Value::Int(period as i64).ref_cell());
    m.insert("payment".to_string(), Value::Float(payment).ref_cell());
    m.insert("interest".to_string(), Value::Float(interest).ref_cell());
    m.insert("principal".to_string(), Value::Float(principal).ref_cell());
    m.insert("balance".to_string(), Value::Float(balance).ref_cell());
    Value::Object(m).ref_cell()
}

// >>> import "nfin"; len(nfin.amortization(0.05 / 12.0, 12, 10000.0)) == 12
fn nfin_amortization(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nfin_amortization", span)?;
    let r = float_arg(args, 0, "nfin_amortization", span)?;
    let nper = int_arg(args, 1, "nfin_amortization", span)?;
    if nper < 0 {
        return Ok(soft_err(
            span,
            FinError::Param("nper must be non-negative".into()),
        ));
    }
    let pv0 = float_arg(args, 2, "nfin_amortization", span)?;
    let when = when_arg(args, 3);
    match amortization(r, nper as usize, pv0, when) {
        Ok(rows) => {
            let out: Vec<ValueRef> = rows
                .into_iter()
                .map(|row| {
                    row_obj(
                        row.period,
                        row.payment,
                        row.interest,
                        row.principal,
                        row.balance,
                    )
                })
                .collect();
            Ok(Value::Array(out).ref_cell())
        }
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nfin"; len(nfin.simple_return([100.0, 110.0])) == 1
fn nfin_simple_return(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfin_simple_return", span)?;
    let prices = floats(args, 0, "nfin_simple_return", span)?;
    match simple_return(&prices) {
        Ok(v) => Ok(ok_floats(v)),
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nfin"; len(nfin.log_return([100.0, 110.0])) == 1
fn nfin_log_return(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfin_log_return", span)?;
    let prices = floats(args, 0, "nfin_log_return", span)?;
    match log_return(&prices) {
        Ok(v) => Ok(ok_floats(v)),
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nfin"; len(nfin.cumulative_return([0.1, 0.05])) == 2
fn nfin_cumulative_return(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfin_cumulative_return", span)?;
    let returns = floats(args, 0, "nfin_cumulative_return", span)?;
    match cumulative_return(&returns) {
        Ok(v) => Ok(ok_floats(v)),
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nfin"; nfin.cagr(100.0, 200.0, 10.0) > 0.07
fn nfin_cagr(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nfin_cagr", span)?;
    let start = float_arg(args, 0, "nfin_cagr", span)?;
    let end = float_arg(args, 1, "nfin_cagr", span)?;
    let periods = float_arg(args, 2, "nfin_cagr", span)?;
    match cagr(start, end, periods) {
        Ok(v) => Ok(ok_float(v)),
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nfin"; nfin.sharpe([0.01, -0.005, 0.02, 0.015], 0.0, 252.0) > 0.0
fn nfin_sharpe(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nfin_sharpe", span)?;
    let returns = floats(args, 0, "nfin_sharpe", span)?;
    let rf = optional_float(args, 1, 0.0);
    let ppy = optional_float(args, 2, 252.0);
    match sharpe(&returns, rf, ppy) {
        Ok(v) => Ok(ok_float(v)),
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nfin"; nfin.max_drawdown([100.0, 120.0, 90.0]).max_drawdown > 0.0
fn nfin_max_drawdown(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfin_max_drawdown", span)?;
    let prices = floats(args, 0, "nfin_max_drawdown", span)?;
    match max_drawdown(&prices) {
        Ok(d) => {
            let mut m = HashMap::new();
            m.insert(
                "max_drawdown".to_string(),
                Value::Float(d.max_drawdown).ref_cell(),
            );
            m.insert(
                "peak_idx".to_string(),
                Value::Int(d.peak_idx as i64).ref_cell(),
            );
            m.insert(
                "trough_idx".to_string(),
                Value::Int(d.trough_idx as i64).ref_cell(),
            );
            Ok(Value::Object(m).ref_cell())
        }
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nfin"; len(nfin.sma([1.0, 2.0, 3.0, 4.0, 5.0], 3)) == 5
fn nfin_sma(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nfin_sma", span)?;
    let values = floats(args, 0, "nfin_sma", span)?;
    let period = int_arg(args, 1, "nfin_sma", span)?;
    if period <= 0 {
        return Ok(soft_err(
            span,
            FinError::Param("period must be positive".into()),
        ));
    }
    match sma(&values, period as usize) {
        Ok(v) => Ok(ok_floats(v)),
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nfin"; len(nfin.ema([1.0, 2.0, 3.0, 4.0, 5.0], 3)) == 5
fn nfin_ema(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nfin_ema", span)?;
    let values = floats(args, 0, "nfin_ema", span)?;
    let period = int_arg(args, 1, "nfin_ema", span)?;
    if period <= 0 {
        return Ok(soft_err(
            span,
            FinError::Param("period must be positive".into()),
        ));
    }
    match ema(&values, period as usize) {
        Ok(v) => Ok(ok_floats(v)),
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nfin"; len(nfin.rsi([100.0, 101.0, 102.0, 101.0, 100.0, 99.0, 98.0, 99.0, 100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 106.0], 14)) == 15
fn nfin_rsi(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nfin_rsi", span)?;
    let values = floats(args, 0, "nfin_rsi", span)?;
    let period = optional_int(args, 1, 14) as usize;
    if period == 0 {
        return Ok(soft_err(
            span,
            FinError::Param("period must be positive".into()),
        ));
    }
    match rsi(&values, period) {
        Ok(v) => Ok(ok_floats(v)),
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nfin"; len(nfin.macd([100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 106.0, 107.0, 108.0, 109.0, 110.0, 111.0, 112.0, 113.0, 114.0, 115.0, 116.0, 117.0, 118.0, 119.0, 120.0, 121.0, 122.0, 123.0, 124.0, 125.0, 126.0, 127.0, 128.0, 129.0]).macd) == 30
fn nfin_macd(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 4, "nfin_macd", span)?;
    let values = floats(args, 0, "nfin_macd", span)?;
    let fast = optional_int(args, 1, 12) as usize;
    let slow = optional_int(args, 2, 26) as usize;
    let signal = optional_int(args, 3, 9) as usize;
    match macd(&values, fast, slow, signal) {
        Ok(m) => {
            let mut out = HashMap::new();
            out.insert("macd".to_string(), ok_floats(m.macd));
            out.insert("signal".to_string(), ok_floats(m.signal));
            out.insert("histogram".to_string(), ok_floats(m.histogram));
            Ok(Value::Object(out).ref_cell())
        }
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nfin"; len(nfin.bbands([100.0, 101.0, 102.0, 101.0, 100.0, 99.0, 98.0, 99.0, 100.0, 101.0], 5).upper) == 10
fn nfin_bbands(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nfin_bbands", span)?;
    let values = floats(args, 0, "nfin_bbands", span)?;
    let period = optional_int(args, 1, 20) as usize;
    let nbdev = optional_float(args, 2, 2.0);
    match bbands(&values, period, nbdev) {
        Ok(b) => {
            let mut out = HashMap::new();
            out.insert("upper".to_string(), ok_floats(b.upper));
            out.insert("middle".to_string(), ok_floats(b.middle));
            out.insert("lower".to_string(), ok_floats(b.lower));
            Ok(Value::Object(out).ref_cell())
        }
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nfin"; len(nfin.atr([10.0, 11.0, 12.0], [9.0, 10.0, 10.5], [9.5, 10.5, 11.5], 2)) == 3
fn nfin_atr(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nfin_atr", span)?;
    let high = floats(args, 0, "nfin_atr", span)?;
    let low = floats(args, 1, "nfin_atr", span)?;
    let close = floats(args, 2, "nfin_atr", span)?;
    let period = optional_int(args, 3, 14) as usize;
    match atr(&high, &low, &close, period) {
        Ok(v) => Ok(ok_floats(v)),
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "nfin"; len(nfin.stoch([10.0, 11.0, 12.0, 11.5, 12.5, 13.0, 12.0, 11.0, 10.5, 11.0, 11.5, 12.0, 12.5, 13.0, 12.5], [9.0, 10.0, 10.5, 10.0, 11.0, 11.5, 10.5, 9.5, 9.0, 9.5, 10.0, 10.5, 11.0, 11.5, 11.0], [9.5, 10.5, 11.5, 10.8, 12.0, 12.2, 11.0, 10.0, 9.8, 10.5, 11.0, 11.5, 12.0, 12.5, 12.0], 14, 3).k) == 15
fn nfin_stoch(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 5, "nfin_stoch", span)?;
    let high = floats(args, 0, "nfin_stoch", span)?;
    let low = floats(args, 1, "nfin_stoch", span)?;
    let close = floats(args, 2, "nfin_stoch", span)?;
    let k_period = optional_int(args, 3, 14) as usize;
    let d_period = optional_int(args, 4, 3) as usize;
    match stoch(&high, &low, &close, k_period, d_period) {
        Ok(s) => {
            let mut out = HashMap::new();
            out.insert("k".to_string(), ok_floats(s.k));
            out.insert("d".to_string(), ok_floats(s.d));
            Ok(Value::Object(out).ref_cell())
        }
        Err(e) => Ok(soft_err(span, e)),
    }
}

macro_rules! nfin_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nfin_fns![
    ("nfin_fv", "fv", nfin_fv),
    ("nfin_pv", "pv", nfin_pv),
    ("nfin_pmt", "pmt", nfin_pmt),
    ("nfin_ipmt", "ipmt", nfin_ipmt),
    ("nfin_ppmt", "ppmt", nfin_ppmt),
    ("nfin_nper", "nper", nfin_nper),
    ("nfin_rate", "rate", nfin_rate),
    ("nfin_npv", "npv", nfin_npv),
    ("nfin_irr", "irr", nfin_irr),
    ("nfin_mirr", "mirr", nfin_mirr),
    ("nfin_amortization", "amortization", nfin_amortization),
    ("nfin_simple_return", "simple_return", nfin_simple_return),
    ("nfin_log_return", "log_return", nfin_log_return),
    (
        "nfin_cumulative_return",
        "cumulative_return",
        nfin_cumulative_return
    ),
    ("nfin_cagr", "cagr", nfin_cagr),
    ("nfin_sharpe", "sharpe", nfin_sharpe),
    ("nfin_max_drawdown", "max_drawdown", nfin_max_drawdown),
    ("nfin_sma", "sma", nfin_sma),
    ("nfin_ema", "ema", nfin_ema),
    ("nfin_rsi", "rsi", nfin_rsi),
    ("nfin_macd", "macd", nfin_macd),
    ("nfin_bbands", "bbands", nfin_bbands),
    ("nfin_atr", "atr", nfin_atr),
    ("nfin_stoch", "stoch", nfin_stoch),
];

pub const MODULE_NAME: &str = "nfin";
pub const MODULE_PATHS: &[&str] = &["nfin", "std/nfin"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs()
        .into_iter()
        .map(|(f, _, fn_)| (f, fn_))
        .collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}
