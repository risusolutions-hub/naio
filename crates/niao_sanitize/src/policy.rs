//! Default allowlists and URL scheme policy (~bleach / nh3 defaults).

use std::collections::{HashMap, HashSet};

/// Default HTML tags allowed by bleach/ammonia conservative profile.
pub fn default_tags() -> HashSet<String> {
    [
        "a",
        "abbr",
        "acronym",
        "address",
        "article",
        "aside",
        "b",
        "bdi",
        "bdo",
        "big",
        "blockquote",
        "br",
        "caption",
        "center",
        "cite",
        "code",
        "col",
        "colgroup",
        "data",
        "dd",
        "del",
        "details",
        "dfn",
        "div",
        "dl",
        "dt",
        "em",
        "figcaption",
        "figure",
        "footer",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "header",
        "hr",
        "i",
        "img",
        "ins",
        "kbd",
        "li",
        "main",
        "mark",
        "meter",
        "nav",
        "ol",
        "p",
        "pre",
        "progress",
        "q",
        "rp",
        "rt",
        "ruby",
        "s",
        "samp",
        "section",
        "small",
        "span",
        "strike",
        "strong",
        "sub",
        "summary",
        "sup",
        "table",
        "tbody",
        "td",
        "tfoot",
        "th",
        "thead",
        "time",
        "tr",
        "tt",
        "u",
        "ul",
        "var",
        "wbr",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect()
}

/// Per-tag attribute allowlist (bleach default).
pub fn default_tag_attributes() -> HashMap<String, HashSet<String>> {
    let mut map: HashMap<String, HashSet<String>> = HashMap::new();
    map.insert(
        "a".into(),
        ["href", "title", "name"]
            .iter()
            .map(|s| (*s).into())
            .collect(),
    );
    map.insert(
        "abbr".into(),
        ["title"].iter().map(|s| (*s).into()).collect(),
    );
    map.insert(
        "acronym".into(),
        ["title"].iter().map(|s| (*s).into()).collect(),
    );
    map.insert("bdo".into(), ["dir"].iter().map(|s| (*s).into()).collect());
    map.insert(
        "blockquote".into(),
        ["cite"].iter().map(|s| (*s).into()).collect(),
    );
    map.insert(
        "del".into(),
        ["cite", "datetime"].iter().map(|s| (*s).into()).collect(),
    );
    map.insert(
        "ins".into(),
        ["cite", "datetime"].iter().map(|s| (*s).into()).collect(),
    );
    map.insert(
        "img".into(),
        ["align", "alt", "height", "src", "title", "width"]
            .iter()
            .map(|s| (*s).into())
            .collect(),
    );
    map.insert(
        "ol".into(),
        ["start", "type"].iter().map(|s| (*s).into()).collect(),
    );
    map.insert("q".into(), ["cite"].iter().map(|s| (*s).into()).collect());
    map.insert(
        "table".into(),
        ["summary", "width"].iter().map(|s| (*s).into()).collect(),
    );
    map.insert(
        "td".into(),
        [
            "abbr", "align", "axis", "colspan", "rowspan", "valign", "width",
        ]
        .iter()
        .map(|s| (*s).into())
        .collect(),
    );
    map.insert(
        "th".into(),
        [
            "abbr", "align", "axis", "colspan", "rowspan", "scope", "valign", "width",
        ]
        .iter()
        .map(|s| (*s).into())
        .collect(),
    );
    map.insert(
        "time".into(),
        ["datetime"].iter().map(|s| (*s).into()).collect(),
    );
    map
}

/// URL schemes allowed in href/src by default.
pub fn default_protocols() -> HashSet<String> {
    ["http", "https", "mailto", "ftp"]
        .iter()
        .map(|s| (*s).into())
        .collect()
}

/// Relative URL handling mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RelativeUrlMode {
    #[default]
    PassThrough,
    Drop,
    Sanitize,
}

impl RelativeUrlMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pass" | "pass_through" | "allow" => Some(Self::PassThrough),
            "drop" | "deny" | "block" => Some(Self::Drop),
            "sanitize" | "rewrite" => Some(Self::Sanitize),
            _ => None,
        }
    }
}

/// Fast URL scheme policy check (no full URL parse).
///
/// Accepts `http://…`, `//host`, `mailto:…`, relative paths, and fragment-only refs.
pub fn allowed_url(url: &str, protocols: &HashSet<String>) -> bool {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return true;
    }
    if trimmed.starts_with('#')
        || trimmed.starts_with('/')
        || trimmed.starts_with("./")
        || trimmed.starts_with("../")
    {
        return true;
    }
    if trimmed.starts_with("//") {
        return protocols.contains("http") || protocols.contains("https");
    }
    let lower = trimmed.to_ascii_lowercase();
    if let Some(colon) = lower.find(':') {
        let scheme = &lower[..colon];
        if scheme == "javascript" || scheme == "data" || scheme == "vbscript" {
            return false;
        }
        return protocols.contains(scheme);
    }
    // Bare hostname or path without scheme — allow (relative).
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_javascript() {
        let p = default_protocols();
        assert!(!allowed_url("javascript:alert(1)", &p));
        assert!(!allowed_url("JavaScript:alert(1)", &p));
    }

    #[test]
    fn allows_https() {
        let p = default_protocols();
        assert!(allowed_url("https://example.com", &p));
    }

    #[test]
    fn allows_relative() {
        let p = default_protocols();
        assert!(allowed_url("/path", &p));
        assert!(allowed_url("#frag", &p));
    }
}
