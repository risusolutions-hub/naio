# Task 02 — nstats: scipy.stats + statsmodels core (crate `niao_stats`)
Wave 1 (needs nnum). Read `../MASTER_PLAN.md` + `../specs/niao_stats__nstats.md`. Error block **4020–4029**.
Depends on: `nnum`, `nrand`.

## Build (`crates/niao_stats`, zero new deps)
- Special functions FIRST (everything depends on them): erf/erfc, gamma/lgamma, beta, regularized incomplete
  beta/gamma via continued fractions (Lentz). Test to 1e-10 vs scipy.
- Distributions (pdf/pmf, cdf, sf, ppf, mean/var/std, rvs via nrand): Normal, StudentT, ChiSquare, F, Exponential,
  Gamma, Beta, Uniform, Poisson, Binomial, Bernoulli, LogNormal. t/F/chi² cdfs via incomplete beta/gamma; ppf via Newton/bisection.
- Descriptive: mean/var/std(ddof)/median/mode/quantile/skew/kurtosis/IQR/describe/zscore. Correlation: pearson(+p), spearman, kendall, cov.
- Tests (stat+p, one/two-sided): t-test (1samp/ind/paired/Welch), one-way ANOVA, chi² (independence+GOF), KS (1&2 sample),
  Mann–Whitney, Wilcoxon, Levene, normality (KS/normaltest; Shapiro optional).
- OLS via nnum QR (coeffs, SE, t, p, R²/adjR², F, CIs); logistic summary (IRLS).

## Wire up
- `niao_libs/nstats/` wrapper + builtins; `docs/NSTATS.md`; `examples/nstats_demo.niao` (fit OLS, t-test, sample a dist).

## Acceptance
- dist pdf/cdf/ppf vs scipy fixtures rtol 1e-9; ppf(cdf(x))≈x; special fns rtol 1e-10; tests' stat+p vs scipy 1e-8;
  OLS vs statsmodels 1e-8.
- bad params→4023, non-convergent ppf→4024.
- `benchmarks/benchmark_nstats.py` vs scipy. `cargo test -p niao_stats` green.

See `../cursor-rules.md`.
