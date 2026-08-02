//! Probability distributions.

use crate::error::{StatsError, StatsResult};
use crate::special::{beta, betainc, gamma, gammainc, norm_cdf, norm_pdf, norm_ppf, ppf_from_cdf};
use niao_rand::{Rng, SeedableRng, Xoshiro256StarStar};

const SQRT_2PI: f64 = 2.5066282746310005;
const LN_SQRT_2PI: f64 = 0.9189385332046727;

fn check_pos(name: &str, v: f64) -> StatsResult<()> {
    if v <= 0.0 || !v.is_finite() {
        return Err(StatsError::Domain(format!("{name} must be positive")));
    }
    Ok(())
}

fn check_nonneg(name: &str, v: f64) -> StatsResult<()> {
    if v < 0.0 || !v.is_finite() {
        return Err(StatsError::Domain(format!("{name} must be non-negative")));
    }
    Ok(())
}

fn check_prob(name: &str, p: f64) -> StatsResult<()> {
    if p < 0.0 || p > 1.0 || !p.is_finite() {
        return Err(StatsError::Domain(format!("{name} must be in [0,1]")));
    }
    Ok(())
}

fn rng_from_seed(seed: u64) -> Xoshiro256StarStar {
    Xoshiro256StarStar::seed_from_u64(seed)
}

fn box_muller_pair(rng: &mut Xoshiro256StarStar) -> (f64, f64) {
    let u1 = rng.gen_f64().max(1e-300);
    let u2 = rng.gen_f64();
    let r = (-2.0 * u1.ln()).sqrt();
    let theta = 2.0 * std::f64::consts::PI * u2;
    (r * theta.cos(), r * theta.sin())
}

// ── Normal ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct Normal {
    pub mu: f64,
    pub sigma: f64,
}

impl Normal {
    pub fn new(mu: f64, sigma: f64) -> StatsResult<Self> {
        check_pos("sigma", sigma)?;
        Ok(Self { mu, sigma })
    }

    pub fn standard() -> Self {
        Self {
            mu: 0.0,
            sigma: 1.0,
        }
    }

    #[inline]
    pub fn pdf(&self, x: f64) -> f64 {
        let z = (x - self.mu) / self.sigma;
        norm_pdf(z) / self.sigma
    }

    #[inline]
    pub fn cdf(&self, x: f64) -> f64 {
        norm_cdf((x - self.mu) / self.sigma)
    }

    #[inline]
    pub fn sf(&self, x: f64) -> f64 {
        1.0 - self.cdf(x)
    }

    pub fn ppf(&self, p: f64) -> StatsResult<f64> {
        check_prob("p", p)?;
        Ok(self.mu + self.sigma * norm_ppf(p)?)
    }

    pub fn mean(&self) -> f64 {
        self.mu
    }
    pub fn var(&self) -> f64 {
        self.sigma * self.sigma
    }
    pub fn std(&self) -> f64 {
        self.sigma
    }

    pub fn rvs(&self, n: usize, seed: u64) -> Vec<f64> {
        let mut rng = rng_from_seed(seed);
        let mut out = Vec::with_capacity(n);
        let mut spare = None;
        for _ in 0..n {
            if let Some(z) = spare.take() {
                out.push(self.mu + self.sigma * z);
            } else {
                let (z0, z1) = box_muller_pair(&mut rng);
                out.push(self.mu + self.sigma * z0);
                spare = Some(z1);
            }
        }
        out
    }
}

// ── StudentT ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct StudentT {
    pub df: f64,
}

impl StudentT {
    pub fn new(df: f64) -> StatsResult<Self> {
        check_pos("df", df)?;
        Ok(Self { df })
    }

    pub fn pdf(&self, x: f64) -> f64 {
        let v = self.df;
        let coef = gamma((v + 1.0) / 2.0).unwrap()
            / (gamma(v / 2.0).unwrap() * (v * std::f64::consts::PI).sqrt());
        (1.0 + x * x / v).powf(-(v + 1.0) / 2.0) * coef
    }

    pub fn cdf(&self, x: f64) -> StatsResult<f64> {
        let v = self.df;
        let t2 = x * x;
        let ib = betainc(v / 2.0, 0.5, v / (v + t2))?;
        Ok(if x >= 0.0 { 1.0 - 0.5 * ib } else { 0.5 * ib })
    }

