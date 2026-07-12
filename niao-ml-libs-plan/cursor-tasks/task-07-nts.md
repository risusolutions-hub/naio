# Task 07 — nts: statsmodels.tsa / prophet (crate `niao_ts`)
Wave 2 (needs nnum, nstats, noptim). Read `../MASTER_PLAN.md` + `../specs/niao_ts__nts.md`. Error block **4070–4079**.
Depends on: `nnum`, `nstats`, `noptim`, `nframe`.

## Build (`crates/niao_ts`, zero new deps)
- Diagnostics: ACF/PACF (autocovariance via nnum.fft; PACF by Durbin–Levinson), adfuller, KPSS, Ljung–Box; diff/seasonal-diff/lagmat.
- Decomposition: classical additive/multiplicative seasonal_decompose (STL = v2).
- AR (Yule–Walker/Levinson + OLS) → ARMA(p,q) → ARIMA(p,d,q) → SARIMA(p,d,q)(P,D,Q,s) by MLE: build the log-likelihood
  (innovations/Kalman state-space), optimize with noptim L-BFGS, enforce stationarity/invertibility via parameter transforms.
- Exponential smoothing: SES, Holt, Holt-Winters (add + mult seasonality); fit smoothing params by SSE min (noptim, bounds [0,1]).
- forecast(h): point + prediction intervals (residual σ² recursion, nstats Normal ppf); predict(start,end); fitted+residuals.
- Model selection: AIC/BIC/AICc; small auto_arima grid over orders by IC. Metrics/backtest (rolling origin) via neval.
- Keep the likelihood allocation-free inside (optimizer calls it many times); #[inline] Levinson/HW/Kalman kernels.

## Wire up
- `niao_libs/nts/` wrapper + builtins; `docs/NTS.md`; `examples/nts_demo.niao` (fit ARIMA, forecast horizon with intervals).

## Acceptance
- ACF/PACF vs statsmodels 1e-8; ADF/KPSS/Ljung–Box stat+p vs statsmodels; AR Yule–Walker coeffs 1e-6;
  ARIMA on airline/sunspots: params+AIC vs statsmodels within 1e-3/1e-2, forecast within 1e-3; Holt-Winters forecast in tolerance;
  auto_arima picks expected order; interval coverage sanity.
- non-convergence→4075, non-stationary→4074, predict-before-fit→4073. Validate AR→MA→ARMA→ARIMA→seasonal each step.
- `benchmarks/benchmark_nts.py` vs statsmodels. `cargo test -p niao_ts` green.

See `../cursor-rules.md`.
