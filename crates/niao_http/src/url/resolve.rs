//! RFC 3986 relative resolution (WHATWG URL resolution subset).

use super::parse::parse_url;
use super::Url;

/// Normalize a path by removing `.` segments and resolving `..`.
pub fn normalize_path(path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut segments: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if absolute {
                    segments.pop();
                } else if !segments.is_empty() {
                    segments.pop();
                }
            }
            other => segments.push(other),
        }
    }
    if absolute {
        if segments.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", segments.join("/"))
        }
    } else if segments.is_empty() {
        String::new()
    } else {
        segments.join("/")
    }
}

fn merge_paths(base_path: &str, reference: &str) -> String {
    if reference.starts_with('/') {
        return normalize_path(reference);
    }
    let base_dir = base_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let merged = if base_dir.is_empty() {
        reference.to_string()
    } else {
        format!("{base_dir}/{reference}")
    };
    normalize_path(&format!("/{merged}"))
        .trim_start_matches('/')
        .to_string()
}

/// Resolve `reference` against `base` (RFC 3986 section 5.2).
pub fn resolve(base: &Url, reference: &str) -> Result<Url, String> {
    let reference = reference.trim_matches(|c: char| {
        matches!(c, '\u{0009}' | '\u{000A}' | '\u{000D}' | ' ')
    });
    if reference.is_empty() {
        return Ok(base.clone());
    }

    if let Some(scheme_end) = reference.find(':') {
        if scheme_end > 0
            && reference[..scheme_end]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
            && reference[scheme_end + 1..].starts_with("//")
        {
            return parse_url(reference);
        }
    }

    if reference.starts_with("//") {
        return parse_url(&format!("{}:{}", base.scheme, reference));
    }

    let mut out = base.clone();

    if reference.starts_with('#') {
        out.fragment = reference[1..].to_string();
        return Ok(out);
    }

    let (ref_path_query, fragment) = match reference.find('#') {
        Some(i) => (&reference[..i], reference[i + 1..].to_string()),
        None => (reference, String::new()),
    };

    let (ref_path, query) = match ref_path_query.find('?') {
        Some(i) => (&ref_path_query[..i], ref_path_query[i + 1..].to_string()),
        None => (ref_path_query, String::new()),
    };

    if ref_path.starts_with('?') {
        out.query = ref_path[1..].to_string();
        out.fragment = fragment;
        return Ok(out);
    }

    if !ref_path.is_empty() {
        if ref_path.starts_with('/') {
            out.path = normalize_path(ref_path);
        } else {
            let merged = merge_paths(&base.path, ref_path);
            out.path = if merged.is_empty() {
                "/".to_string()
            } else if merged.starts_with('/') {
                merged
            } else {
                format!("/{merged}")
            };
        }
        if !query.is_empty() {
            out.query = query;
        } else if !ref_path_query.contains('?') {
            out.query.clear();
        }
    } else if !query.is_empty() {
        out.query = query;
    }

    out.fragment = fragment;
    Ok(out)
}

/// Free-function alias used by legacy call sites.
pub fn join(base: &Url, reference: &str) -> Result<Url, String> {
    resolve(base, reference)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::url::parse_url;

    #[test]
    fn rfc3986_examples() {
        let base = parse_url("http://a/b/c/d;p?q").unwrap();
        let cases = [
            ("g", "/b/c/g"),
            ("./g", "/b/c/g"),
            ("../g", "/b/g"),
            ("../g?y", "/b/g"),
            ("#s", "/b/c/d;p"),
            ("?y", "/b/c/d;p"),
            ("/g", "/g"),
        ];
        for (r, want_path) in cases {
            let got = resolve(&base, r).unwrap();
            assert_eq!(got.path, want_path, "ref={r}");
        }
    }

    #[test]
    fn join_relative_file() {
        let base = parse_url("https://example.com/a/b/").unwrap();
        let got = join(&base, "c").unwrap();
        assert_eq!(got.path, "/a/b/c");
        assert_eq!(got.origin(), "https://example.com");
    }

    #[test]
    fn fragment_only() {
        let base = parse_url("http://ex/x").unwrap();
        let got = join(&base, "#frag").unwrap();
        assert_eq!(got.fragment, "frag");
        assert_eq!(got.path, "/x");
    }

    #[test]
    fn absolute_override() {
        let base = parse_url("http://a/old").unwrap();
        let got = join(&base, "https://b/new").unwrap();
        assert_eq!(got.host, "b");
        assert_eq!(got.path, "/new");
    }
}