    pub fn sf(&self, x: f64) -> StatsResult<f64> {
        Ok(1.0 - self.cdf(x)?)
    }

    pub fn ppf(&self, p: f64) -> StatsResult<f64> {
        check_prob("p", p)?;
        ppf_from_cdf(|x| self.cdf(x), p, -1e6, 1e6)
    }

    pub fn mean(&self) -> StatsResult<f64> {
        if self.df <= 1.0 {
            Err(StatsError::Domain("t mean undefined for df <= 1".into()))
        } else {
            Ok(0.0)
        }
    }

    pub fn var(&self) -> StatsResult<f64> {
        if self.df <= 2.0 {
            Err(StatsError::Domain("t var undefined for df <= 2".into()))
        } else {
            Ok(self.df / (self.df - 2.0))
        }
    }

    pub fn std(&self) -> StatsResult<f64> {
        Ok(self.var()?.sqrt())
    }

    pub fn rvs(&self, n: usize, seed: u64) -> StatsResult<Vec<f64>> {
        let mut rng = rng_from_seed(seed);
        let norm = Normal::standard();
        let chi = ChiSquare::new(self.df)?;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let z = norm.rvs(1, rng.next_u64())[0];
            let c = chi.rvs(1, rng.next_u64())?[0];
            out.push(z / (c / self.df).sqrt());
        }
        Ok(out)
    }
}

// ── ChiSquare ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct ChiSquare {
    pub df: f64,
}

impl ChiSquare {
    pub fn new(df: f64) -> StatsResult<Self> {
        check_pos("df", df)?;
        Ok(Self { df })
    }

    pub fn pdf(&self, x: f64) -> StatsResult<f64> {
        if x < 0.0 {
            return Ok(0.0);
        }
        let k = self.df;
        let coef = 1.0 / (2.0_f64.powf(k / 2.0) * gamma(k / 2.0)?);
        Ok(coef * x.powf(k / 2.0 - 1.0) * (-x / 2.0).exp())
    }

    pub fn cdf(&self, x: f64) -> StatsResult<f64> {
        if x < 0.0 {
            return Ok(0.0);
        }
        gammainc(self.df / 2.0, x / 2.0)
    }

    pub fn sf(&self, x: f64) -> StatsResult<f64> {
        Ok(1.0 - self.cdf(x)?)
    }

    pub fn ppf(&self, p: f64) -> StatsResult<f64> {
        check_prob("p", p)?;
        ppf_from_cdf(|x| self.cdf(x), p, 0.0, self.df * 100.0 + 100.0)
    }

    pub fn mean(&self) -> f64 {
        self.df
    }
    pub fn var(&self) -> f64 {
        2.0 * self.df
    }
    pub fn std(&self) -> f64 {
        (2.0 * self.df).sqrt()
    }

    pub fn rvs(&self, n: usize, seed: u64) -> StatsResult<Vec<f64>> {
        let mut rng = rng_from_seed(seed);
        let g = Gamma::new(self.df / 2.0, 2.0)?;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(g.rvs(1, rng.next_u64())?[0]);
        }
        Ok(out)
    }
}

// ── F distribution ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct F {
    pub dfn: f64,
    pub dfd: f64,
}

impl F {
    pub fn new(dfn: f64, dfd: f64) -> StatsResult<Self> {
        check_pos("dfn", dfn)?;
        check_pos("dfd", dfd)?;
        Ok(Self { dfn, dfd })
    }

    pub fn pdf(&self, x: f64) -> StatsResult<f64> {
        if x < 0.0 {
            return Ok(0.0);
        }
        let d1 = self.dfn;
        let d2 = self.dfd;
        let coef = beta(d2 / 2.0, d1 / 2.0)? * d1.powf(d1 / 2.0) * d2.powf(d2 / 2.0)
            / (d1 * x + d2).powf((d1 + d2) / 2.0);
        Ok(coef * x.powf(d1 / 2.0 - 1.0))
    }

    pub fn cdf(&self, x: f64) -> StatsResult<f64> {
        if x < 0.0 {
            return Ok(0.0);
        }
        let d1 = self.dfn;
        let d2 = self.dfd;
        betainc(d1 / 2.0, d2 / 2.0, d1 * x / (d1 * x + d2))
    }

    pub fn sf(&self, x: f64) -> StatsResult<f64> {
        Ok(1.0 - self.cdf(x)?)
    }

