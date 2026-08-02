//! Timestamp tolerance checks (replay defense window).

use crate::error::{WebhookError, WebhookResult};
use crate::secret::DEFAULT_TOLERANCE_SECS;
use std::time::{SystemTime, UNIX_EPOCH};

/// Current Unix time in seconds.
#[inline]
pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Parse a webhook-timestamp header value as integer Unix seconds.
pub fn parse_timestamp(header: &str) -> WebhookResult<i64> {
    let s = header.trim();
    if s.is_empty() {
        return Err(WebhookError::InvalidTimestamp);
    }
    // Reject floats / scientific notation for strictness; allow leading '+'.
    if s.contains('.') || s.contains('e') || s.contains('E') {
        return Err(WebhookError::InvalidTimestamp);
    }
    s.parse::<i64>().map_err(|_| WebhookError::InvalidTimestamp)
}

/// Verify timestamp is within `tolerance` seconds of `now`.
pub fn check_timestamp(ts: i64, now: i64, tolerance: i64) -> WebhookResult<()> {
    let tol = if tolerance < 0 {
        DEFAULT_TOLERANCE_SECS
    } else {
        tolerance
    };
    if now - ts > tol {
        return Err(WebhookError::TimestampTooOld);
    }
    if ts > now + tol {
        return Err(WebhookError::TimestampTooNew);
    }
    Ok(())
}

/// Parse + check a timestamp header.
pub fn verify_timestamp_header(header: &str, now: i64, tolerance: i64) -> WebhookResult<i64> {
    let ts = parse_timestamp(header)?;
    check_timestamp(ts, now, tolerance)?;
    Ok(ts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn within_window() {
        let now = 1_000_000;
        assert!(check_timestamp(now, now, 300).is_ok());
        assert!(check_timestamp(now - 299, now, 300).is_ok());
        assert!(check_timestamp(now + 299, now, 300).is_ok());
    }

    #[test]
    fn too_old_new() {
        let now = 1_000_000;
        assert!(matches!(
            check_timestamp(now - 301, now, 300),
            Err(WebhookError::TimestampTooOld)
        ));
        assert!(matches!(
            check_timestamp(now + 301, now, 300),
            Err(WebhookError::TimestampTooNew)
        ));
    }

    #[test]
    fn bad_parse() {
        assert!(parse_timestamp("hello").is_err());
        assert!(parse_timestamp("1.5").is_err());
        assert!(parse_timestamp("").is_err());
    }
}
