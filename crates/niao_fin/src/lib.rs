//! Financial math for Niao (~numpy-financial + TA-Lib indicators subset).
//!
//! Time value of money, NPV/IRR, amortization, return metrics, and
//! common technical indicators. Zero external dependencies.

mod amort;
mod cashflow;
mod error;
mod indicators;
mod returns;
mod tvm;

pub use amort::{amortization, AmortRow};
pub use cashflow::{irr, mirr, npv};
pub use error::{FinError, FinResult};
pub use indicators::{
    atr, bbands, ema, macd, rsi, sma, stoch, BBandsResult, MacdResult, StochResult,
};
pub use returns::{
    cagr, cumulative_return, log_return, max_drawdown, sharpe, simple_return, DrawdownResult,
};
pub use tvm::{fv, ipmt, nper, pmt, ppmt, pv, rate};