    pub fn ppf(&self, p: f64) -> StatsResult<f64> {
        check_prob("p", p)?;
        ppf_from_cdf(|x| self.cdf(x), p, 0.0, 1e6)
    }

    pub fn mean(&self) -> StatsResult<f64> {
        if self.dfd <= 2.0 {
            Err(StatsError::Domain("F mean undefined for dfd <= 2".into()))
        } else {
            Ok(self.dfd / (self.dfd - 2.0))
        }
    }

    pub fn var(&self) -> StatsResult<f64> {
        let d2 = self.dfd;
        if d2 <= 4.0 {
            Err(StatsError::Domain("F var undefined for dfd <= 4".into()))
        } else {
            let d1 = self.dfn;
            let num = 2.0 * d2 * d2 * (d1 + d2 - 2.0);
            let den = d1 * (d2 - 2.0).powi(2) * (d2 - 4.0);
            Ok(num / den)
        }
    }

    pub fn std(&self) -> StatsResult<f64> {
        Ok(self.var()?.sqrt())
    }

    pub fn rvs(&self, n: usize, seed: u64) -> StatsResult<Vec<f64>> {
        let mut rng = rng_from_seed(seed);
        let chi1 = ChiSquare::new(self.dfn)?;
        let chi2 = ChiSquare::new(self.dfd)?;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let x1 = chi1.rvs(1, rng.next_u64())?[0];
            let x2 = chi2.rvs(1, rng.next_u64())?[0];
            out.push((x1 / self.dfn) / (x2 / self.dfd));
        }
        Ok(out)
    }
}

// ── Exponential ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct Exponential {
    pub scale: f64,
}

impl Exponential {
    pub fn new(scale: f64) -> StatsResult<Self> {
        check_pos("scale", scale)?;
        Ok(Self { scale })
    }

    pub fn pdf(&self, x: f64) -> f64 {
        if x < 0.0 {
            0.0
        } else {
            (-x / self.scale).exp() / self.scale
        }
    }

    pub fn cdf(&self, x: f64) -> f64 {
        if x < 0.0 {
            0.0
        } else {
            1.0 - (-x / self.scale).exp()
        }
    }

    pub fn sf(&self, x: f64) -> f64 {
        1.0 - self.cdf(x)
    }

    pub fn ppf(&self, p: f64) -> StatsResult<f64> {
        check_prob("p", p)?;
        Ok(-self.scale * (1.0 - p).ln())
    }

    pub fn mean(&self) -> f64 {
        self.scale
    }
    pub fn var(&self) -> f64 {
        self.scale * self.scale
    }
    pub fn std(&self) -> f64 {
        self.scale
    }

    pub fn rvs(&self, n: usize, seed: u64) -> Vec<f64> {
        let mut rng = rng_from_seed(seed);
        (0..n).map(|_| -self.scale * rng.gen_f64().ln()).collect()
    }
}

// ── Gamma ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct Gamma {
    pub shape: f64,
    pub scale: f64,
}

impl Gamma {
    pub fn new(shape: f64, scale: f64) -> StatsResult<Self> {
        check_pos("shape", shape)?;
        check_pos("scale", scale)?;
        Ok(Self { shape, scale })
    }

    pub fn pdf(&self, x: f64) -> StatsResult<f64> {
        if x < 0.0 {
            return Ok(0.0);
        }
        let a = self.shape;
        let b = self.scale;
        Ok(x.powf(a - 1.0) * (-x / b).exp() / (gamma(a)? * b.powf(a)))
    }

    pub fn cdf(&self, x: f64) -> StatsResult<f64> {
        if x < 0.0 {
            return Ok(0.0);
        }
        gammainc(self.shape, x / self.scale)
    }

    pub fn sf(&self, x: f64) -> StatsResult<f64> {
        Ok(1.0 - self.cdf(x)?)
    }

    pub fn ppf(&self, p: f64) -> StatsResult<f64> {
        check_prob("p", p)?;
        ppf_from_cdf(
            |x| self.cdf(x),
            p,
            0.0,
            self.shape * self.scale * 50.0 + 100.0,
        )
    }

    pub fn mean(&self) -> f64 {
        self.shape * self.scale
    }
    pub fn var(&self) -> f64 {
        self.shape * self.scale * self.scale
    }
    pub fn std(&self) -> f64 {
        self.var().sqrt()
    }

