# Library spec: `nstats`  →  crate `niao_stats`

| | |
|---|---|
| Category | Statistics |
| Replaces (Python) | `scipy.stats` + `statsmodels` (core) |
| Rust reference | `statrs` |
| Target Niao crate | `crates/niao_stats` |
| Niao import name | `nstats` |
| Difficulty | 3/5 — Hard |
| Wave | 1 (needs nnum) |
| Depends on Niao libs | `nnum`, `nrand` |
| Error block | 4020–4029 |

## Goal
Probability distributions, descriptive statistics, hypothesis tests, correlation, and basic regression
summaries — the inference layer scientific users expect. **Zero external deps.** Sampling routes through
`nrand`; matrix math through `nnum`.

## Scope (v1)
- **Distributions** (each: `pdf/pmf`, `cdf`, `sf`, `ppf` (inverse cdf), `mean/var/std`, `rvs(n, seed)`):
  Normal, StudentT, ChiSquare, F, Exponential, Gamma, Beta, Uniform, Poisson, Binomial, Bernoulli, LogNormal.
- **Descriptive:** mean, var, std (ddof), median, mode, quantile/percentile, skew, kurtosis, min/max, IQR,
  `describe`, `zscore`, `trim_mean`.
- **Correlation / association:** Pearson `r` (+ p-value), Spearman ρ, Kendall τ, covariance matrix.
- **Hypothesis tests** (statistic + p-value; two/one-sided): one-sample / two-sample / paired **t-test**,
  Welch's t-test, one-way **ANOVA**, **chi-square** (independence + goodness-of-fit), **Kolmogorov–Smirnov**
  (1- and 2-sample), Mann–Whitney U, Wilcoxon signed-rank, Shapiro–Wilk (normality), Levene (variance).
- **Regression summary:** OLS (`fit` → coefficients, std errors, t-stats, p-values, R²/adj-R², F-stat, CIs);
  simple logistic regression summary (IRLS). Confidence intervals for mean/proportion/diff.
- **Special functions** (needed by the above): `erf/erfc`, `gamma/lgamma`, `beta/betainc` (regularized incomplete),
  `gammainc`. Implement to double precision with published rational/continued-fraction approximations.

## Implementation blueprint
- `ppf` via Newton/bisection on `cdf` with good starting guesses; Normal `ppf` uses Acklam/Wichura for speed.
- `betainc`/`gammainc` via continued fractions (Lentz) — these power the t/F/chi²/beta cdfs; get them right first,
  everything downstream depends on them.
- t/F/chi² cdfs expressed through the regularized incomplete beta/gamma functions.
- OLS via `nnum` QR (`lstsq`) for stability, not the normal equations. Covariance of coefficients from `(XᵀX)⁻¹σ̂²`.
- ANOVA/chi²/KS from first principles; p-values from the corresponding cdfs.

### Performance rules
- Special functions are the hot path — `#[inline]`, no allocation, converge in a bounded iteration count.
- Vectorized `pdf/cdf` over an `nnum` array reuse one buffer.

## Public API surface
`dist::{Normal, StudentT, ...}` with the method set above; `describe/quantile/skew/kurtosis`; `pearsonr/spearmanr`;
`ttest_ind/ttest_rel/ttest_1samp/anova/chi2/ks/mannwhitney/shapiro`; `ols(x, y) -> OlsResult`. Expose to Niao via
`niao_libs/nstats/` + builtins (mirror `niao_libs/nvalid`).

## Performance target
Correctness within tolerance is the gate; perf is secondary. Special functions must converge in ≤ 100 iters.

## Tests required
- Distribution `pdf/cdf/ppf` vs scipy.stats fixtures across the support, `rtol=1e-9` (looser 1e-6 in far tails).
- `ppf(cdf(x)) ≈ x` round-trip for every distribution.
- Special functions `erf/gamma/betainc/gammainc` vs scipy fixtures, `rtol=1e-10`.
- t-test / ANOVA / chi² / KS statistic **and** p-value vs scipy fixtures on seeded data, `rtol=1e-8`.
- OLS coefficients, std errors, t-stats, p-values, R² vs statsmodels fixture, `rtol=1e-8`.
- Degenerate: bad distribution params (negative variance) → 4023; non-convergent `ppf` → 4024.
- Plus: in-crate unit tests, `examples/nstats_demo.niao`, `benchmarks/benchmark_nstats.py` vs scipy.

## Risk / notes
- The special functions are the whole ballgame — invest test effort there; everything else composes from them.
- Watch numerical stability in tails; document where tolerance is relaxed.
- Shapiro–Wilk is fiddly (needs the a-coefficients); if time-boxed, ship it behind a v2 flag and keep KS/normaltest.

## Done criteria
- `cargo check --workspace` and `cargo test -p niao_stats` green; scipy/statsmodels fixtures pass in tolerance.
- `niao_libs/nstats/` wrapper + `examples/nstats_demo.niao` runs (fit an OLS, run a t-test, sample a distribution).
- Benchmark + notes in `REPORT.md`; `CHANGELOG.md` updated; shared-file edits reported, not applied.
