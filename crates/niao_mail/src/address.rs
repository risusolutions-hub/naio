//! Address format / parse helpers (~email.utils).

use crate::error::MailError;

/// Parsed mailbox address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailAddr {
    pub name: Option<String>,
    pub email: String,
}

/// Format `"Name" <email>` or bare `email`.
pub fn format_addr(name: Option<&str>, email: &str) -> Result<String, MailError> {
    let email = email.trim();
    if email.is_empty() || !email.contains('@') {
        return Err(MailError::InvalidAddress(email.to_string()));
    }
    match name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) => {
            if needs_quote(n) {
                Ok(format!("\"{}\" <{email}>", escape_phrase(n)))
            } else {
                Ok(format!("{n} <{email}>"))
            }
        }
        None => Ok(email.to_string()),
    }
}

/// Parse a single address from `Name <email>` / `"Name" <email>` / bare email.
pub fn parse_addr(raw: &str) -> Result<MailAddr, MailError> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(MailError::InvalidAddress("empty".into()));
    }
    if let Some(lt) = s.rfind('<') {
        let gt = s[lt..]
            .find('>')
            .map(|i| lt + i)
            .ok_or_else(|| MailError::InvalidAddress(s.to_string()))?;
        let email = s[lt + 1..gt].trim().to_string();
        if email.is_empty() || !email.contains('@') {
            return Err(MailError::InvalidAddress(email));
        }
        let mut name = s[..lt].trim().to_string();
        if name.starts_with('"') && name.ends_with('"') && name.len() >= 2 {
            name = unescape_phrase(&name[1..name.len() - 1]);
        }
        let name = if name.is_empty() { None } else { Some(name) };
        Ok(MailAddr { name, email })
    } else if s.contains('@') {
        Ok(MailAddr {
            name: None,
            email: s.to_string(),
        })
    } else {
        Err(MailError::InvalidAddress(s.to_string()))
    }
}

/// Split a comma-separated address list (does not split commas inside quotes).
pub fn parse_addrs(raw: &str) -> Result<Vec<MailAddr>, MailError> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut in_quotes = false;
    let bytes = raw.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'"' => in_quotes = !in_quotes,
            b',' if !in_quotes => {
                let piece = raw[start..i].trim();
                if !piece.is_empty() {
                    out.push(parse_addr(piece)?);
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    let piece = raw[start..].trim();
    if !piece.is_empty() {
        out.push(parse_addr(piece)?);
    }
    Ok(out)
}

/// Generate a Message-ID like `<nmail.<nonce>@domain>`.
pub fn make_msgid(domain: Option<&str>) -> String {
    let dom = domain.unwrap_or("localhost");
    let nonce = unique_token();
    format!("<nmail.{nonce}@{dom}>")
}

/// Format an RFC 2822 date. `unix_secs` of `None` uses a deterministic fallback epoch.
pub fn format_date(unix_secs: Option<i64>) -> String {
    let secs = unix_secs.unwrap_or(0);
    let days = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    let months = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    // Civil conversion from Unix seconds (UTC).
    let z = secs.div_euclid(86400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let tod = secs.rem_euclid(86400) as u32;
    let hh = tod / 3600;
    let mm = (tod % 3600) / 60;
    let ss = tod % 60;
    let wday = ((secs.div_euclid(86400) + 4).rem_euclid(7)) as usize;
    format!(
        "{}, {:02} {} {:04} {:02}:{:02}:{:02} +0000",
        days[wday],
        d,
        months[(m as usize) - 1],
        y,
        hh,
        mm,
        ss
    )
}

fn needs_quote(s: &str) -> bool {
    s.bytes()
        .any(|b| !b.is_ascii_alphanumeric() && b != b' ' && b != b'-' && b != b'.' && b != b'_')
}

fn escape_phrase(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn unescape_phrase(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(n) = chars.next() {
                out.push(n);
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn unique_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_and_parse() {
        let f = format_addr(Some("Ada Lovelace"), "ada@example.com").unwrap();
        assert_eq!(f, "Ada Lovelace <ada@example.com>");
        let p = parse_addr(&f).unwrap();
        assert_eq!(p.email, "ada@example.com");
        assert_eq!(p.name.as_deref(), Some("Ada Lovelace"));
    }

    #[test]
    fn parse_list() {
        let list = parse_addrs(r#"Ada <a@e.com>, "B, C" <b@e.com>"#).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].name.as_deref(), Some("B, C"));
    }

    #[test]
    fn date_epoch() {
        let d = format_date(Some(0));
        assert!(d.contains("1970"));
        assert!(d.contains("+0000"));
    }
}