    pub fn rvs(&self, n: usize, seed: u64) -> StatsResult<Vec<f64>> {
        // Marsaglia-Tsang for shape >= 1; Ahrens for shape < 1
        let mut rng = rng_from_seed(seed);
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(sample_gamma(self.shape, self.scale, &mut rng)?);
        }
        Ok(out)
    }
}

fn sample_gamma(shape: f64, scale: f64, rng: &mut Xoshiro256StarStar) -> StatsResult<f64> {
    if shape < 1.0 {
        let u = rng.gen_f64();
        return Ok(sample_gamma(1.0 + shape, scale, rng)? * u.powf(1.0 / shape));
    }
    let d = shape - 1.0 / 3.0;
    let c = 1.0 / (9.0 * d).sqrt();
    loop {
        let mut x: f64;
        let mut v: f64;
        loop {
            let (z, _) = box_muller_pair(rng);
            x = z;
            v = 1.0 + c * x;
            if v > 0.0 {
                break;
            }
        }
        v = v * v * v;
        let u = rng.gen_f64();
        if u < 1.0 - 0.0331 * x * x * x * x {
            return Ok(d * v * scale);
        }
        if u.ln() < 0.5 * x * x + d * (1.0 - v + v.ln()) {
            return Ok(d * v * scale);
        }
    }
}

// ── Beta ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct Beta {
    pub a: f64,
    pub b: f64,
}

impl Beta {
    pub fn new(a: f64, b: f64) -> StatsResult<Self> {
        check_pos("a", a)?;
        check_pos("b", b)?;
        Ok(Self { a, b })
    }

    pub fn pdf(&self, x: f64) -> StatsResult<f64> {
        if x <= 0.0 || x >= 1.0 {
            return Ok(0.0);
        }
        Ok(x.powf(self.a - 1.0) * (1.0 - x).powf(self.b - 1.0) / beta(self.a, self.b)?)
    }

    pub fn cdf(&self, x: f64) -> StatsResult<f64> {
        if x <= 0.0 {
            return Ok(0.0);
        }
        if x >= 1.0 {
            return Ok(1.0);
        }
        betainc(self.a, self.b, x)
    }

    pub fn sf(&self, x: f64) -> StatsResult<f64> {
        Ok(1.0 - self.cdf(x)?)
    }

    pub fn ppf(&self, p: f64) -> StatsResult<f64> {
        check_prob("p", p)?;
        ppf_from_cdf(|x| self.cdf(x), p, 0.0, 1.0)
    }

    pub fn mean(&self) -> f64 {
        self.a / (self.a + self.b)
    }
    pub fn var(&self) -> f64 {
        let s = self.a + self.b;
        self.a * self.b / (s * s * (s + 1.0))
    }
    pub fn std(&self) -> f64 {
        self.var().sqrt()
    }

    pub fn rvs(&self, n: usize, seed: u64) -> StatsResult<Vec<f64>> {
        let mut rng = rng_from_seed(seed);
        let ga = Gamma::new(self.a, 1.0)?;
        let gb = Gamma::new(self.b, 1.0)?;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let x = ga.rvs(1, rng.next_u64())?[0];
            let y = gb.rvs(1, rng.next_u64())?[0];
            out.push(x / (x + y));
        }
        Ok(out)
    }
}

// ── Uniform ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct Uniform {
    pub loc: f64,
    pub scale: f64,
}

impl Uniform {
    pub fn new(loc: f64, scale: f64) -> StatsResult<Self> {
        check_pos("scale", scale)?;
        Ok(Self { loc, scale })
    }

    pub fn pdf(&self, x: f64) -> f64 {
        if x < self.loc || x > self.loc + self.scale {
            0.0
        } else {
            1.0 / self.scale
        }
    }

    pub fn cdf(&self, x: f64) -> f64 {
        if x < self.loc {
            0.0
        } else if x > self.loc + self.scale {
            1.0
        } else {
            (x - self.loc) / self.scale
        }
    }

    pub fn sf(&self, x: f64) -> f64 {
        1.0 - self.cdf(x)
    }

    pub fn ppf(&self, p: f64) -> StatsResult<f64> {
        check_prob("p", p)?;
        Ok(self.loc + p * self.scale)
    }

