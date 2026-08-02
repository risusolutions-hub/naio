//! Compiled matchers — glob sets and gitignore-style pathspecs.

use crate::error::GlobError;
use crate::pattern::{basename, build_path_glob, normalize_path};
use globset::{GlobMatcher, GlobSet, GlobSetBuilder};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use niao_parallel::map;
use std::path::PathBuf;

/// How a pathspec classifies a path (gitignore semantics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchKind {
    /// Explicitly included (negated rule or whitelist).
    Whitelist,
    /// Explicitly ignored.
    Ignore,
    /// No rule matched.
    None,
}

/// Options when compiling patterns.
#[derive(Debug, Clone)]
pub struct CompileOpts {
    pub gitignore: bool,
    pub case_sensitive: bool,
    pub root: PathBuf,
}

impl Default for CompileOpts {
    fn default() -> Self {
        Self {
            gitignore: false,
            case_sensitive: true,
            root: PathBuf::from("."),
        }
    }
}

enum MatcherInner {
    GlobSet(GlobSet),
    Gitignore(Gitignore),
    SinglePath(GlobMatcher),
}

/// Compiled multi-pattern matcher.
pub struct CompiledMatcher {
    inner: MatcherInner,
    gitignore: bool,
}

impl CompiledMatcher {
    pub fn is_gitignore(&self) -> bool {
        self.gitignore
    }

    pub fn pattern_count(&self) -> usize {
        match &self.inner {
            MatcherInner::GlobSet(gs) => gs.len(),
            MatcherInner::Gitignore(gi) => gi.num_ignores() as usize,
            MatcherInner::SinglePath(_) => 1,
        }
    }

    /// Match a path string. For gitignore matchers, returns classification.
    pub fn classify(&self, path: &str, is_dir: bool) -> MatchKind {
        let norm = normalize_path(path);
        match &self.inner {
            MatcherInner::GlobSet(gs) => {
                if gs.is_match(&norm) {
                    MatchKind::Whitelist
                } else {
                    MatchKind::None
                }
            }
            MatcherInner::Gitignore(gi) => match gi.matched(&norm, is_dir) {
                ignore::Match::None => MatchKind::None,
                ignore::Match::Ignore(_) => MatchKind::Ignore,
                ignore::Match::Whitelist(_) => MatchKind::Whitelist,
            },
            MatcherInner::SinglePath(g) => {
                if g.is_match(&norm) {
                    MatchKind::Whitelist
                } else {
                    MatchKind::None
                }
            }
        }
    }

    /// True when the path is considered a match for include-style use.
    pub fn matches(&self, path: &str) -> bool {
        self.matches_with(path, false)
    }

    pub fn matches_with(&self, path: &str, is_dir: bool) -> bool {
        match self.classify(path, is_dir) {
            MatchKind::Whitelist => true,
            MatchKind::Ignore => false,
            MatchKind::None => false,
        }
    }

    pub fn ignored(&self, path: &str, is_dir: bool) -> bool {
        matches!(self.classify(path, is_dir), MatchKind::Ignore)
    }

    pub fn included(&self, path: &str, is_dir: bool) -> bool {
        !self.ignored(path, is_dir)
    }

    pub fn matches_basename(&self, path: &str) -> bool {
        let base = basename(path);
        self.matches(&base)
    }

    pub fn filter<'a>(&self, paths: &'a [String]) -> Vec<&'a str> {
        paths
            .iter()
            .filter(|p| self.matches(p))
            .map(|s| s.as_str())
            .collect()
    }

    pub fn filter_owned(&self, paths: &[String]) -> Vec<String> {
        paths.iter().filter(|p| self.matches(p)).cloned().collect()
    }

    pub fn parallel_filter(&self, paths: &[String], threads: usize) -> Vec<String> {
        let hits: Vec<Option<String>> = map(paths, threads, |p| {
            if self.matches(p) {
                Some(p.clone())
            } else {
                None
            }
        });
        hits.into_iter().flatten().collect()
    }
}

/// Compile one or more patterns into a matcher.
pub fn compile(patterns: &[String], opts: &CompileOpts) -> Result<CompiledMatcher, GlobError> {
    if patterns.is_empty() {
        return Err(GlobError::InvalidPattern(
            "at least one pattern required".into(),
        ));
    }

    if opts.gitignore {
        let mut builder = GitignoreBuilder::new(&opts.root);
        for line in patterns {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            builder
                .add_line(None, trimmed)
                .map_err(|e| GlobError::InvalidPattern(e.to_string()))?;
        }
        let gi = builder
            .build()
            .map_err(|e| GlobError::InvalidPattern(e.to_string()))?;
        return Ok(CompiledMatcher {
            inner: MatcherInner::Gitignore(gi),
            gitignore: true,
        });
    }

    if patterns.len() == 1 {
        let matcher = build_path_glob(&patterns[0], opts.case_sensitive)?;
        return Ok(CompiledMatcher {
            inner: MatcherInner::SinglePath(matcher),
            gitignore: false,
        });
    }

    let mut builder = GlobSetBuilder::new();
    for pat in patterns {
        let trimmed = pat.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut gb = globset::GlobBuilder::new(trimmed);
        gb.case_insensitive(!opts.case_sensitive);
        gb.literal_separator(true);
        gb.backslash_escape(true);
        builder.add(gb.build()?);
    }
    let set = builder.build()?;
    Ok(CompiledMatcher {
        inner: MatcherInner::GlobSet(set),
        gitignore: false,
    })
}

/// Match any of several patterns (OR).
pub fn match_any(path: &str, patterns: &[String], case_sensitive: bool) -> Result<bool, GlobError> {
    let opts = CompileOpts {
        case_sensitive,
        ..Default::default()
    };
    let m = compile(patterns, &opts)?;
    Ok(m.matches(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globset_or() {
        let pats = vec!["*.rs".into(), "*.toml".into()];
        let m = compile(&pats, &CompileOpts::default()).unwrap();
        assert!(m.matches("lib.rs"));
        assert!(m.matches("Cargo.toml"));
        assert!(!m.matches("readme.md"));
    }

    #[test]
    fn gitignore_negation() {
        let pats = vec!["*.py".into(), "!tests/**".into()];
        let opts = CompileOpts {
            gitignore: true,
            ..Default::default()
        };
        let m = compile(&pats, &opts).unwrap();
        assert!(m.ignored("main.py", false));
        assert!(!m.ignored("tests/foo.py", false));
    }
}
