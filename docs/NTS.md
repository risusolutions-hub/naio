# NTS — Time Series for Niao

`nts` replaces core **statsmodels.tsa** functionality with a std-only native library
(`crates/niao_ts`). Depends on `nnum` (FFT/linalg), `nstats` (distributions/CIs),
`noptim` (MLE/SSE fitting), and `nframe` (Series I/O).

Import:

```niao
import "nts"
```

## Diagnostics

| Function | Description |
|----------|-------------|
| `nts.acf(x, nlags?)` | Autocorrelation function (FFT-based) |
| `nts.pacf(x, nlags?)` | Partial autocorrelation (Durbin–Levinson) |
| `nts.diff(x, periods?)` | Differencing |
| `nts.seasonal_diff(x, seasonal, periods?)` | Seasonal differencing |
| `nts.lagmat(x, maxlag, trim?)` | Lag matrix |
| `nts.adfuller(x, maxlag?)` | Augmented Dickey–Fuller test → `{statistic, pvalue, lags}` |
| `nts.kpss(x, lags?)` | KPSS stationarity test |
| `nts.ljungbox(x, lags)` | Ljung–Box portmanteau test |

## Decomposition

| Function | Description |
|----------|-------------|
| `nts.seasonal_decompose(x, period, multiplicative?)` | Classical additive/multiplicative decomposition → `{observed, trend, seasonal, resid}` |

## ARIMA / SARIMA

| API | Description |
|-----|-------------|
| `nts.arima(p, d, q)` | Create ARIMA model handle |
| `nts.sarima(p,d,q,P,D,Q,s)` | Seasonal ARIMA |
| `model.fit(y)` | Fit via Yule–Walker (AR) or MLE (ARMA/ARIMA) |
| `model.forecast(h, alpha?)` | Point forecasts + prediction intervals |
| `model.predict(start, end)` | In-sample / extended predictions |
| `model.summary()` | Text summary with AIC/BIC |
| `nts.auto_arima(y, max_p, max_d, max_q, seasonal?)` | Grid search by AICc |

## Exponential Smoothing

| API | Description |
|-----|-------------|
| `nts.ses(y, alpha?)` | Simple exponential smoothing |
| `nts.holt()` / `nts.holt_winters(period, mult?)` | Trend / seasonal smoothing |
| `model.fit(y)` | Fit smoothing parameters (SSE minimization) |
| `model.forecast(h)` | Point forecasts |

## Model Selection & Backtesting

| Function | Description |
|----------|-------------|
| `nts.backtest(y, order, train_size, horizon)` | Rolling-origin evaluation → MAE/RMSE/MAPE |

## Error codes (4070–4079)

| Code | Name | When |
|------|------|------|
| 4070 | `NTS_ARITY` | Wrong argument count |
| 4071 | `NTS_ERROR` | General error |
| 4072 | `NTS_TYPE` | Type mismatch |
| 4073 | `NTS_NOT_FITTED` | `predict`/`forecast` before `fit` |
| 4074 | `NTS_NON_STATIONARY` | Near unit-root / invertibility failure |
| 4075 | `NTS_NON_CONVERGENCE` | Optimizer did not converge |
| 4076 | `NTS_DOMAIN` | Invalid parameters / empty series |
| 4077 | `NTS_SHAPE` | Length / shape mismatch |

## Example

See `examples/nts_demo.niao`.
