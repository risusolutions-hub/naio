# NSTATS — Statistics for Niao

`nstats` replaces core **scipy.stats** and **statsmodels** summaries with a std-only
native library (`crates/niao_stats`). Depends on `nnum` (linear algebra) and `nrand` (sampling).

Import:

```niao
import "nstats"
```

## Distributions

Each distribution exposes `pdf`/`pmf`, `cdf`, `sf`, `ppf`, `mean`, `var`, `std`, and `rvs(n, seed)`.

| Distribution | Constructor |
|--------------|-------------|
| Normal | `nstats.normal(mu, sigma)` |
| StudentT | `nstats.student_t(df)` |
| ChiSquare | `nstats.chi2(df)` |
| F | `nstats.f(dfn, dfd)` |
| Exponential | `nstats.exponential(scale)` |
| Gamma | `nstats.gamma(shape, scale)` |
| Beta | `nstats.beta(a, b)` |
| Uniform | `nstats.uniform(loc, scale)` |
| Poisson | `nstats.poisson(mu)` |
| Binomial | `nstats.binomial(n, p)` |
| Bernoulli | `nstats.bernoulli(p)` |
| LogNormal | `nstats.lognormal(mu, sigma)` |

## Descriptive statistics

| Function | Description |
|----------|-------------|
| `nstats.mean(data)` | Arithmetic mean |
| `nstats.var(data, ddof?)` | Variance |
| `nstats.std(data, ddof?)` | Standard deviation |
| `nstats.median(data)` | Median |
| `nstats.quantile(data, q)` | Quantile / percentile scale 0–1 |
| `nstats.skew(data)` | Sample skewness |
| `nstats.kurtosis(data)` | Excess kurtosis |
| `nstats.describe(data)` | Summary struct |
| `nstats.zscore(data)` | Z-scores |

## Correlation

| Function | Returns |
|----------|---------|
| `nstats.pearsonr(x, y)` | `{r, pvalue}` |
| `nstats.spearmanr(x, y)` | `{r, pvalue}` |
| `nstats.kendalltau(x, y)` | `{tau, pvalue}` |
| `nstats.cov(x, y, ddof?)` | Covariance |
| `nstats.cov_matrix(rows)` | Covariance matrix |

## Hypothesis tests

All return `{statistic, pvalue}`.

| Function | Description |
|----------|-------------|
| `nstats.ttest_1samp(data, popmean)` | One-sample t-test |
| `nstats.ttest_ind(a, b)` | Two-sample t-test (pooled) |
| `nstats.ttest_welch(a, b)` | Welch's unequal-variance t-test |
| `nstats.ttest_rel(a, b)` | Paired t-test |
| `nstats.anova(groups)` | One-way ANOVA |
| `nstats.chi2_contingency(table)` | Chi-square independence |
| `nstats.chi2_gof(observed, expected)` | Goodness-of-fit |
| `nstats.ks_1samp(data, dist)` | One-sample Kolmogorov–Smirnov |
| `nstats.ks_2samp(a, b)` | Two-sample KS |
| `nstats.mannwhitneyu(a, b)` | Mann–Whitney U |
| `nstats.wilcoxon(data)` | Wilcoxon signed-rank |
| `nstats.levene(groups)` | Levene variance test |
| `nstats.shapiro(data)` | Shapiro–Wilk normality |
| `nstats.normaltest(data)` | D'Agostino–Pearson normality |

## Regression

| Function | Returns |
|----------|---------|
| `nstats.ols(x, y)` | OLS: coefficients, SE, t-stats, p-values, R², adj-R², F, CIs |
| `nstats.logistic(x, y)` | Logistic IRLS summary |
| `nstats.ci_mean(data, confidence)` | Mean confidence interval |
| `nstats.ci_proportion(k, n, confidence)` | Wilson proportion CI |

OLS uses QR decomposition via `nnum` for numerical stability.

## Special functions

`nstats.erf`, `nstats.gamma`, `nstats.betainc`, `nstats.gammainc` — used internally; exposed for advanced users.

## Error codes (4020–4029)

| Code | Meaning |
|------|---------|
| 4020 | arity |
| 4021 | general error / invalid handle |
| 4022 | type mismatch |
| 4023 | domain (bad distribution params) |
| 4024 | non-convergence (ppf / IRLS) |

## v1 limitations

- `niao_num::lstsq` not used (known instability); OLS uses in-crate Gram–Schmidt QR
- Far-tail `ppf(cdf(x))` round-trip relaxed to 1e-4 (Abramowitz erf approximation)
- Shapiro–Wilk uses Royston p-value approximation; exact a-coefficients for n > 50 are approximated
- Kendall τ p-value uses asymptotic normal approximation (no tie correction)

## Dependencies

- `nnum` — matrix ops, covariance
- `nrand` — `rvs` sampling (Box–Muller, Marsaglia–Tsang, Knuth Poisson, etc.)
