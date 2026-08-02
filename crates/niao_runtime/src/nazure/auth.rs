//! Azure authentication utilities — SharedKey (Blob/Queue/File) and
//! SharedKeyLite (Table), plus optional OAuth 2.0 Bearer token via
//! client-credentials flow.
//!
//! No third-party crates: HMAC-SHA256 via `niao_crypto`, base64 via `niao_codec`.

use niao_codec::base64;
use niao_crypto::hmac_sha256;
use std::time::{SystemTime, UNIX_EPOCH};

// ──────────────────────────────────────────────────────────────────────────────
// RFC 1123 date formatting (HTTP Date header, required by Azure REST)
// ──────────────────────────────────────────────────────────────────────────────

/// DOW starting from Thursday (epoch 1970-01-01 was a Thursday).
const DOW: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn is_leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Convert days-since-epoch (1970-01-01) into (year, month 1-12, day 1-31).
fn days_to_ymd(mut d: u64) -> (u64, u64, u64) {
    let mut year = 1970u64;
    loop {
        let yd = if is_leap(year) { 366 } else { 365 };
        if d < yd {
            break;
        }
        d -= yd;
        year += 1;
    }
    let md_tab = [31u64, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 1u64;
    for (i, &base) in md_tab.iter().enumerate() {
        let days_in = if i == 1 && is_leap(year) { 29 } else { base };
        if d < days_in {
            month = i as u64 + 1;
            break;
        }
        d -= days_in;
    }
    (year, month, d + 1)
}

/// Returns current UTC time formatted as RFC 1123, e.g. `"Mon, 15 Nov 2021 08:12:31 GMT"`.
pub fn rfc1123_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let total_days = secs / 86400;
    let dow = (total_days % 7) as usize;
    let (year, month, day) = days_to_ymd(total_days);
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!(
        "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
        DOW[dow],
        day,
        MONTHS[month as usize - 1],
        year,
        h,
        m,
        s
    )
}

// ──────────────────────────────────────────────────────────────────────────────
// HMAC-SHA256 + base64 helper
// ──────────────────────────────────────────────────────────────────────────────

fn hmac_b64(key: &[u8], data: &str) -> String {
    let mac = hmac_sha256(key, data.as_bytes());
    base64::encode_standard(&mac)
}

// ──────────────────────────────────────────────────────────────────────────────
// Blob / Queue / File — SharedKey (full)
// ──────────────────────────────────────────────────────────────────────────────

/// Build `Authorization: SharedKey {account}:{sig}` for Azure Blob/Queue/File REST.
///
/// - `content_length`: `""` for empty bodies, decimal string otherwise.
/// - `content_type`: `""` for requests without a body.
/// - `date`: RFC 1123 date string (value passed in the `x-ms-date` header).
/// - `ms_headers`: **sorted** `(lowercase-name, trimmed-value)` pairs for every
///   `x-ms-*` header that will be sent on the wire.
/// - `canon_resource`: e.g. `"/{account}/{container}/{blob}"` or
///   `"/{account}/{container}\ncomp:list\nprefix:{p}\nrestype:container"`.
///
/// Reference: <https://docs.microsoft.com/en-us/rest/api/storageservices/authorize-with-shared-key>
pub fn shared_key_blob(
    account: &str,
    key: &[u8],
    method: &str,
    content_length: &str,
    content_type: &str,
    date: &str,
    ms_headers: &[(String, String)],
    canon_resource: &str,
) -> String {
    // Build CanonicalizedHeaders: each `lowercase-name:trimmed-value\n`, sorted.
    let mut can_hdrs = String::new();
    for (k, v) in ms_headers {
        can_hdrs.push_str(k);
        can_hdrs.push(':');
        can_hdrs.push_str(v.trim());
        can_hdrs.push('\n');
    }

    // Azure SharedKey string-to-sign (13 fields, \n-separated):
    // VERB | CE | CL | Content-Length | CMD5 | Content-Type | Date |
    // If-Modified-Since | If-Match | If-None-Match | If-Unmodified-Since | Range |
    // CanonicalizedHeaders + CanonicalizedResource
    let sts = format!(
        "{method}\n\n\n{content_length}\n\n{content_type}\n{date}\n\n\n\n\n\n{can_hdrs}{canon_resource}"
    );
    format!("SharedKey {}:{}", account, hmac_b64(key, &sts))
}

// ──────────────────────────────────────────────────────────────────────────────
// Table Storage — SharedKeyLite
// ──────────────────────────────────────────────────────────────────────────────

/// Build `Authorization: SharedKeyLite {account}:{sig}` for Azure Table REST.
///
/// - `date`: RFC 1123 date string (value of `x-ms-date`).
/// - `canon_resource`: e.g. `"/{account}/{table}"`.
pub fn shared_key_lite_table(
    account: &str,
    key: &[u8],
    date: &str,
    canon_resource: &str,
) -> String {
    let sts = format!("{date}\n{canon_resource}");
    format!("SharedKeyLite {}:{}", account, hmac_b64(key, &sts))
}

// ──────────────────────────────────────────────────────────────────────────────
// Bearer token — OAuth 2.0 client-credentials
// ──────────────────────────────────────────────────────────────────────────────

