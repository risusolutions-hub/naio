//! Filesystem glob expansion (~Python `glob.glob`).

use crate::error::GlobError;
use crate::matcher::{compile, CompileOpts};
use crate::pattern::{build_path_glob, normalize_path};
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

/// Options for filesystem glob.
#[derive(Debug, Clone)]
pub struct GlobOpts {
    pub root: PathBuf,
    pub recursive: bool,
    pub hidden: bool,
    pub case_sensitive: bool,
    pub follow_links: bool,
}

impl Default for GlobOpts {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            recursive: false,
            hidden: false,
            case_sensitive: true,
            follow_links: false,
        }
    }
}

fn pattern_needs_recursive(pattern: &str) -> bool {
    pattern.contains("**")
}

/// Expand `pattern` under `opts.root`, returning normalized forward-slash paths.
pub fn glob_paths(pattern: &str, opts: &GlobOpts) -> Result<Vec<String>, GlobError> {
    let pattern = normalize_path(pattern);
    if pattern.is_empty() {
        return Err(GlobError::InvalidPattern("empty pattern".into()));
    }

    let recursive = opts.recursive || pattern_needs_recursive(&pattern);
    let root = opts.root.clone();
    let root = if root.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        root
    };

    // Absolute pattern: match from filesystem root portion.
    if Path::new(&pattern).is_absolute() {
        return glob_absolute(&pattern, opts);
    }

    // Pattern contains directory components — anchor walk under root.
    if pattern.contains('/') {
        return glob_with_prefix(&root, &pattern, recursive, opts);
    }

    // Bare filename pattern — single directory unless recursive requested.
    if recursive {
        return walk_and_match(&root, &pattern, true, opts);
    }

    let dir = std::fs::read_dir(&root).map_err(GlobError::from)?;
    let glob = build_path_glob(&pattern, opts.case_sensitive)?;
    let mut out = Vec::new();
    for entry in dir {
        let entry = entry.map_err(GlobError::from)?;
        let ft = entry.file_type().map_err(GlobError::from)?;
        if ft.is_dir() {
            continue;
        }
        if !opts.hidden {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with('.') {
                    continue;
                }
            }
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if glob.is_match(name.as_ref()) {
            let rel = root.join(name.as_ref());
            out.push(path_to_string(&rel));
        }
    }
    out.sort();
    Ok(out)
}

fn glob_absolute(pattern: &str, opts: &GlobOpts) -> Result<Vec<String>, GlobError> {
    let norm = normalize_path(pattern);
    let (parent, file_pat) = split_dir_pattern(&norm);
    let parent_path = Path::new(parent);
    if !parent_path.exists() {
        return Ok(Vec::new());
    }
    let recursive = opts.recursive || pattern_needs_recursive(file_pat);
    walk_and_match(parent_path, file_pat, recursive, opts)
}

fn glob_with_prefix(
    root: &Path,
    pattern: &str,
    recursive: bool,
    opts: &GlobOpts,
) -> Result<Vec<String>, GlobError> {
    let norm = normalize_path(pattern);
    if let Some(idx) = norm.find("**") {
        let (prefix, rest) = norm.split_at(idx);
        let rest = rest.trim_start_matches("**/");
        let base = if prefix.is_empty() {
            root.to_path_buf()
        } else {
            root.join(prefix.trim_end_matches('/'))
        };
        if !base.exists() {
            return Ok(Vec::new());
        }
        if rest.is_empty() {
            return walk_all(&base, recursive, opts);
        }
        return walk_and_match(&base, rest, true, opts);
    }

    let (dir_part, file_pat) = split_dir_pattern(&norm);
    let base = if dir_part.is_empty() || dir_part == "." {
        root.to_path_buf()
    } else {
        root.join(dir_part)
    };
    if !base.exists() {
        return Ok(Vec::new());
    }
    if recursive {
        walk_and_match(&base, file_pat, true, opts)
    } else {
        let glob = build_path_glob(file_pat, opts.case_sensitive)?;
        let mut out = Vec::new();
        if base.is_dir() {
            let rd = std::fs::read_dir(&base).map_err(GlobError::from)?;
            for entry in rd {
                let entry = entry.map_err(GlobError::from)?;
                let ft = entry.file_type().map_err(GlobError::from)?;
                if !ft.is_file() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                if glob.is_match(&name) {
                    out.push(path_to_string(&entry.path()));
                }
            }
        }
        out.sort();
        Ok(out)
    }
}

fn split_dir_pattern(pattern: &str) -> (&str, &str) {
    if let Some(pos) = pattern.rfind('/') {
        let (dir, file) = pattern.split_at(pos);
        (dir, file.trim_start_matches('/'))
    } else {
        (".", pattern)
    }
}

fn walk_and_match(
    root: &Path,
    file_pattern: &str,
    recursive: bool,
    opts: &GlobOpts,
) -> Result<Vec<String>, GlobError> {
    let glob = build_path_glob(file_pattern, opts.case_sensitive)?;
    let mut builder = WalkBuilder::new(root);
    builder.hidden(!opts.hidden);
    builder.follow_links(opts.follow_links);
    if !recursive {
        builder.max_depth(Some(1));
    }
    let walker = builder.build();
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut out = Vec::new();
    for result in walker {
        let entry = result.map_err(|e| GlobError::Io(e.to_string()))?;
        if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            continue;
        }
        let path = entry.path();
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let name = rel.rsplit('/').next().unwrap_or(rel.as_str());
        if glob.is_match(name) || glob.is_match(&rel) {
            out.push(path_to_string(path));
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn walk_all(root: &Path, recursive: bool, opts: &GlobOpts) -> Result<Vec<String>, GlobError> {
    let mut builder = WalkBuilder::new(root);
    builder.hidden(!opts.hidden);
    builder.follow_links(opts.follow_links);
    if !recursive {
        builder.max_depth(Some(1));
    }
    let walker = builder.build();
    let mut out = Vec::new();
    for result in walker {
        let entry = result.map_err(|e| GlobError::Io(e.to_string()))?;
        if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            out.push(path_to_string(entry.path()));
        }
    }
    out.sort();
    Ok(out)
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Glob many patterns and return the union (sorted, deduped).
pub fn glob_many(patterns: &[String], opts: &GlobOpts) -> Result<Vec<String>, GlobError> {
    let mut all = Vec::new();
    for pat in patterns {
        let mut hits = glob_paths(pat, opts)?;
        all.append(&mut hits);
    }
    all.sort();
    all.dedup();
    Ok(all)
}

/// Match paths against compiled include globs (no filesystem walk).
pub fn paths_matching_globs(
    paths: &[String],
    patterns: &[String],
    case_sensitive: bool,
) -> Result<Vec<String>, GlobError> {
    let opts = CompileOpts {
        case_sensitive,
        ..Default::default()
    };
    let m = compile(patterns, &opts)?;
    Ok(m.filter_owned(paths))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_rs_in_crate() {
        let opts = GlobOpts {
            root: PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            recursive: true,
            ..Default::default()
        };
        let hits = glob_paths("**/*.rs", &opts).unwrap();
        assert!(hits.iter().any(|p| p.ends_with("lib.rs")));
    }

    #[test]
    fn glob_nonexistent_empty() {
        let opts = GlobOpts {
            root: PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            ..Default::default()
        };
        let hits = glob_paths("no_such_dir_nglob/*.xyz", &opts).unwrap();
        assert!(hits.is_empty());
    }
}
