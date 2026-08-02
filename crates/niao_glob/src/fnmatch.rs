//! Unix fnmatch-style matching (~Python `fnmatch`).

use crate::error::GlobError;
use crate::pattern::{build_fnmatch, normalize_path};
use niao_parallel::map;

/// Match `name` against `pattern` (case-sensitive by default).
pub fn match_str(name: &str, pattern: &str, case_sensitive: bool) -> Result<bool, GlobError> {
    let glob = build_fnmatch(pattern, case_sensitive)?;
    Ok(glob.is_match(name))
}

/// Match only the basename of `path`.
pub fn match_basename(path: &str, pattern: &str, case_sensitive: bool) -> Result<bool, GlobError> {
    let base = crate::pattern::basename(path);
    match_str(&base, pattern, case_sensitive)
}

/// Filter string paths that match `pattern`.
pub fn filter_strs<'a>(
    names: &'a [String],
    pattern: &str,
    case_sensitive: bool,
) -> Result<Vec<&'a str>, GlobError> {
    let glob = build_fnmatch(pattern, case_sensitive)?;
    Ok(names
        .iter()
        .filter(|n| glob.is_match(n.as_str()))
        .map(|s| s.as_str())
        .collect())
}

/// Parallel filter of many paths against one pattern.
pub fn parallel_filter(
    paths: &[String],
    pattern: &str,
    case_sensitive: bool,
    threads: usize,
) -> Result<Vec<String>, GlobError> {
    let glob = build_fnmatch(pattern, case_sensitive)?;
    let hits: Vec<Option<String>> = map(paths, threads, |p| {
        if glob.is_match(p) {
            Some(p.clone())
        } else {
            None
        }
    });
    Ok(hits.into_iter().flatten().collect())
}

/// Filter with path normalization (forward slashes).
pub fn filter_paths_normalized(
    paths: &[String],
    pattern: &str,
    case_sensitive: bool,
) -> Result<Vec<String>, GlobError> {
    let glob = build_fnmatch(pattern, case_sensitive)?;
    Ok(paths
        .iter()
        .filter(|p| glob.is_match(&normalize_path(p)))
        .cloned()
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_wildcards() {
        assert!(match_str("foo.py", "*.py", true).unwrap());
        assert!(!match_str("foo.txt", "*.py", true).unwrap());
        assert!(match_str("a", "?", true).unwrap());
        assert!(!match_str("ab", "?", true).unwrap());
    }

    #[test]
    fn case_insensitive() {
        assert!(match_str("Foo.py", "*.py", false).unwrap());
        // Extension match is case-insensitive on Windows paths in globset.
        #[cfg(not(windows))]
        assert!(!match_str("Foo.py", "*.py", true).unwrap());
    }

    #[test]
    fn char_class() {
        assert!(match_str("a9", "a[0-9]", true).unwrap());
        assert!(!match_str("ax", "a[0-9]", true).unwrap());
    }

    #[test]
    fn star_crosses_slash_fnmatch() {
        assert!(match_str("a/b/c.txt", "*/*.txt", true).unwrap());
    }
}
