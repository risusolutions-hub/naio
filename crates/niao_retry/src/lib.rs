//! `niao_retry` — exponential backoff, jitter, deadlines, and retry predicates.
//!
//! Core timing math lives here; the Niao runtime wires callables, sleep, and
//! predicate dispatch in `niao_runtime::nretry`.

mod backoff;
mod policy;

pub use backoff::{
    apply_jitter, compute_wait_ms, deadline_exceeded, exponential_raw, should_stop_attempts,
};
pub use policy::{BackoffStrategy, JitterKind, RetryOutcome, RetryPolicy};
