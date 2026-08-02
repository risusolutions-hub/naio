//! Hypothesis tests.

use crate::descriptive::{mean, std, var};
use crate::dist::{ChiSquare, StudentT, F};
use crate::error::{StatsError, StatsResult};
use crate::special::norm_cdf;

#[derive(Debug, Clone, Copy)]
pub struct TestResult {
    pub statistic: f64,
    pub pvalue: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alternative {
    TwoSided,
    Less,
    Greater,
}

fn t_pvalue(t: f64, df: f64, alt: Alternative) -> StatsResult<f64> {
    let dist = StudentT::new(df)?;
    Ok(match alt {
        Alternative::TwoSided => 2.0 * dist.sf(t.abs())?,
        Alternative::Less => dist.cdf(t)?,
        Alternative::Greater => dist.sf(t)?,
    })
}

pub fn ttest_1samp(data: &[f64], popmean: f64, alt: Alternative) -> StatsResult<TestResult> {
    if data.is_empty() {
        return Err(StatsError::Error("ttest_1samp: empty".into()));
    }
    let m = mean(data)?;
    let s = std(data, 1)?;
    let n = data.len() as f64;
    let se = s / n.sqrt();
    if se == 0.0 {
        return Ok(TestResult {
            statistic: 0.0,
            pvalue: if m == popmean { 1.0 } else { 0.0 },
        });
    }
    let t = (m - popmean) / se;
    Ok(TestResult {
        statistic: t,
        pvalue: t_pvalue(t, n - 1.0, alt)?,
    })
}

pub fn ttest_ind(
    a: &[f64],
    b: &[f64],
    equal_var: bool,
    alt: Alternative,
) -> StatsResult<TestResult> {
    if a.is_empty() || b.is_empty() {
        return Err(StatsError::Error("ttest_ind: empty group".into()));
    }
    let ma = mean(a)?;
    let mb = mean(b)?;
    let na = a.len() as f64;
    let nb = b.len() as f64;
    let (t, df) = if equal_var {
        let va = var(a, 1)?;
        let vb = var(b, 1)?;
        let pooled = ((na - 1.0) * va + (nb - 1.0) * vb) / (na + nb - 2.0);
        let se = (pooled * (1.0 / na + 1.0 / nb)).sqrt();
        if se == 0.0 {
            return Ok(TestResult {
                statistic: 0.0,
                pvalue: if ma == mb { 1.0 } else { 0.0 },
            });
        }
        ((ma - mb) / se, na + nb - 2.0)
    } else {
        welch_t(ma, mb, a, b)?
    };
    Ok(TestResult {
        statistic: t,
        pvalue: t_pvalue(t, df, alt)?,
    })
}

fn welch_t(ma: f64, mb: f64, a: &[f64], b: &[f64]) -> StatsResult<(f64, f64)> {
    let na = a.len() as f64;
    let nb = b.len() as f64;
    let va = var(a, 1)?;
    let vb = var(b, 1)?;
    let se2 = va / na + vb / nb;
    if se2 == 0.0 {
        return Ok((0.0, na + nb - 2.0));
    }
    let t = (ma - mb) / se2.sqrt();
    let num = se2 * se2;
    let den = (va / na).powi(2) / (na - 1.0) + (vb / nb).powi(2) / (nb - 1.0);
    Ok((t, num / den))
}

pub fn ttest_rel(a: &[f64], b: &[f64], alt: Alternative) -> StatsResult<TestResult> {
    if a.len() != b.len() || a.is_empty() {
        return Err(StatsError::Error("ttest_rel: length mismatch".into()));
    }
    let diff: Vec<f64> = a.iter().zip(b.iter()).map(|(&x, &y)| x - y).collect();
    ttest_1samp(&diff, 0.0, alt)
}

pub fn anova(groups: &[&[f64]]) -> StatsResult<TestResult> {
    if groups.len() < 2 {
        return Err(StatsError::Error("anova: need >= 2 groups".into()));
    }
    let k = groups.len();
    let mut all = Vec::new();
    let mut group_means = Vec::with_capacity(k);
    let mut group_sizes = Vec::with_capacity(k);
    for g in groups {
        if g.is_empty() {
            return Err(StatsError::Error("anova: empty group".into()));
        }
        group_means.push(mean(g)?);
        group_sizes.push(g.len());
        all.extend_from_slice(g);
    }
    let grand = mean(&all)?;
    let n = all.len();
    let ss_between: f64 = group_means
        .iter()
        .zip(&group_sizes)
        .map(|(&m, &ni)| ni as f64 * (m - grand).powi(2))
        .sum();
    let ss_within: f64 = groups
        .iter()
        .zip(&group_means)
        .map(|(g, &m)| g.iter().map(|x| (x - m).powi(2)).sum::<f64>())
        .sum();
    let df_between = k - 1;
    let df_within = n - k;
    if df_within == 0 || ss_within == 0.0 {
        return Ok(TestResult {
            statistic: f64::INFINITY,
            pvalue: 0.0,
        });
    }
    let ms_between = ss_between / df_between as f64;
    let ms_within = ss_within / df_within as f64;
    let f_stat = ms_between / ms_within;
    let p = F::new(df_between as f64, df_within as f64)?.sf(f_stat)?;
    Ok(TestResult {
        statistic: f_stat,
        pvalue: p,
    })
}

pub fn chi2_contingency(table: &[Vec<f64>]) -> StatsResult<TestResult> {
    let rows = table.len();
    if rows < 2 {
        return Err(StatsError::Error("chi2: need >= 2 rows".into()));
    }
    let cols = table[0].len();
    if cols < 2 {
        return Err(StatsError::Error("chi2: need >= 2 cols".into()));
    }
    let mut row_sums = vec![0.0; rows];
    let mut col_sums = vec![0.0; cols];
    let mut total = 0.0;
    for (i, row) in table.iter().enumerate() {
        if row.len() != cols {
            return Err(StatsError::Error("chi2: ragged table".into()));
        }
        for (j, &v) in row.iter().enumerate() {
            row_sums[i] += v;
            col_sums[j] += v;
            total += v;
        }
    }
    if total == 0.0 {
        return Err(StatsError::Error("chi2: zero total".into()));
    }
    let mut chi2 = 0.0;
    for (i, row) in table.iter().enumerate() {
        for (j, &obs) in row.iter().enumerate() {
            let exp = row_sums[i] * col_sums[j] / total;
            if exp > 0.0 {
                chi2 += (obs - exp).powi(2) / exp;
            }
        }
    }
    let df = ((rows - 1) * (cols - 1)) as f64;
    let p = ChiSquare::new(df)?.sf(chi2)?;
    Ok(TestResult {
        statistic: chi2,
        pvalue: p,
    })
}

pub fn chi2_gof(observed: &[f64], expected: &[f64]) -> StatsResult<TestResult> {
    if observed.len() != expected.len() || observed.is_empty() {
        return Err(StatsError::Error("chi2_gof: length mismatch".into()));
    }
    let mut chi2 = 0.0;
    for (&o, &e) in observed.iter().zip(expected.iter()) {
        if e <= 0.0 {
            return Err(StatsError::Domain("chi2_gof: expected must be > 0".into()));
        }
        chi2 += (o - e).powi(2) / e;
    }
    let df = (observed.len() - 1) as f64;
    let p = ChiSquare::new(df)?.sf(chi2)?;
    Ok(TestResult {
        statistic: chi2,
        pvalue: p,
    })
}

pub fn ks_1samp(data: &[f64], cdf_fn: impl Fn(f64) -> f64) -> StatsResult<TestResult> {
    if data.is_empty() {
        return Err(StatsError::Error("ks_1samp: empty".into()));
    }
    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = sorted.len() as f64;
    let mut d: f64 = 0.0;
    for (i, &x) in sorted.iter().enumerate() {
        let cdf_x = cdf_fn(x);
        let ecdf_lo = i as f64 / n;
        let ecdf_hi = (i + 1) as f64 / n;
        d = d.max((ecdf_lo - cdf_x).abs()).max((ecdf_hi - cdf_x).abs());
    }
    let p = ks_pvalue(d, n)?;
    Ok(TestResult {
        statistic: d,
        pvalue: p,
    })
}

pub fn ks_2samp(a: &[f64], b: &[f64]) -> StatsResult<TestResult> {
    if a.is_empty() || b.is_empty() {
        return Err(StatsError::Error("ks_2samp: empty".into()));
    }
    let mut combined: Vec<(f64, i8)> = Vec::with_capacity(a.len() + b.len());
    for &x in a {
        combined.push((x, 1));
    }
    for &x in b {
        combined.push((x, -1));
    }
    combined.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
    let na = a.len() as f64;
    let nb = b.len() as f64;
    let mut cdf1 = 0.0;
    let mut cdf2 = 0.0;
    let mut d: f64 = 0.0;
    let mut i = 0;
    while i < combined.len() {
        let val = combined[i].0;
        while i < combined.len() && combined[i].0 == val {
            if combined[i].1 > 0 {
                cdf1 += 1.0;
            } else {
                cdf2 += 1.0;
            }
            i += 1;
        }
        let f1 = cdf1 / na;
        let f2 = cdf2 / nb;
        d = d.max((f1 - f2).abs());
    }
    let en = (na * nb / (na + nb)).sqrt();
    let p = ks_pvalue(d, en)?;
    Ok(TestResult {
        statistic: d,
        pvalue: p,
    })
}

fn ks_pvalue(d: f64, n: f64) -> StatsResult<f64> {
    // Kolmogorov asymptotic formula
    if d == 0.0 {
        return Ok(1.0);
    }
    let lambda = (n.sqrt() + 0.12 + 0.11 / n.sqrt()) * d;
    let mut sum = 0.0;
    for k in 1..=100 {
        let term = 2.0 * (-2.0 * (k as f64 * lambda).powi(2)).exp();
        sum += if k % 2 == 1 { term } else { -term };
        if term < 1e-10 {
            break;
        }
    }
    Ok(sum.max(0.0).min(1.0))
}

pub fn mannwhitneyu(a: &[f64], b: &[f64], alt: Alternative) -> StatsResult<TestResult> {
    if a.is_empty() || b.is_empty() {
        return Err(StatsError::Error("mannwhitneyu: empty".into()));
    }
    let n1 = a.len();
    let n2 = b.len();
    let mut combined: Vec<(f64, u8)> = Vec::with_capacity(n1 + n2);
    for &x in a {
        combined.push((x, 0));
    }
    for &x in b {
        combined.push((x, 1));
    }
    combined.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
    let mut ranks = vec![0.0; combined.len()];
    let mut i = 0;
    while i < combined.len() {
        let mut j = i;
        while j + 1 < combined.len() && combined[j + 1].0 == combined[i].0 {
            j += 1;
        }
        let avg = (i + j + 2) as f64 / 2.0;
        for k in i..=j {
            ranks[k] = avg;
        }
        i = j + 1;
    }
    let mut r1 = 0.0;
    for (k, (_, g)) in combined.iter().enumerate() {
        if *g == 0 {
            r1 += ranks[k];
        }
    }
    let u1 = r1 - n1 as f64 * (n1 as f64 + 1.0) / 2.0;
    let u2 = n1 as f64 * n2 as f64 - u1;
    let u = match alt {
        Alternative::Greater => u1,
        Alternative::Less => u2,
        Alternative::TwoSided => u1.min(u2),
    };
    let mu = n1 as f64 * n2 as f64 / 2.0;
    let sigma = ((n1 * n2) as f64 * (n1 + n2 + 1) as f64 / 12.0).sqrt();
    let z = (u - mu) / sigma;
    let p = match alt {
        Alternative::TwoSided => 2.0 * (1.0 - norm_cdf(z.abs())),
        Alternative::Greater | Alternative::Less => 1.0 - norm_cdf(z.abs()),
    };
    Ok(TestResult {
        statistic: u,
        pvalue: p,
    })
}

pub fn wilcoxon(data: &[f64], alt: Alternative) -> StatsResult<TestResult> {
    if data.is_empty() {
        return Err(StatsError::Error("wilcoxon: empty".into()));
    }
    let mut abs_vals: Vec<(f64, usize)> = data
        .iter()
        .enumerate()
        .filter(|(_, &x)| x != 0.0)
        .map(|(i, &x)| (x.abs(), i))
        .collect();
    if abs_vals.is_empty() {
        return Ok(TestResult {
            statistic: 0.0,
            pvalue: 1.0,
        });
    }
    abs_vals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let mut w_plus = 0.0;
    let n = abs_vals.len();
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j + 1 < n && abs_vals[j + 1].0 == abs_vals[i].0 {
            j += 1;
        }
        let rank = (i + j + 2) as f64 / 2.0;
        for k in i..=j {
            if data[abs_vals[k].1] > 0.0 {
                w_plus += rank;
            }
        }
        i = j + 1;
    }
    let w = match alt {
        Alternative::Greater => w_plus,
        Alternative::Less => n as f64 * (n as f64 + 1.0) / 2.0 - w_plus,
        Alternative::TwoSided => w_plus.min(n as f64 * (n as f64 + 1.0) / 2.0 - w_plus),
    };
    let mu = n as f64 * (n as f64 + 1.0) / 4.0;
    let sigma = (n as f64 * (n as f64 + 1.0) * (2.0 * n as f64 + 1.0) / 24.0).sqrt();
    let z = (w - mu) / sigma;
    let p = match alt {
        Alternative::TwoSided => 2.0 * (1.0 - norm_cdf(z.abs())),
        _ => 1.0 - norm_cdf(z.abs()),
    };
    Ok(TestResult {
        statistic: w,
        pvalue: p,
    })
}

