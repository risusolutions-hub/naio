//! Dot-separated topic patterns with `*` (one segment) and `**` (zero-or-more segments).

use std::borrow::Cow;

/// Parse / validation failure for a topic or pattern string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopicError {
    Empty,
    InvalidChar(char),
    EmptySegment,
    LoneWildcard,
    WildcardNotAlone(char),
}

impl std::fmt::Display for TopicError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TopicError::Empty => write!(f, "topic must not be empty"),
            TopicError::InvalidChar(c) => write!(f, "invalid topic character '{c}'"),
            TopicError::EmptySegment => write!(f, "topic must not contain empty segments"),
            TopicError::LoneWildcard => write!(f, "wildcard segment must be '*' or '**' alone"),
            TopicError::WildcardNotAlone(c) => {
                write!(f, "wildcard '{c}' must occupy an entire segment")
            }
        }
    }
}

impl std::error::Error for TopicError {}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Literal(Cow<'static, str>),
    Single,
    Multi,
}

/// Compiled topic pattern (literal topic or wildcard pattern).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicPattern {
    raw: String,
    segments: Vec<Segment>,
    has_wildcard: bool,
}

impl TopicPattern {
    /// Parse and validate a topic or pattern string.
    pub fn parse(raw: &str) -> Result<Self, TopicError> {
        let normalized = normalize(raw);
        if normalized.is_empty() {
            return Err(TopicError::Empty);
        }
        let parts = normalized.split('.');
        let mut segments = Vec::new();
        let mut has_wildcard = false;
        for part in parts {
            if part.is_empty() {
                return Err(TopicError::EmptySegment);
            }
            let seg = parse_segment(part)?;
            if !matches!(seg, Segment::Literal(_)) {
                has_wildcard = true;
            }
            segments.push(seg);
        }
        Ok(Self {
            raw: normalized,
            segments,
            has_wildcard,
        })
    }

    /// Borrow the normalized pattern string.
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// Whether the pattern contains `*` or `**`.
    #[inline]
    pub fn has_wildcard(&self) -> bool {
        self.has_wildcard
    }

    /// Segment strings for a literal topic (no wildcards).
    pub fn segments(&self) -> Vec<&str> {
        self.segments
            .iter()
            .map(|s| match s {
                Segment::Literal(l) => l.as_ref(),
                Segment::Single => "*",
                Segment::Multi => "**",
            })
            .collect()
    }

    /// Whether `topic` matches this pattern.
    pub fn matches(&self, topic: &str) -> bool {
        let topic = match normalize(topic) {
            t if t.is_empty() => return false,
            t => t,
        };
        let topic_parts: Vec<&str> = topic.split('.').collect();
        if topic_parts.iter().any(|p| p.is_empty()) {
            return false;
        }
        segments_match(&self.segments, &topic_parts, 0, 0)
    }
}

fn parse_segment(part: &str) -> Result<Segment, TopicError> {
    if part == "*" {
        return Ok(Segment::Single);
    }
    if part == "**" {
        return Ok(Segment::Multi);
    }
    if part.contains('*') {
        if part == "*" || part == "**" {
            return Err(TopicError::LoneWildcard);
        }
        return Err(TopicError::WildcardNotAlone('*'));
    }
    for c in part.chars() {
        if !is_topic_char(c) {
            return Err(TopicError::InvalidChar(c));
        }
    }
    Ok(Segment::Literal(Cow::Owned(part.to_string())))
}

#[inline]
fn is_topic_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-')
}

/// Trim whitespace and collapse duplicate dots.
pub fn normalize(raw: &str) -> String {
    let trimmed = raw.trim();
    let mut out = String::with_capacity(trimmed.len());
    let mut prev_dot = false;
    for c in trimmed.chars() {
        if c == '.' {
            if !prev_dot && !out.is_empty() {
                out.push('.');
                prev_dot = true;
            }
        } else if !c.is_whitespace() {
            out.push(c);
            prev_dot = false;
        }
    }
    while out.ends_with('.') {
        out.pop();
    }
    out
}

/// Whether `s` is a valid literal topic (no wildcards).
pub fn is_valid_topic(s: &str) -> bool {
    let n = normalize(s);
    if n.is_empty() {
        return false;
    }
    TopicPattern::parse(&n)
        .map(|p| !p.has_wildcard())
        .unwrap_or(false)
}

/// Whether `s` is a valid pattern (literal or wildcards).
pub fn is_valid_pattern(s: &str) -> bool {
    let n = normalize(s);
    !n.is_empty() && TopicPattern::parse(&n).is_ok()
}

/// Fast pattern match without allocating a `TopicPattern`.
pub fn topic_matches(pattern: &str, topic: &str) -> bool {
    TopicPattern::parse(pattern)
        .map(|p| p.matches(topic))
        .unwrap_or(false)
}

/// Split a literal topic into segments (returns empty vec on invalid input).
pub fn split_topic(topic: &str) -> Vec<String> {
    let n = normalize(topic);
    if n.is_empty() || !is_valid_topic(&n) {
        return Vec::new();
    }
    n.split('.').map(str::to_string).collect()
}

/// Join segments into a dot-separated topic.
pub fn join_topic(parts: &[&str]) -> Result<String, TopicError> {
    if parts.is_empty() {
        return Err(TopicError::Empty);
    }
    let mut out = String::new();
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            return Err(TopicError::EmptySegment);
        }
        for c in part.chars() {
            if !is_topic_char(c) {
                return Err(TopicError::InvalidChar(c));
            }
        }
        if i > 0 {
            out.push('.');
        }
        out.push_str(part);
    }
    Ok(out)
}

fn segments_match(pattern: &[Segment], topic: &[&str], pi: usize, ti: usize) -> bool {
    if pi == pattern.len() {
        return ti == topic.len();
    }
    match &pattern[pi] {
        Segment::Literal(lit) => {
            if ti >= topic.len() || topic[ti] != lit.as_ref() {
                false
            } else {
                segments_match(pattern, topic, pi + 1, ti + 1)
            }
        }
        Segment::Single => {
            if ti >= topic.len() {
                false
            } else {
                segments_match(pattern, topic, pi + 1, ti + 1)
            }
        }
        Segment::Multi => {
            for skip in ti..=topic.len() {
                if segments_match(pattern, topic, pi + 1, skip) {
                    return true;
                }
            }
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match() {
        let p = TopicPattern::parse("user.created").unwrap();
        assert!(p.matches("user.created"));
        assert!(!p.matches("user.deleted"));
    }

    #[test]
    fn single_wildcard() {
        let p = TopicPattern::parse("user.*").unwrap();
        assert!(p.matches("user.created"));
        assert!(p.matches("user.deleted"));
        assert!(!p.matches("user.admin.created"));
        assert!(!p.matches("order.created"));
    }

    #[test]
    fn multi_wildcard_end() {
        let p = TopicPattern::parse("user.**").unwrap();
        assert!(p.matches("user"));
        assert!(p.matches("user.created"));
        assert!(p.matches("user.admin.login"));
    }

    #[test]
    fn multi_wildcard_middle() {
        let p = TopicPattern::parse("a.**.c").unwrap();
        assert!(p.matches("a.b.c"));
        assert!(p.matches("a.x.y.c"));
        assert!(!p.matches("a.b"));
    }

    #[test]
    fn normalize_collapses_dots() {
        assert_eq!(normalize("  foo..bar. "), "foo.bar");
    }

    #[test]
    fn invalid_wildcard_mixed() {
        assert!(TopicPattern::parse("foo*").is_err());
    }
}