    pub fn mean(&self) -> f64 {
        self.loc + 0.5 * self.scale
    }
    pub fn var(&self) -> f64 {
        self.scale * self.scale / 12.0
    }
    pub fn std(&self) -> f64 {
        self.var().sqrt()
    }

    pub fn rvs(&self, n: usize, seed: u64) -> Vec<f64> {
        let mut rng = rng_from_seed(seed);
        (0..n)
            .map(|_| self.loc + rng.gen_f64() * self.scale)
            .collect()
    }
}

// ── Poisson ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct Poisson {
    pub mu: f64,
}

impl Poisson {
    pub fn new(mu: f64) -> StatsResult<Self> {
        check_pos("mu", mu)?;
        Ok(Self { mu })
    }

    pub fn pmf(&self, k: i64) -> StatsResult<f64> {
        if k < 0 {
            return Ok(0.0);
        }
        Ok(self.mu.powi(k as i32) * (-self.mu).exp() / gamma(k as f64 + 1.0)?)
    }

    pub fn cdf(&self, k: i64) -> StatsResult<f64> {
        if k < 0 {
            return Ok(0.0);
        }
        let mut sum = 0.0;
        for i in 0..=k {
            sum += self.pmf(i)?;
        }
        Ok(sum)
    }

    pub fn sf(&self, k: i64) -> StatsResult<f64> {
        Ok(1.0 - self.cdf(k)?)
    }

    pub fn ppf(&self, p: f64) -> StatsResult<f64> {
        check_prob("p", p)?;
        let mut k = 0i64;
        while self.cdf(k)? < p {
            k += 1;
            if k > 1_000_000 {
                return Err(StatsError::NonConvergence("poisson ppf".into()));
            }
        }
        Ok(k as f64)
    }

    pub fn mean(&self) -> f64 {
        self.mu
    }
    pub fn var(&self) -> f64 {
        self.mu
    }
    pub fn std(&self) -> f64 {
        self.mu.sqrt()
    }

    pub fn rvs(&self, n: usize, seed: u64) -> StatsResult<Vec<f64>> {
        let mut rng = rng_from_seed(seed);
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let mut k = 0i64;
            let mut p = 1.0;
            let l = (-self.mu).exp();
            loop {
                p *= rng.gen_f64();
                if p <= l {
                    break;
                }
                k += 1;
            }
            out.push(k as f64);
        }
        Ok(out)
    }
}

// ── Binomial ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct Binomial {
    pub n: u64,
    pub p: f64,
}

impl Binomial {
    pub fn new(n: u64, p: f64) -> StatsResult<Self> {
        check_prob("p", p)?;
        Ok(Self { n, p })
    }

    pub fn pmf(&self, k: i64) -> StatsResult<f64> {
        if k < 0 || k as u64 > self.n {
            return Ok(0.0);
        }
        let kf = k as f64;
        let nf = self.n as f64;
        let coef = gamma(nf + 1.0)? / (gamma(kf + 1.0)? * gamma(nf - kf + 1.0)?);
        Ok(coef * self.p.powf(kf) * (1.0 - self.p).powf(nf - kf))
    }

    pub fn cdf(&self, k: i64) -> StatsResult<f64> {
        if k < 0 {
            return Ok(0.0);
        }
        let mut sum = 0.0;
        for i in 0..=k.min(self.n as i64) {
            sum += self.pmf(i)?;
        }
        Ok(sum)
    }

    pub fn sf(&self, k: i64) -> StatsResult<f64> {
        Ok(1.0 - self.cdf(k)?)
    }

    pub fn ppf(&self, prob: f64) -> StatsResult<f64> {
        check_prob("prob", prob)?;
        for k in 0..=self.n as i64 {
            if self.cdf(k)? >= prob {
                return Ok(k as f64);
            }
        }
        Ok(self.n as f64)
    }

    pub fn mean(&self) -> f64 {
        self.n as f64 * self.p
    }
    pub fn var(&self) -> f64 {
        self.n as f64 * self.p * (1.0 - self.p)
    }
    pub fn std(&self) -> f64 {
        self.var().sqrt()
    }

    pub fn rvs(&self, n: usize, seed: u64) -> Vec<f64> {
        let mut rng = rng_from_seed(seed);
        (0..n)
            .map(|_| {
                let mut s = 0u64;
                for _ in 0..self.n {
                    if rng.gen_f64() < self.p {
                        s += 1;
                    }
                }
                s as f64
            })
            .collect()
    }
}