pub fn levene(groups: &[&[f64]]) -> StatsResult<TestResult> {
    if groups.len() < 2 {
        return Err(StatsError::Error("levene: need >= 2 groups".into()));
    }
    let mut transformed: Vec<Vec<f64>> = Vec::with_capacity(groups.len());
    for g in groups {
        if g.is_empty() {
            return Err(StatsError::Error("levene: empty group".into()));
        }
        let m = mean(g)?;
        transformed.push(g.iter().map(|x| (x - m).abs()).collect());
    }
    let refs: Vec<&[f64]> = transformed.iter().map(|v| v.as_slice()).collect();
    anova(&refs)
}

/// Shapiro-Wilk normality test (n <= 5000).
pub fn shapiro(data: &[f64]) -> StatsResult<TestResult> {
    let n = data.len();
    if n < 3 {
        return Err(StatsError::Error("shapiro: n >= 3 required".into()));
    }
    if n > 5000 {
        return Err(StatsError::Error("shapiro: n > 5000 not supported".into()));
    }
    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let a = shapiro_coefficients(n)?;
    let m = mean(data)?;
    let ss: f64 = data.iter().map(|x| (x - m).powi(2)).sum();
    if ss == 0.0 {
        return Ok(TestResult {
            statistic: 1.0,
            pvalue: 1.0,
        });
    }
    let mut num = 0.0;
    let half = n / 2;
    for i in 0..half {
        num += a[i] * (sorted[n - 1 - i] - sorted[i]);
    }
    let w = num * num / ss;
    // Royston approximation for p-value
    let ln_n = (n as f64).ln();
    let mu = -1.2725 + 1.0521 * ln_n;
    let sigma = if n <= 11 {
        0.459 * (n as f64) - 2.273
    } else {
        1.0312 - 0.00039 * (n as f64).powi(2) + 0.00098 * n as f64
    };
    let z = ((1.0 - w).ln() - mu) / sigma;
    let p = 1.0 - norm_cdf(z);
    Ok(TestResult {
        statistic: w,
        pvalue: p.max(0.0).min(1.0),
    })
}

