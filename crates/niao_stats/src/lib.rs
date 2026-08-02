//! nstats — scipy.stats + statsmodels core for Niao.
//!
//! Probability distributions, descriptive statistics, hypothesis tests,
//! correlation, and regression summaries. Zero external deps beyond niao_num/niao_rand.

pub mod correlation;
pub mod descriptive;
pub mod dist;
pub mod error;
pub mod hypothesis;
pub mod regression;
pub mod special;

pub use correlation::{cov, cov_matrix, kendalltau, pearsonr, spearmanr, CorrResult};
pub use descriptive::{
    describe, iqr, kurtosis, max_val, mean, median, min_val, mode, percentile, quantile, skew, std,
    trim_mean, var, zscore, DescribeResult,
};
pub use dist::{
    Bernoulli, Beta, Binomial, ChiSquare, Exponential, Gamma, LogNormal, Normal, Poisson, StudentT,
    Uniform, F,
};
pub use error::{
    StatsError, StatsResult, E4020_NSTATS_ARITY, E4021_NSTATS_ERROR, E4022_NSTATS_TYPE,
    E4023_NSTATS_DOMAIN, E4024_NSTATS_NON_CONVERGENCE,
};
pub use hypothesis::{
    anova, chi2_contingency, chi2_gof, ks_1samp, ks_2samp, levene, mannwhitneyu, normaltest,
    shapiro, ttest_1samp, ttest_ind, ttest_rel, wilcoxon, Alternative, TestResult,
};
pub use regression::{
    ci_diff_means, ci_mean, ci_proportion, logistic, ols, LogisticResult, OlsResult,
};
pub use special::{
    beta, betainc, erf, erfc, gamma, gammainc, lgamma, norm_cdf, norm_pdf, norm_ppf,
};

/// Welch's t-test (unequal variances).
pub fn ttest_welch(a: &[f64], b: &[f64], alt: Alternative) -> StatsResult<TestResult> {
    ttest_ind(a, b, false, alt)
}

#[cfg(test)]
mod scipy_fixtures {
    use super::*;
    use crate::correlation::pearsonr;
    use crate::hypothesis::{anova, ttest_1samp, ttest_ind, Alternative};

    fn close(a: f64, b: f64, rtol: f64) -> bool {
        (a - b).abs() <= rtol * b.abs().max(1.0) + 1e-12
    }

    #[test]
    fn special_functions_scipy() {
        assert!(close(erf(0.5), 0.5204998778130465, 1e-6));
        assert!(close(erfc(1.0), 0.1572992070502852, 1e-6));
        assert!(close(lgamma(2.5).unwrap(), 0.2846828704729192, 1e-10));
        assert!(close(beta(2.0, 3.0).unwrap(), 0.08333333333333333, 1e-10));
        assert!(close(betainc(2.0, 3.0, 0.5).unwrap(), 0.6875, 1e-10));
        assert!(close(
            gammainc(5.0, 3.0).unwrap(),
            0.1847367554762279,
            1e-10
        ));
    }

    #[test]
    fn normal_dist_scipy() {
        let n = Normal::standard();
        assert!(close(n.pdf(0.0), 0.3989422804014327, 1e-9));
        assert!(close(n.cdf(0.0), 0.5, 1e-9));
        assert!(close(n.cdf(1.0), 0.8413447460685429, 1e-7));
        for &x in &[-3.0, -1.0, 0.0, 1.0, 2.0] {
            let p = n.cdf(x);
            assert!((n.ppf(p).unwrap() - x).abs() < 1e-4, "roundtrip at {x}");
        }
    }

    #[test]
    fn hypothesis_scipy_seed42() {
        let data = [
            0.496714, -0.138264, 0.647689, 1.523030, -0.234153, -0.234137, 1.579213, 0.767435,
            -0.469474, 0.542560, -0.463418, -0.465730, 0.241962, -1.913280, -1.724918, -0.562288,
            -1.012831, 0.314247, -0.908024, -1.412304, 1.465649, -0.225776, 0.067528, -1.424748,
            -0.544383, 0.110923, -1.150994, 0.375698, -0.600639, -0.291694,
        ];
        let b = [
            -0.301707, 2.152278, 0.286503, -0.757711, 1.122545, -0.920844, 0.508864, -1.659670,
            -1.028186, 0.496861, 1.038467, 0.471368, 0.184352, -0.001104, -1.178522, -0.419844,
            -0.160639, 1.357122, 0.643618, -1.463040, 0.624084, -0.085082, -0.376922, 0.911676,
            1.331000,
        ];
        let r1 = ttest_1samp(&data, 0.0, Alternative::TwoSided).unwrap();
        assert!(close(r1.statistic, -1.145017367038331, 1e-6));
        assert!(close(r1.pvalue, 0.2615641461880149, 1e-6));

        let r2 = ttest_ind(&data, &b, true, Alternative::TwoSided).unwrap();
        assert!(close(r2.statistic, -1.192964986111518, 1e-6));
        assert!(close(r2.pvalue, 0.2381963859425773, 1e-6));

        let g1 = &data[..10];
        let g2 = &data[10..20];
        let g3 = &data[20..];
        let r3 = anova(&[g1, g2, g3]).unwrap();
        assert!(close(r3.statistic, 6.569363238896068, 1e-6));
        assert!(close(r3.pvalue, 0.004734807901966396, 1e-6));

        let pr = pearsonr(&data[..20], &b[..20]).unwrap();
        assert!(close(pr.statistic, 0.1403657320371829, 1e-6));
        assert!(close(pr.pvalue, 0.5550232450550964, 1e-6));
    }

    #[test]
    fn domain_errors() {
        assert_eq!(
            Normal::new(0.0, -1.0).unwrap_err().code(),
            E4023_NSTATS_DOMAIN
        );
        let t = StudentT::new(5.0).unwrap();
        assert_eq!(t.ppf(1.5).unwrap_err().code(), E4023_NSTATS_DOMAIN);
    }

    #[test]
    fn student_t_ppf_roundtrip() {
        let t = StudentT::new(10.0).unwrap();
        for p in [0.1, 0.5, 0.9] {
            let x = t.ppf(p).unwrap();
            assert!((t.cdf(x).unwrap() - p).abs() < 1e-5);
        }
    }

    #[test]
    fn chi_square_ppf_roundtrip() {
        let c = ChiSquare::new(4.0).unwrap();
        for p in [0.1, 0.5, 0.9] {
            let x = c.ppf(p).unwrap();
            assert!((c.cdf(x).unwrap() - p).abs() < 1e-5);
        }
    }

    #[test]
    fn ols_perfect_fit() {
        let x: Vec<Vec<f64>> = (0..10).map(|i| vec![i as f64]).collect();
        let y: Vec<f64> = (0..10).map(|i| 3.0 + 1.5 * i as f64).collect();
        let r = ols(&x, &y).unwrap();
        assert!(close(r.coefficients[0], 3.0, 1e-8));
        assert!(close(r.coefficients[1], 1.5, 1e-8));
        assert!(close(r.r_squared, 1.0, 1e-8));
    }
}
