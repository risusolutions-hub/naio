//! Backoff and jitter computation — hot path, allocation-free.

use crate::policy::{BackoffStrategy, JitterKind, RetryPolicy};
use niao_rand::Rng;

#[inline]
fn cap_wait(wait: u64, policy: &RetryPolicy) -> u64 {
    wait.min(policy.max_wait_ms)
}

/// Raw exponential delay before jitter: `min * multiplier^(attempt-1)`.
#[inline]
pub fn exponential_raw(attempt: u32, policy: &RetryPolicy) -> u64 {
    if attempt <= 1 {
        return policy.min_wait_ms;
    }
    let exp = (attempt - 1) as i32;
    let factor = policy.multiplier.powi(exp);
    let scaled = (policy.min_wait_ms as f64 * factor).round();
    cap_wait(scaled.max(0.0) as u64, policy)
}

/// Compute wait milliseconds for `attempt` (1-based), using `prev_wait` for decorrelated modes.
#[inline]
pub fn compute_wait_ms(
    attempt: u32,
    policy: &RetryPolicy,
    prev_wait: u64,
    rng: &mut impl Rng,
) -> u64 {
    let base = match policy.strategy {
        BackoffStrategy::Fixed | BackoffStrategy::Constant => policy.min_wait_ms,
        BackoffStrategy::Exponential => exponential_raw(attempt, policy),
        BackoffStrategy::RandomExponential => {
            let raw = exponential_raw(attempt, policy) as f64;
            let factor = 0.5 + (rng.next_u64() as f64 / u64::MAX as f64);
            cap_wait((raw * factor).round() as u64, policy)
        }
        BackoffStrategy::Decorrelated => {
            let upper = prev_wait.saturating_mul(3).max(policy.min_wait_ms);
            let span = upper.saturating_sub(policy.min_wait_ms);
            if span == 0 {
                policy.min_wait_ms
            } else {
                policy.min_wait_ms + (rng.next_u64() % (span + 1))
            }
        }
    };

    apply_jitter(base, policy.jitter, policy, prev_wait, rng)
}

/// Apply jitter to an already-computed wait.
#[inline]
pub fn apply_jitter(
    wait_ms: u64,
    jitter: JitterKind,
    policy: &RetryPolicy,
    prev_wait: u64,
    rng: &mut impl Rng,
) -> u64 {
    match jitter {
        JitterKind::None => cap_wait(wait_ms, policy),
        JitterKind::Full => {
            if wait_ms == 0 {
                0
            } else {
                rng.next_u64() % (wait_ms + 1)
            }
        }
        JitterKind::Equal => {
            let half = wait_ms / 2;
            half + if half == 0 {
                0
            } else {
                rng.next_u64() % (half + 1)
            }
        }
        JitterKind::Decorrelated => {
            let upper = prev_wait
                .saturating_mul(3)
                .max(wait_ms.max(policy.min_wait_ms));
            let lo = policy.min_wait_ms;
            let span = upper.saturating_sub(lo);
            if span == 0 {
                lo
            } else {
                lo + (rng.next_u64() % (span + 1))
            }
        }
    }
}

/// Whether another attempt is allowed under attempt/deadline limits.
#[inline]
pub fn should_stop_attempts(attempt: u32, policy: &RetryPolicy) -> bool {
    policy.max_attempts > 0 && attempt >= policy.max_attempts
}

/// Whether the deadline has been exceeded (`start_ms` + `deadline_ms` <= `now_ms`).
#[inline]
pub fn deadline_exceeded(start_ms: u64, now_ms: u64, deadline_ms: Option<u64>) -> bool {
    match deadline_ms {
        Some(limit) if limit > 0 => now_ms.saturating_sub(start_ms) >= limit,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_rand::SeedableRng;
    use niao_rand::Xoshiro256StarStar;

    fn policy() -> RetryPolicy {
        RetryPolicy {
            max_attempts: 5,
            min_wait_ms: 100,
            max_wait_ms: 10_000,
            multiplier: 2.0,
            strategy: BackoffStrategy::Exponential,
            jitter: JitterKind::None,
            deadline_ms: None,
            retry_on_error: true,
            retry_on_nil: false,
            sleep: true,
        }
    }

    #[test]
    fn exponential_growth() {
        let p = policy();
        assert_eq!(exponential_raw(1, &p), 100);
        assert_eq!(exponential_raw(2, &p), 200);
        assert_eq!(exponential_raw(3, &p), 400);
    }

    #[test]
    fn cap_at_max() {
        let p = RetryPolicy {
            max_wait_ms: 250,
            ..policy()
        };
        assert_eq!(exponential_raw(10, &p), 250);
    }

    #[test]
    fn full_jitter_bounded() {
        let p = policy();
        let mut rng = Xoshiro256StarStar::seed_from_u64(42);
        for _ in 0..100 {
            let j = apply_jitter(1000, JitterKind::Full, &p, 100, &mut rng);
            assert!(j <= 1000);
        }
    }

    #[test]
    fn deadline() {
        assert!(!deadline_exceeded(0, 999, Some(1000)));
        assert!(deadline_exceeded(0, 1000, Some(1000)));
    }
}
