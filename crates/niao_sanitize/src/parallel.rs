//! Parallel batch sanitization.

use crate::clean::{clean, CleanOpts, Sanitizer};
use crate::error::SanitizeError;
use niao_parallel::map;

/// Sanitize many HTML fragments in parallel.
pub fn parallel_clean(
    items: &[String],
    opts: &CleanOpts,
    threads: usize,
) -> Result<Vec<String>, SanitizeError> {
    let sanitizer = Sanitizer::new(opts.clone())?;
    Ok(map(items, threads, |s| sanitizer.clean(s)))
}

/// Parallel one-shot clean (re-validates opts per call).
pub fn parallel_clean_once(
    items: &[String],
    opts: &CleanOpts,
    threads: usize,
) -> Result<Vec<String>, SanitizeError> {
    validate_parallel(opts)?;
    let opts = opts.clone();
    Ok(map(items, threads, |s| clean(s, &opts).unwrap_or_default()))
}

fn validate_parallel(opts: &CleanOpts) -> Result<(), SanitizeError> {
    Sanitizer::new(opts.clone())?;
    Ok(())
}
