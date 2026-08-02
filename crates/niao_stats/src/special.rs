//! Special functions: erf, gamma, beta, incomplete beta/gamma.

use crate::error::{StatsError, StatsResult};

const SQRT_2: f64 = std::f64::consts::SQRT_2;
const SQRT_PI: f64 = 1.772453850905516;
const LN_2PI: f64 = 1.8378770664093453;
const MAX_ITERS: usize = 100;
const EPS: f64 = 1e-15;
const FPMIN: f64 = 1e-300;

#[inline]
pub fn erf(x: f64) -> f64 {
    if x == 0.0 {
        return 0.0;
    }
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * x);
    let poly = (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t
        + 0.254829592)
        * t;
    sign * (1.0 - poly * (-x * x).exp())
}

#[inline]
pub fn erfc(x: f64) -> f64 {
    1.0 - erf(x)
}

/// Lanczos log-gamma for x > 0.
#[inline]
pub fn lgamma(x: f64) -> StatsResult<f64> {
    if x <= 0.0 {
        if x.fract() == 0.0 {
            return Err(StatsError::Domain("lgamma at non-positive integer".into()));
        }
        let s = (std::f64::consts::PI * x).sin();
        return Ok(std::f64::consts::PI.ln() - s.ln() - lgamma(1.0 - x)?);
    }
    if (x - 0.5).abs() < 1e-15 {
        return Ok(0.5 * std::f64::consts::PI.ln());
    }
    let mut z = x;
    let mut corr = 0.0;
    while z < 7.0 {
        corr -= z.ln();
        z += 1.0;
    }
    Ok(lgamma_lanczos(z)? + corr)
}

fn lgamma_lanczos(x: f64) -> StatsResult<f64> {
    let g = 7.0;
    let c = [
        0.99999999999980993,
        676.5203681218851,
        -1259.1392167224028,
        771.32342877765313,
        -176.61502916214059,
        12.507343278686905,
        -0.13857109526572012,
        9.98434977e-6,
        1.50563277e-7,
    ];
    let mut sum = c[0];
    for i in 1..9 {
        sum += c[i] / (x - 1.0 + i as f64);
    }
    let t = x + g - 0.5;
    Ok(0.5 * LN_2PI + (x - 0.5) * t.ln() - t + sum.ln())
}

#[inline]
pub fn gamma(x: f64) -> StatsResult<f64> {
    Ok(lgamma(x)?.exp())
}

#[inline]
pub fn beta(a: f64, b: f64) -> StatsResult<f64> {
    if a <= 0.0 || b <= 0.0 {
        return Err(StatsError::Domain("beta requires a,b > 0".into()));
    }
    Ok((lgamma(a)? + lgamma(b)? - lgamma(a + b)?).exp())
}

/// Regularized incomplete beta I_x(a,b).
pub fn betainc(a: f64, b: f64, x: f64) -> StatsResult<f64> {
    if a <= 0.0 || b <= 0.0 {
        return Err(StatsError::Domain("betainc requires a,b > 0".into()));
    }
    if x <= 0.0 {
        return Ok(0.0);
    }
    if x >= 1.0 {
        return Ok(1.0);
    }
    let ln_bt = lgamma(a + b)? - lgamma(a)? - lgamma(b)? + a * x.ln() + b * (1.0 - x).ln();
    let bt = ln_bt.exp();
    if x < (a + 1.0) / (a + b + 2.0) {
        Ok(bt * betacf(a, b, x)? / a)
    } else {
        Ok(1.0 - bt * betacf(b, a, 1.0 - x)? / b)
    }
}

