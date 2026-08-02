use std::collections::HashMap;

/// A single iCalendar / vCard property (RFC 5545 / RFC 6350).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    pub name: String,
    pub params: HashMap<String, Vec<String>>,
    pub value: String,
}

impl Property {
    /// >>> use niao_ical::Property;
    /// >>> let p = Property::new("SUMMARY", "Team standup");
    /// >>> p.name == "SUMMARY" && p.value == "Team standup"
    /// true
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into().to_ascii_uppercase(),
            params: HashMap::new(),
            value: value.into(),
        }
    }

    /// >>> use niao_ical::Property;
    /// >>> let p = Property::new("DTSTART", "20260105T090000Z").with_param("VALUE", "DATE-TIME");
    /// >>> p.params.get("VALUE").map(|v| v[0].as_str()) == Some("DATE-TIME")
    /// true
    pub fn with_param(mut self, key: impl Into<String>, val: impl Into<String>) -> Self {
        self.params
            .entry(key.into().to_ascii_uppercase())
            .or_default()
            .push(val.into());
        self
    }

    /// First parameter value for `key`, if any.
    pub fn param(&self, key: &str) -> Option<&str> {
        self.params
            .get(&key.to_ascii_uppercase())
            .and_then(|v| v.first())
            .map(|s| s.as_str())
    }

    /// All values for a parameter key.
    pub fn param_all(&self, key: &str) -> &[String] {
        self.params
            .get(&key.to_ascii_uppercase())
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
}

/// Unescape RFC 5545 property text values.
pub fn unescape_value(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some(',') => out.push(','),
                Some(';') => out.push(';'),
                Some('\\') => out.push('\\'),
                Some('n') | Some('N') => out.push('\n'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Escape text for emission.
pub fn escape_value(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for c in raw.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            other => out.push(other),
        }
    }
    out
}
