use crate::error::{FeedError, FeedResult};
use chrono::{DateTime, NaiveDateTime, Utc};

/// Parsed date with unix milliseconds and normalized ISO string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDate {
    pub raw: String,
    pub iso: String,
    pub unix_ms: i64,
}

/// Parse RFC 822 / RFC 3339 / ISO 8601 feed timestamps.
///
/// >>> use niao_feed::parse_date;
/// >>> let d = parse_date("Mon, 06 Sep 2010 00:01:00 +0000").unwrap();
/// >>> d.unix_ms > 0
/// true
pub fn parse_date(raw: &str) -> FeedResult<ParsedDate> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(FeedError::InvalidDate("empty date".into()));
    }
    if let Ok(dt) = DateTime::parse_from_rfc2822(trimmed) {
        let utc = dt.with_timezone(&Utc);
        return Ok(ParsedDate {
            raw: raw.into(),
            iso: utc.to_rfc3339(),
            unix_ms: utc.timestamp_millis(),
        });
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        let utc = dt.with_timezone(&Utc);
        return Ok(ParsedDate {
            raw: raw.into(),
            iso: utc.to_rfc3339(),
            unix_ms: utc.timestamp_millis(),
        });
    }
    if let Ok(nd) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S") {
        let utc = DateTime::<Utc>::from_naive_utc_and_offset(nd, Utc);
        return Ok(ParsedDate {
            raw: raw.into(),
            iso: utc.to_rfc3339(),
            unix_ms: utc.timestamp_millis(),
        });
    }
    Err(FeedError::InvalidDate(format!(
        "unrecognized date: {trimmed}"
    )))
}

/// Format unix milliseconds as RFC 3339 UTC.
///
/// >>> use niao_feed::format_date;
/// >>> format_date(1283730060000).starts_with("2010")
/// true
pub fn format_date(unix_ms: i64) -> String {
    let secs = unix_ms.div_euclid(1000);
    let nsec = ((unix_ms.rem_euclid(1000)) * 1_000_000) as u32;
    let dt = DateTime::<Utc>::from_timestamp(secs, nsec).unwrap_or_else(Utc::now);
    dt.to_rfc3339()
}

pub(crate) fn datetime_to_fields(dt: &DateTime<Utc>) -> (String, i64) {
    (dt.to_rfc3339(), dt.timestamp_millis())
}

pub(crate) fn optional_datetime(dt: Option<DateTime<Utc>>) -> (Option<String>, Option<i64>) {
    dt.map(|d| datetime_to_fields(&d))
        .map(|(s, ms)| (Some(s), Some(ms)))
        .unwrap_or((None, None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc822() {
        let d = parse_date("Mon, 06 Sep 2010 00:01:00 +0000").unwrap();
        assert!(d.unix_ms > 0);
    }
}