fn betacf(a: f64, b: f64, x: f64) -> StatsResult<f64> {
    let mut m = 1u32;
    let mut m2: f64;
    let mut aa: f64;
    let mut c = 1.0;
    let mut d = 1.0 - (a + b) * x / (a + 1.0);
    if d.abs() < FPMIN {
        d = FPMIN;
    }
    d = 1.0 / d;
    let mut h = d;
    while m <= MAX_ITERS as u32 {
        m2 = 2.0 * m as f64;
        aa = m as f64 * (b - m as f64) * x / ((a + m2 - 1.0) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < FPMIN {
            d = FPMIN;
        }
        c = 1.0 + aa / c;
        if c.abs() < FPMIN {
            c = FPMIN;
        }
        d = 1.0 / d;
        h *= d * c;
        aa = -(a + m as f64) * (a + b + m as f64) * x / ((a + m2) * (a + m2 + 1.0));
        d = 1.0 + aa * d;
        if d.abs() < FPMIN {
            d = FPMIN;
        }
        c = 1.0 + aa / c;
        if c.abs() < FPMIN {
            c = FPMIN;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < EPS {
            return Ok(h);
        }
        m += 1;
    }
    Err(StatsError::NonConvergence("betacf did not converge".into()))
}

/// Lower regularized incomplete gamma P(a,x).
pub fn gammainc(a: f64, x: f64) -> StatsResult<f64> {
    if a <= 0.0 {
        return Err(StatsError::Domain("gammainc requires a > 0".into()));
    }
    if x < 0.0 {
        return Err(StatsError::Domain("gammainc requires x >= 0".into()));
    }
    if x == 0.0 {
        return Ok(0.0);
    }
    if x < a + 1.0 {
        gammainc_series(a, x)
    } else {
        Ok(1.0 - gammainc_cf(a, x)?)
    }
}

fn gammainc_series(a: f64, x: f64) -> StatsResult<f64> {
    let ap = a;
    let mut sum = 1.0 / a;
    let mut del = sum;
    for n in 1..=MAX_ITERS {
        del *= x / (ap + n as f64);
        sum += del;
        if del.abs() < EPS * sum.abs() {
            return Ok(sum * (-x + a * x.ln() - lgamma(a)?).exp());
        }
    }
    Err(StatsError::NonConvergence(
        "gammainc series did not converge".into(),
    ))
}

fn gammainc_cf(a: f64, x: f64) -> StatsResult<f64> {
    let mut b = x + 1.0 - a;
    let mut c = 1.0 / FPMIN;
    let mut d = 1.0 / b;
    let mut h = d;
    for i in 1..=MAX_ITERS {
        let an = -(i as f64) * (i as f64 - a);
        b += 2.0;
        d = an * d + b;
        if d.abs() < FPMIN {
            d = FPMIN;
        }
        c = b + an / c;
        if c.abs() < FPMIN {
            c = FPMIN;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < EPS {
            return Ok((-x + a * x.ln() - lgamma(a)?).exp() * h);
        }
    }
    Err(StatsError::NonConvergence(
        "gammainc cf did not converge".into(),
    ))
}

/// Standard normal CDF.
#[inline]
pub fn norm_cdf(z: f64) -> f64 {
    0.5 * (1.0 + erf(z / SQRT_2))
}

/// Standard normal PDF.
#[inline]
pub fn norm_pdf(z: f64) -> f64 {
    (-0.5 * z * z).exp() / (SQRT_2 * SQRT_PI)
}

/// Acklam inverse normal CDF (ppf).
pub fn norm_ppf(p: f64) -> StatsResult<f64> {
    if p <= 0.0 || p >= 1.0 {
        return Err(StatsError::Domain("norm_ppf requires 0 < p < 1".into()));
    }
    const A: [f64; 6] = [
        -3.969683028665376e+01,
        2.209460984245205e+02,
        -2.759285104469687e+02,
        1.383577518672690e+02,
        -3.066479806614716e+01,
        2.506628277459239e+00,
    ];
    const B: [f64; 5] = [
        -5.447609879822406e+01,
        1.615858368580409e+02,
        -1.556989798598866e+02,
        6.680131188771972e+01,
        -1.328068155288572e+01,
    ];
    const C: [f64; 6] = [
        -7.784894002430293e-03,
        -3.223964580411365e-01,
        -2.400758277161838e+00,
        -2.549507530012656e+00,
        4.374664141464968e+00,
        2.938163982698783e+00,
    ];
    const D: [f64; 4] = [
        7.784695709091636e-03,
        3.224671290700398e-01,
        2.445134137142996e+00,
        3.754408661907416e+00,
    ];
    const P_LOW: f64 = 0.02425;
    const P_HIGH: f64 = 1.0 - P_LOW;

    if p < P_LOW {
        let q = (-2.0 * p.ln()).sqrt();
        let num = ((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5];
        let den = (((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0;
        return Ok(num / den);
    }
    if p > P_HIGH {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        let num = ((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5];
        let den = (((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0;
        return Ok(-(num / den));
    }
    let q = p - 0.5;
    let r = q * q;
    let num = (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q;
    let den = ((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0;
    Ok(num / den)
}

/// Generic ppf via bisection on cdf.
pub fn ppf_from_cdf<F>(cdf: F, p: f64, lo: f64, hi: f64) -> StatsResult<f64>
where
    F: Fn(f64) -> StatsResult<f64>,
{
    if p <= 0.0 || p >= 1.0 {
        return Err(StatsError::Domain("ppf requires 0 < p < 1".into()));
    }
    let mut a = lo;
    let mut b = hi;
    let mut x = 0.5 * (a + b);
    for _ in 0..MAX_ITERS {
        let fx = cdf(x)? - p;
        if fx.abs() < 1e-10 {
            return Ok(x);
        }
        if fx > 0.0 {
            b = x;
        } else {
            a = x;
        }
        x = 0.5 * (a + b);
        if (b - a).abs() < 1e-10 {
            return Ok(x);
        }
    }
    Err(StatsError::NonConvergence("ppf did not converge".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, rtol: f64) -> bool {
        (a - b).abs() <= rtol * b.abs().max(1.0) + 1e-12
    }

    #[test]
    fn erf_vs_scipy() {
        let cases = [
            (0.0, 0.0),
            (0.5, 0.5204998778130465),
            (1.0, 0.8427007929497148),
            (2.0, 0.9953222650189527),
        ];
        for (x, want) in cases {
            assert!(close(erf(x), want, 1e-6), "erf({x}) got {}", erf(x));
        }
    }

    #[test]
    fn lgamma_vs_scipy() {
        let cases = [
            (0.5, 0.5723649429247),
            (1.0, 0.0),
            (2.5, 0.2846828704729192),
        ];
        for (x, want) in cases {
            assert!(close(lgamma(x).unwrap(), want, 1e-9), "lgamma({x})");
        }
    }

    #[test]
    fn betainc_vs_scipy() {
        assert!(close(betainc(2.0, 3.0, 0.5).unwrap(), 0.6875, 1e-9));
        assert!(close(betainc(5.0, 2.0, 0.3).unwrap(), 0.010935, 1e-6));
    }

    #[test]
    fn gammainc_vs_scipy() {
        assert!(close(
            gammainc(2.0, 0.5).unwrap(),
            0.09020401043104986,
            1e-9
        ));
        assert!(close(gammainc(5.0, 3.0).unwrap(), 0.1847367554762279, 1e-9));
    }

    #[test]
    fn norm_ppf_roundtrip() {
        for &x in &[-3.0, -1.0, 0.0, 1.0, 2.0] {
            let p = norm_cdf(x);
            let back = norm_ppf(p).unwrap();
            assert!((back - x).abs() < 1e-4, "roundtrip at {x}");
        }
    }
}