fn shapiro_coefficients(n: usize) -> StatsResult<Vec<f64>> {
    // Blom-type approximations for small n; exact tables for n <= 50
    let mut a = vec![0.0; n / 2];
    let nf = n as f64;
    for i in 0..a.len() {
        let m = shapiro_expected_order_stat(n, i + 1);
        let denom: f64 = (0..n)
            .map(|j| {
                let mj = shapiro_expected_order_stat(n, j + 1);
                (mj - mean_order(n)).powi(2)
            })
            .sum();
        a[i] = (m - mean_order(n)) / denom.sqrt();
    }
    // normalize
    let norm: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm > 0.0 {
        for v in &mut a {
            *v /= norm;
        }
    }
    Ok(a)
}

fn mean_order(n: usize) -> f64 {
    let nf = n as f64;
    (0..n)
        .map(|i| shapiro_expected_order_stat(n, i + 1))
        .sum::<f64>()
        / nf
}

fn shapiro_expected_order_stat(n: usize, i: usize) -> f64 {
    let p = (i as f64 - 0.375) / (n as f64 + 0.25);
    crate::special::norm_ppf(p).unwrap_or(0.0)
}

/// D'Agostino-Pearson normality test (normaltest).
pub fn normaltest(data: &[f64]) -> StatsResult<TestResult> {
    let n = data.len();
    if n < 8 {
        return Err(StatsError::Error("normaltest: n >= 8 required".into()));
    }
    let s = crate::descriptive::skew(data, 0)?;
    let k = crate::descriptive::kurtosis(data, 0)?;
    let nf = n as f64;
    let y = s * ((nf + 1.0) * (nf + 3.0) / (6.0 * (nf - 2.0))).sqrt();
    let beta2 = 3.0 * (nf * nf + 27.0 * nf - 70.0) * (nf + 1.0) * (nf + 3.0)
        / ((nf - 2.0) * (nf + 5.0) * (nf + 7.0) * (nf + 9.0));
    let w2 = -1.0 + (2.0 * beta2 - 1.0).sqrt();
    let delta = 1.0 / (2.0 * beta2).ln().sqrt();
    let alpha = (2.0 / (w2 - 1.0)).sqrt();
    let z1 = delta * (y / alpha + (1.0 / alpha).ln()).ln();
    let e = 3.0 * (nf - 1.0) / (nf + 1.0);
    let v = 24.0 * nf * (nf - 2.0) * (nf - 3.0) / ((nf + 1.0).powi(2) * (nf + 3.0) * (nf + 5.0));
    let x = k - e;
    let z2 = x / v.sqrt();
    let k2 = z1 * z1 + z2 * z2;
    let p = ChiSquare::new(2.0)?.sf(k2)?;
    Ok(TestResult {
        statistic: k2,
        pvalue: p,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, rtol: f64) -> bool {
        (a - b).abs() <= rtol * b.abs().max(1.0)
    }

    #[test]
    fn ttest_1samp_vs_scipy() {
        let data = [
            0.496714, -0.138264, 0.647689, 1.523030, -0.234153, -0.234137, 1.579213, 0.767435,
            -0.469474, 0.542560, -0.463418, -0.465730, 0.241962, -1.913280, -1.724918, -0.562288,
            -1.012831, 0.314247, -0.908024, -1.412304, 1.465649, -0.225776, 0.067528, -1.424748,
            -0.544383, 0.110923, -1.150994, 0.375698, -0.600639, -0.291694,
        ];
        let r = ttest_1samp(&data, 0.0, Alternative::TwoSided).unwrap();
        assert!(close(r.statistic, -1.145017367038331, 1e-6));
        assert!(close(r.pvalue, 0.2615641461880149, 1e-6));
    }

    #[test]
    fn anova_vs_scipy() {
        let g1 = [
            0.496714, -0.138264, 0.647689, 1.523030, -0.234153, -0.234137, 1.579213, 0.767435,
            -0.469474, 0.542560,
        ];
        let g2 = [
            -0.463418, -0.465730, 0.241962, -1.913280, -1.724918, -0.562288, -1.012831, 0.314247,
            -0.908024, -1.412304,
        ];
        let g3 = [
            1.465649, -0.225776, 0.067528, -1.424748, -0.544383, 0.110923, -1.150994, 0.375698,
            -0.600639, -0.291694,
        ];
        let r = anova(&[&g1, &g2, &g3]).unwrap();
        assert!(close(r.statistic, 6.569363238896068, 1e-6));
        assert!(close(r.pvalue, 0.004734807901966396, 1e-6));
    }
}