// ── Bernoulli ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct Bernoulli {
    pub p: f64,
}

impl Bernoulli {
    pub fn new(p: f64) -> StatsResult<Self> {
        check_prob("p", p)?;
        Ok(Self { p })
    }

    pub fn pmf(&self, k: i64) -> f64 {
        if k == 0 {
            1.0 - self.p
        } else if k == 1 {
            self.p
        } else {
            0.0
        }
    }

    pub fn cdf(&self, k: i64) -> f64 {
        if k < 0 {
            0.0
        } else if k < 1 {
            1.0 - self.p
        } else {
            1.0
        }
    }

    pub fn sf(&self, k: i64) -> f64 {
        1.0 - self.cdf(k)
    }

    pub fn ppf(&self, prob: f64) -> StatsResult<f64> {
        check_prob("prob", prob)?;
        Ok(if prob <= 1.0 - self.p { 0.0 } else { 1.0 })
    }

    pub fn mean(&self) -> f64 {
        self.p
    }
    pub fn var(&self) -> f64 {
        self.p * (1.0 - self.p)
    }
    pub fn std(&self) -> f64 {
        self.var().sqrt()
    }

    pub fn rvs(&self, n: usize, seed: u64) -> Vec<f64> {
        let mut rng = rng_from_seed(seed);
        (0..n)
            .map(|_| if rng.gen_f64() < self.p { 1.0 } else { 0.0 })
            .collect()
    }
}

// ── LogNormal ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct LogNormal {
    pub mu: f64,
    pub sigma: f64,
}

impl LogNormal {
    pub fn new(mu: f64, sigma: f64) -> StatsResult<Self> {
        check_pos("sigma", sigma)?;
        Ok(Self { mu, sigma })
    }

    pub fn pdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            0.0
        } else {
            let z = (x.ln() - self.mu) / self.sigma;
            norm_pdf(z) / (x * self.sigma)
        }
    }

    pub fn cdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            0.0
        } else {
            norm_cdf((x.ln() - self.mu) / self.sigma)
        }
    }

    pub fn sf(&self, x: f64) -> f64 {
        1.0 - self.cdf(x)
    }

    pub fn ppf(&self, p: f64) -> StatsResult<f64> {
        check_prob("p", p)?;
        Ok((self.mu + self.sigma * norm_ppf(p)?).exp())
    }

    pub fn mean(&self) -> f64 {
        (self.mu + 0.5 * self.sigma * self.sigma).exp()
    }
    pub fn var(&self) -> f64 {
        let s2 = self.sigma * self.sigma;
        (2.0 * self.mu + s2).exp() * ((s2).exp() - 1.0)
    }
    pub fn std(&self) -> f64 {
        self.var().sqrt()
    }

    pub fn rvs(&self, n: usize, seed: u64) -> Vec<f64> {
        let norm = Normal::new(self.mu, self.sigma).unwrap();
        norm.rvs(n, seed).into_iter().map(f64::exp).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, rtol: f64) -> bool {
        (a - b).abs() <= rtol * b.abs().max(1.0)
    }

    #[test]
    fn normal_pdf_cdf_ppf() {
        let n = Normal::standard();
        assert!(close(n.pdf(0.0), 0.3989422804014327, 1e-9));
        assert!(close(n.cdf(0.0), 0.5, 1e-9));
        for &x in &[-3.0, -1.0, 0.0, 1.0, 2.0] {
            let p = n.cdf(x);
            let back = n.ppf(p).unwrap();
            assert!((back - x).abs() < 1e-4, "roundtrip {x}");
        }
    }

    #[test]
    fn student_t_cdf() {
        let t = StudentT::new(5.0).unwrap();
        assert!(close(t.cdf(0.0).unwrap(), 0.5, 1e-9));
        assert!(close(t.cdf(2.0).unwrap(), 0.9490302389231076, 1e-6));
    }

    #[test]
    fn exponential_ppf_roundtrip() {
        let e = Exponential::new(2.0).unwrap();
        for p in [0.1, 0.5, 0.9] {
            let x = e.ppf(p).unwrap();
            assert!(close(e.cdf(x), p, 1e-9));
        }
    }

    #[test]
    fn bad_params_domain() {
        assert!(Normal::new(0.0, -1.0).is_err());
        assert!(StudentT::new(0.0).is_err());
    }
}
