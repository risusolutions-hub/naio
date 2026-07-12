# Library spec: `nts`  →  crate `niao_ts`

| | |
|---|---|
| Category | Time series |
| Replaces (Python) | `statsmodels.tsa` + `prophet` (core) |
| Rust reference | `augurs` |
| Target Niao crate | `crates/niao_ts` |
| Niao import name | `nts` |
| Difficulty | 4/5 — Very Hard |
| Wave | 2 (needs nnum, nstats, noptim) |
| Depends on Niao libs | `nnum`, `nstats`, `noptim`, `nframe` |
| Error block | 4070–4079 |

## Goal
Classical time-series analysis and forecasting: decomposition, autocorrelation, AR/ARIMA/SARIMA,
exponential smoothing (Holt-Winters), and forecast intervals. **Zero external deps** — MLE via `noptim`,
linear algebra via `nnum`, distributions/CIs via `nstats`.

## Scope (v1)
- **Descriptive / diagnostics:** ACF, PACF, `adfuller` (Augmented Dickey–Fuller stationarity test),
  KPSS, Ljung–Box (white-noise test), differencing/`diff`/seasonal-diff, `lagmat`.
- **Decomposition:** classical additive/multiplicative (`seasonal_decompose`), STL (loess-based) — STL v2 if time-boxed.
- **AR / ARMA / ARIMA:** `AR(p)` (Yule–Walker + OLS), `ARMA(p,q)`, `ARIMA(p,d,q)` via conditional/exact MLE
  (optimize the log-likelihood with `noptim` L-BFGS), `SARIMA(p,d,q)(P,D,Q,s)`.
- **Exponential smoothing:** SES, Holt (trend), **Holt-Winters** (additive + multiplicative seasonality),
  parameter fitting by SSE minimization (noptim).
- **Forecasting:** `forecast(h)` → point forecasts + prediction intervals (from residual variance / model σ²);
  `predict(start, end)`; in-sample fitted values + residuals.
- **Model selection:** AIC/BIC/AICc; a small `auto_arima` grid over (p,d,q)(P,D,Q) by information criterion.
- **Metrics:** MAE/RMSE/MAPE/sMAPE via `neval`; backtesting with rolling-origin evaluation.

## Implementation blueprint
- ACF/PACF via FFT-based autocovariance (reuse `nnum.fft`) — fast and standard; PACF by Durbin–Levinson.
- AR: Yule–Walker (Levinson recursion) for a fast estimate; refine ARMA/ARIMA by MLE — build the (S)ARIMA
  log-likelihood (innovations algorithm or Kalman-filter state-space form) and hand it to `noptim` L-BFGS with
  numerical or analytic gradients. Enforce stationarity/invertibility by parameter transforms.
- Holt-Winters: recursive level/trend/season updates; fit smoothing params by minimizing SSE (noptim, bounded [0,1]).
- Prediction intervals from the model's residual σ² and forecast-error variance recursion; use `nstats` Normal ppf.
- Differencing/integration handled around the stationary core (the "I" in ARIMA).

### Performance rules
- Reuse the FFT plan/buffers for ACF; avoid re-allocating the residual/state buffers each likelihood eval (the
  optimizer calls the likelihood many times — keep it allocation-free inside).
- `#[inline]` the recursion kernels (Levinson, HW update, Kalman step).

## Public API surface
`acf/pacf/adfuller/ljungbox`, `seasonal_decompose`, `AR/ARIMA/SARIMA` (`fit`, `forecast`, `predict`, `summary`,
`aic/bic`), `ExponentialSmoothing`/`Holt`/`HoltWinters`, `auto_arima`, `backtest`. Same fit/predict shape as `nlearn`.
Expose to Niao via `niao_libs/nts/` + builtins.

## Performance target
Correctness within tolerance is the gate. Fitted parameters and forecasts vs statsmodels on fixtures within
`rtol=1e-3` (MLE optima can differ slightly by optimizer); AIC/BIC within `1e-2`.

## Tests required
- ACF/PACF vs statsmodels fixtures on a seeded AR(2)/MA(1) series, `rtol=1e-8`.
- ADF/KPSS/Ljung–Box statistics + p-values vs statsmodels fixtures.
- AR(p) Yule–Walker coefficients vs statsmodels, `rtol=1e-6`.
- ARIMA(p,d,q) fit on the classic airline/sunspots fixture: params + AIC vs statsmodels within `1e-3`/`1e-2`;
  `forecast(h)` point forecasts within `rtol=1e-3`.
- Holt-Winters forecast on the airline dataset vs statsmodels within tolerance.
- `auto_arima` selects the expected order on a fixture generated from a known process.
- Forecast intervals: coverage sanity + width vs statsmodels on a fixture.
- Degenerate: fit fails to converge → 4075; non-stationary/near-unit-root warning path → 4074; predict before fit → 4073.
- Plus: in-crate unit tests, `examples/nts_demo.niao`, `benchmarks/benchmark_nts.py` vs statsmodels.

## Risk / notes
- **ARIMA MLE is the hard part** — the state-space/innovations likelihood plus stationarity transforms are where
  bugs hide. Start with AR (Yule–Walker, closed form), then MA/ARMA, then differencing, validating each against
  statsmodels before adding seasonality.
- STL and Prophet-style Bayesian decomposition are v2; ship classical decompose + Holt-Winters + ARIMA in v1.
- Optimizer can land on a different local optimum than statsmodels — use their init as a sanity check, allow `1e-3`.

## Done criteria
- `cargo check --workspace` and `cargo test -p niao_ts` green; statsmodels fixtures pass in tolerance.
- `niao_libs/nts/` wrapper + `examples/nts_demo.niao` fits ARIMA and forecasts a horizon with intervals.
- Benchmark + notes in `REPORT.md`; `CHANGELOG.md` updated; shared-file edits reported, not applied.