/// Fetch an OAuth 2.0 Bearer token via the client-credentials flow from
/// Azure AD.  Returns `Ok(access_token_string)` or `Err(error_message)`.
pub fn fetch_bearer_token(
    tenant: &str,
    client_id: &str,
    client_secret: &str,
    scope: &str,
) -> Result<String, String> {
    let url = format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token");
    let body = format!(
        "grant_type=client_credentials&client_id={}&client_secret={}&scope={}",
        percent_encode_form(client_id),
        percent_encode_form(client_secret),
        percent_encode_form(scope),
    );
    let resp = niao_http::post(&url)
        .set("Content-Type", "application/x-www-form-urlencoded")
        .send_string(&body)
        .map_err(|e| format!("nazure: token request failed: {e}"))?;
    if resp.status < 200 || resp.status >= 300 {
        return Err(format!(
            "nazure: token endpoint returned status {}",
            resp.status
        ));
    }
    let body_str = String::from_utf8_lossy(&resp.body);
    // Minimal JSON extraction — avoid adding serde as a dependency inside auth.
    extract_json_string(&body_str, "access_token")
        .ok_or_else(|| "nazure: missing access_token in token response".to_string())
}

/// Very small percent-encoder for form field values (RFC 3986 unreserved chars safe).
fn percent_encode_form(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push(HEX[((b >> 4) & 0xf) as usize] as char);
                out.push(HEX[(b & 0xf) as usize] as char);
            }
        }
    }
    out
}

const HEX: &[u8; 16] = b"0123456789ABCDEF";

/// Extract a string value from a flat JSON object without a full JSON parser.
/// Handles `"key": "value"` patterns; suitable for the simple token endpoint response.
fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\"", key);
    let start = json.find(&needle)?;
    let after = json[start + needle.len()..].trim_start();
    let after = after.strip_prefix(':')?.trim_start();
    let after = after.strip_prefix('"')?;
    let end = after.find('"')?;
    Some(after[..end].to_string())
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc1123_epoch() {
        // 1970-01-01 00:00:00 GMT was a Thursday.
        let s = {
            // Temporarily hard-code the epoch instant.
            let secs = 0u64;
            let total_days = secs / 86400;
            let dow = (total_days % 7) as usize;
            let (year, month, day) = days_to_ymd(total_days);
            let h = (secs % 86400) / 3600;
            let m = (secs % 3600) / 60;
            let sec = secs % 60;
            format!(
                "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
                DOW[dow],
                day,
                MONTHS[month as usize - 1],
                year,
                h,
                m,
                sec
            )
        };
        assert_eq!(s, "Thu, 01 Jan 1970 00:00:00 GMT");
    }

    #[test]
    fn rfc1123_known_monday() {
        // 2021-11-15 was a Monday. Unix timestamp for 2021-11-15 00:00:00 UTC.
        let secs = 1636934400u64;
        let total_days = secs / 86400;
        let dow = (total_days % 7) as usize;
        let (year, month, day) = days_to_ymd(total_days);
        let h = (secs % 86400) / 3600;
        let mnt = (secs % 3600) / 60;
        let sec = secs % 60;
        let s = format!(
            "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
            DOW[dow],
            day,
            MONTHS[month as usize - 1],
            year,
            h,
            mnt,
            sec
        );
        assert_eq!(s, "Mon, 15 Nov 2021 00:00:00 GMT");
    }

    #[test]
    fn shared_key_blob_deterministic() {
        let key = b"fake-storage-key-32-bytes-padding";
        let date = "Mon, 15 Nov 2021 08:12:31 GMT";
        let ms = vec![
            ("x-ms-blob-type".to_string(), "BlockBlob".to_string()),
            ("x-ms-date".to_string(), date.to_string()),
            ("x-ms-version".to_string(), "2020-08-04".to_string()),
        ];
        let a1 = shared_key_blob(
            "acct",
            key,
            "PUT",
            "5",
            "text/plain",
            date,
            &ms,
            "/acct/c/b",
        );
        let a2 = shared_key_blob(
            "acct",
            key,
            "PUT",
            "5",
            "text/plain",
            date,
            &ms,
            "/acct/c/b",
        );
        assert_eq!(a1, a2);
        assert!(a1.starts_with("SharedKey acct:"));
    }

    #[test]
    fn shared_key_lite_table_deterministic() {
        let key = b"table-key-padding-12345678901234";
        let date = "Mon, 15 Nov 2021 08:00:00 GMT";
        let a = shared_key_lite_table("acct", key, date, "/acct/MyTable");
        assert!(a.starts_with("SharedKeyLite acct:"));
    }

    #[test]
    fn extract_json_string_basic() {
        let json = r#"{"token_type":"Bearer","access_token":"eyABC123","expires_in":3599}"#;
        assert_eq!(
            extract_json_string(json, "access_token"),
            Some("eyABC123".to_string())
        );
        assert_eq!(
            extract_json_string(json, "token_type"),
            Some("Bearer".to_string())
        );
        assert!(extract_json_string(json, "missing").is_none());
    }
}
