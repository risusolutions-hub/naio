//! Probability distributions.

use crate::rng::Rng;

/// Sample values of type `T` from a distribution using a generator.
pub trait Distribution<T> {
    fn sample<R: Rng>(&self, rng: &mut R) -> T;
}

/// Error returned when constructing a [`Normal`] with invalid parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct NormalError(&'static str);

impl std::fmt::Display for NormalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for NormalError {}

/// Normal (Gaussian) distribution, sampled via the Box–Muller transform.
#[derive(Debug, Clone, Copy)]
pub struct Normal {
    mean: f64,
    std_dev: f64,
}

impl Normal {
    /// Create a normal distribution with the given `mean` and `std_dev`.
    pub fn new(mean: f64, std_dev: f64) -> Result<Self, NormalError> {
        if !mean.is_finite() {
            return Err(NormalError("mean must be finite"));
        }
        if !std_dev.is_finite() || std_dev < 0.0 {
            return Err(NormalError("std_dev must be finite and non-negative"));
        }
        Ok(Self { mean, std_dev })
    }

    /// The distribution mean.
    pub fn mean(&self) -> f64 {
        self.mean
    }

    /// The distribution standard deviation.
    pub fn std_dev(&self) -> f64 {
        self.std_dev
    }
}

impl Distribution<f64> for Normal {
    fn sample<R: Rng>(&self, rng: &mut R) -> f64 {
        // Box–Muller: two uniforms → one standard normal deviate.
        let mut u1 = rng.gen_f64(); // [0, 1)
        if u1 <= f64::MIN_POSITIVE {
            u1 = f64::MIN_POSITIVE; // avoid ln(0)
        }
        let u2 = rng.gen_f64();
        let mag = (-2.0 * u1.ln()).sqrt();
        let z0 = mag * (std::f64::consts::TAU * u2).cos();
        self.mean + self.std_dev * z0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StdRng;
    use crate::SeedableRng;

    #[test]
    fn normal_new_validates() {
        assert!(Normal::new(0.0, 1.0).is_ok());
        assert!(Normal::new(0.0, -1.0).is_err());
        assert!(Normal::new(f64::NAN, 1.0).is_err());
    }

    #[test]
    fn normal_mean_is_close() {
        let normal = Normal::new(5.0, 2.0).unwrap();
        let mut rng = StdRng::seed_from_u64(123);
        let n = 100_000;
        let sum: f64 = (0..n).map(|_| normal.sample(&mut rng)).sum();
        let mean = sum / n as f64;
        assert!((mean - 5.0).abs() < 0.1, "empirical mean {mean} off");
    }
}
