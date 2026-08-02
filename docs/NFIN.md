# nfin standard library

Financial math: time value of money, NPV/IRR, loan amortization, return metrics, and common technical indicators. Native Rust implementation — a practical **numpy-financial** + **TA-Lib** subset.

## Import

```niao
import "nfin"
```

Paths `import "std/nfin"` and `import "nfin"` are equivalent.

## Quick start

```niao
import "nfin"

// Mortgage payment and full amortization schedule
let rate = 0.05 / 12.0
let payment = nfin.pmt(rate, 360.0, 250000.0)
let schedule = nfin.amortization(rate, 360, 250000.0)
print("monthly payment", payment, "final balance", schedule[359].balance)

// Project IRR and NPV
let cf = [-50000.0, 12000.0, 15000.0, 18000.0, 22000.0, 25000.0]
print("NPV @ 8%:", nfin.npv(0.08, cf))
print("IRR:", nfin.irr(cf))

// Price series: returns + RSI + Bollinger bands
let prices = [100.0, 101.5, 99.0, 102.0, 104.0, 103.0, 105.5, 107.0, 106.0, 108.0,
              110.0, 109.0, 111.0, 113.0, 112.0, 114.0, 116.0, 115.0, 117.0, 119.0]
let rsi = nfin.rsi(prices, 14)
let bands = nfin.bbands(prices, 5, 2.0)
print("RSI last", rsi[19], "upper band", bands.upper[19])
```

Price/cash-flow inputs are plain number arrays or packed `float_array` values. Structured results use objects (`{period, payment, ...}`, `{macd, signal, histogram}`, etc.).

## Time value of money

All TVM functions accept optional `when`: `0` = payment at **end** of period (default), `1` = **beginning**.

| Method | Description |
|--------|-------------|
| `nfin.fv(rate, nper, pmt, pv?, when?)` | Future value. |
| `nfin.pv(rate, nper, pmt, fv?, when?)` | Present value. |
| `nfin.pmt(rate, nper, pv, fv?, when?)` | Periodic payment. |
| `nfin.ipmt(rate, per, nper, pv, fv?, when?)` | Interest portion for period `per` (1-based). |
| `nfin.ppmt(rate, per, nper, pv, fv?, when?)` | Principal portion for period `per`. |
| `nfin.nper(rate, pmt, pv, fv?, when?)` | Number of periods. |
| `nfin.rate(nper, pmt, pv, fv?, when?, guess?)` | Rate per period (Newton–Raphson, default guess `0.1`). |

## Cash flows

| Method | Description |
|--------|-------------|
| `nfin.npv(rate, values)` | Net present value; first flow at t=0. |
| `nfin.irr(values, guess?)` | Internal rate of return. |
| `nfin.mirr(values, finance_rate, reinvest_rate)` | Modified IRR. |

## Amortization

| Method | Description |
|--------|-------------|
| `nfin.amortization(rate, nper, pv, when?)` | Full schedule → array of `{period, payment, interest, principal, balance}`. |

## Returns & risk

| Method | Description |
|--------|-------------|
| `nfin.simple_return(prices)` | Period simple returns (length n−1). |
| `nfin.log_return(prices)` | Log returns (positive prices required). |
| `nfin.cumulative_return(returns)` | Cumulative compounded return. |
| `nfin.cagr(start, end, periods)` | Compound annual growth rate. |
| `nfin.sharpe(returns, risk_free?, periods_per_year?)` | Sharpe ratio (default rf=0, 252 periods/year). |
| `nfin.max_drawdown(prices)` | `{max_drawdown, peak_idx, trough_idx}`. |

## Technical indicators

Warmup periods are padded with NaN (same convention as TA-Lib).

| Method | Description |
|--------|-------------|
| `nfin.sma(values, period)` | Simple moving average. |
| `nfin.ema(values, period)` | Exponential moving average. |
| `nfin.rsi(values, period?)` | Relative strength index (default period 14). |
| `nfin.macd(values, fast?, slow?, signal?)` | `{macd, signal, histogram}` (defaults 12/26/9). |
| `nfin.bbands(values, period?, nbdev?)` | Bollinger bands `{upper, middle, lower}` (defaults 20, 2σ). |
| `nfin.atr(high, low, close, period?)` | Average true range (default 14). |
| `nfin.stoch(high, low, close, k_period?, d_period?)` | Stochastic `{k, d}` (defaults 14/3). |

## Errors

Catchable `nfin_error` values (use `ntest.is_error` / `try`):

| Code | Meaning |
|------|---------|
| 4110 | Wrong argument count. |
| 4111 | General domain error. |
| 4112 | Type mismatch. |
| 4113 | Invalid parameter. |
| 4114 | Empty input / length mismatch. |
| 4115 | Solver non-convergence (rate / IRR). |

## Deferred / not in 0.1.0

- Bond pricing, duration/convexity, options Greeks.
- Full TA-Lib catalog (ADX, CCI, OBV, candlestick patterns, etc.).
- Portfolio optimization, factor models (see `noptim` / `nframe`).
- Calendar-day / business-day accrual (see `ncal`).

## Notes

- TVM signs follow numpy-financial: payments are typically negative, principal positive.
- For dataframe-style workflows, combine with `nframe`; for arbitrary-precision money, use `ndecimal`.
- Run tests: `niao run tests/nfin.niao`
