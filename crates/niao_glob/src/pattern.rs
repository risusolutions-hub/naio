//! Pattern utilities: magic detection, escape, translate.

use crate::error::GlobError;
use globset::{Glob, GlobBuilder, GlobMatcher};

/// Returns true when `pattern` contains glob metacharacters.
pub fn has_magic(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'?' | b'*' => return true,
            b'[' => return true,
            b'\\' if i + 1 < bytes.len() => i += 2,
            _ => i += 1,
        }
    }
    false
}

/// Escape glob metacharacters so the string matches literally.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '*' | '?' | '[' | ']' | '{' | '}' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

/// Translate a Unix fnmatch-style pattern to an anchored regex (Python `fnmatch.translate`).
pub fn translate(pattern: &str, case_sensitive: bool) -> Result<String, GlobError> {
    let glob = build_fnmatch_glob(pattern, case_sensitive)?;
    let mut re = glob.regex().to_string();
    if !re.starts_with("(?:") {
        re = format!("(?-u:{re})");
    }
    Ok(format!("(?s:{re})\\z"))
}

fn build_glob(
    pattern: &str,
    case_sensitive: bool,
    literal_separator: bool,
) -> Result<Glob, GlobError> {
    let mut builder = GlobBuilder::new(pattern);
    builder.case_insensitive(!case_sensitive);
    builder.literal_separator(literal_separator);
    builder.backslash_escape(true);
    Ok(builder.build()?)
}

/// Build a fnmatch-style glob (`*` crosses `/`).
pub fn build_fnmatch_glob(pattern: &str, case_sensitive: bool) -> Result<Glob, GlobError> {
    build_glob(pattern, case_sensitive, false)
}

/// Build a path glob (`*` does not cross `/`; `**` crosses directories).
pub fn build_path_glob_glob(pattern: &str, case_sensitive: bool) -> Result<Glob, GlobError> {
    build_glob(pattern, case_sensitive, true)
}

/// Compiled fnmatch matcher.
pub fn build_fnmatch(pattern: &str, case_sensitive: bool) -> Result<GlobMatcher, GlobError> {
    Ok(build_fnmatch_glob(pattern, case_sensitive)?.compile_matcher())
}

/// Compiled path-glob matcher.
pub fn build_path_glob(pattern: &str, case_sensitive: bool) -> Result<GlobMatcher, GlobError> {
    Ok(build_path_glob_glob(pattern, case_sensitive)?.compile_matcher())
}

/// Normalize path separators to `/` for matching.
pub fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

/// Basename of a path (normalized).
pub fn basename(path: &str) -> String {
    let path = normalize_path(path);
    path.rsplit('/').next().unwrap_or(path.as_str()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_magic_detects() {
        assert!(has_magic("*.py"));
        assert!(has_magic("a?b"));
        assert!(has_magic("a[0-9]"));
        assert!(!has_magic("plain.txt"));
    }

    #[test]
    fn escape_roundtrip_chars() {
        let e = escape("a*b?[x]");
        assert_eq!(e, r"a\*b\?\[x\]");
    }

    #[test]
    fn translate_ends_with_anchor() {
        let re = translate("*.py", true).unwrap();
        assert!(re.contains("\\z"));
    }
}
