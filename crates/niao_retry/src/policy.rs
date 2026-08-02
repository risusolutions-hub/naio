//! Retry policy configuration — stop conditions, retry predicates, and backoff knobs.

/// Backoff growth strategy between attempts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackoffStrategy {
    /// Same delay every attempt (after jitter).
    Fixed,
    /// `min_wait * multiplier^(attempt-1)`, capped at `max_wait`.
    Exponential,
    /// Exponential delay multiplied by a random factor in `[0.5, 1.5)`.
    RandomExponential,
    /// AWS-style decorrelated jitter: random between `min_wait` and `prev * 3`.
    Decorrelated,
    /// Alias for fixed — explicit constant delay.
    Constant,
}

impl BackoffStrategy {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "fixed" | "constant" => Some(Self::Fixed),
            "exponential" | "exp" => Some(Self::Exponential),
            "random_exponential" | "random_exp" | "rand_exp" => Some(Self::RandomExponential),
            "decorrelated" | "decor" => Some(Self::Decorrelated),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fixed | Self::Constant => "fixed",
            Self::Exponential => "exponential",
            Self::RandomExponential => "random_exponential",
            Self::Decorrelated => "decorrelated",
        }
    }
}

/// Jitter applied to a computed wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JitterKind {
    /// No jitter — use the computed wait as-is.
    None,
    /// Uniform in `[0, wait]`.
    Full,
    /// `wait/2 + uniform[0, wait/2]`.
    Equal,
    /// Decorrelated: random between `min_wait` and `prev_wait * 3` (capped).
    Decorrelated,
}

impl JitterKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "none" | "off" | "false" => Some(Self::None),
            "full" | "true" => Some(Self::Full),
            "equal" | "half" => Some(Self::Equal),
            "decorrelated" | "decor" => Some(Self::Decorrelated),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Full => "full",
            Self::Equal => "equal",
            Self::Decorrelated => "decorrelated",
        }
    }
}

/// Immutable retry policy — parsed once from Niao opts, reused across calls.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum invocation attempts (>= 1), or 0 for unlimited until deadline/stop predicate.
    pub max_attempts: u32,
    /// Minimum wait between retries in milliseconds.
    pub min_wait_ms: u64,
    /// Upper cap on any single wait in milliseconds.
    pub max_wait_ms: u64,
    /// Exponential multiplier (>= 1.0).
    pub multiplier: f64,
    pub strategy: BackoffStrategy,
    pub jitter: JitterKind,
    /// Total wall-clock budget; `None` = no deadline.
    pub deadline_ms: Option<u64>,
    /// Retry when the callee returns a catchable `error` value.
    pub retry_on_error: bool,
    /// Retry when the callee returns `nil`.
    pub retry_on_nil: bool,
    /// Block the calling thread between retries.
    pub sleep: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            min_wait_ms: 500,
            max_wait_ms: 30_000,
            multiplier: 2.0,
            strategy: BackoffStrategy::Exponential,
            jitter: JitterKind::Full,
            deadline_ms: None,
            retry_on_error: true,
            retry_on_nil: false,
            sleep: true,
        }
    }
}

impl RetryPolicy {
    pub fn validate(&self) -> Result<(), String> {
        // max_attempts == 0 means unlimited (until deadline / stop_on).
        if self.multiplier < 1.0 {
            return Err("multiplier must be >= 1.0".into());
        }
        if self.min_wait_ms > self.max_wait_ms {
            return Err("min_wait_ms must be <= max_wait_ms".into());
        }
        Ok(())
    }
}

/// Outcome of a retry execution loop (Rust-side bookkeeping).
#[derive(Debug, Clone)]
pub struct RetryOutcome {
    pub attempts: u32,
    pub sleep_ms: u64,
    pub elapsed_ms: u64,
    pub stopped_by_deadline: bool,
    pub stopped_by_attempts: bool,
}

impl RetryOutcome {
    pub fn new(attempts: u32, sleep_ms: u64, elapsed_ms: u64) -> Self {
        Self {
            attempts,
            sleep_ms,
            elapsed_ms,
            stopped_by_deadline: false,
            stopped_by_attempts: false,
        }
    }
}
