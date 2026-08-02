//! Directory walking with include/exclude filters and optional gitignore.

use crate::error::GlobError;
use crate::matcher::{compile, CompileOpts, MatchKind};
use ignore::WalkBuilder;
use niao_parallel::map;
use std::path::{Path, PathBuf};

/// One entry from a directory walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkEntry {
    pub path: String,
    pub is_dir: bool,
    pub depth: usize,
}

/// Options for `walk`.
#[derive(Debug, Clone)]
pub struct WalkOpts {
    pub root: PathBuf,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub gitignore: bool,
    pub hidden: bool,
    pub max_depth: Option<usize>,
    pub follow_links: bool,
    pub files_only: bool,
    pub case_sensitive: bool,
    pub threads: usize,
}

impl Default for WalkOpts {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            include: Vec::new(),
            exclude: Vec::new(),
            gitignore: true,
            hidden: false,
            max_depth: None,
            follow_links: false,
            files_only: true,
            case_sensitive: true,
            threads: niao_parallel::available_threads(),
        }
    }
}

/// Walk `root` and return matching entries.
pub fn walk(opts: &WalkOpts) -> Result<Vec<WalkEntry>, GlobError> {
    let root = if opts.root.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        opts.root.clone()
    };
    if !root.exists() {
        return Err(GlobError::Io(format!(
            "root does not exist: {}",
            root.display()
        )));
    }

    let include_matcher = if opts.include.is_empty() {
        None
    } else {
        Some(compile(
            &opts.include,
            &CompileOpts {
                gitignore: false,
                case_sensitive: opts.case_sensitive,
                root: root.clone(),
            },
        )?)
    };

    let exclude_matcher = if opts.exclude.is_empty() {
        None
    } else {
        Some(compile(
            &opts.exclude,
            &CompileOpts {
                gitignore: opts.gitignore,
                case_sensitive: opts.case_sensitive,
                root: root.clone(),
            },
        )?)
    };

    let mut builder = WalkBuilder::new(&root);
    builder.hidden(!opts.hidden);
    builder.follow_links(opts.follow_links);
    builder.git_ignore(opts.gitignore);
    builder.git_global(opts.gitignore);
    builder.git_exclude(opts.gitignore);
    if let Some(d) = opts.max_depth {
        builder.max_depth(Some(d));
    }

    let walker = builder.build();
    let root_canon = root.canonicalize().unwrap_or_else(|_| root.clone());
    let mut out = Vec::new();

    for result in walker {
        let entry = result.map_err(|e| GlobError::Io(e.to_string()))?;
        let path = entry.path();
        let rel = path
            .strip_prefix(&root_canon)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let rel = if rel.is_empty() { ".".to_string() } else { rel };

        let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        if opts.files_only && is_dir {
            continue;
        }

        if let Some(ref ex) = exclude_matcher {
            let skip = if ex.is_gitignore() {
                matches!(ex.classify(&rel, is_dir), MatchKind::Ignore)
            } else {
                ex.matches_with(&rel, is_dir)
            };
            if skip {
                continue;
            }
        }

        if let Some(ref inc) = include_matcher {
            if !inc.matches_with(&rel, is_dir) {
                continue;
            }
        }

        let depth = rel.matches('/').count();
        out.push(WalkEntry {
            path: path_to_string(path),
            is_dir,
            depth,
        });
    }

    Ok(out)
}

/// Return only path strings from a walk.
pub fn walk_paths(opts: &WalkOpts) -> Result<Vec<String>, GlobError> {
    Ok(walk(opts)?.into_iter().map(|e| e.path).collect())
}

/// Parallel classify of many paths against a compiled matcher (paths already on disk).
pub fn parallel_classify(
    paths: &[String],
    patterns: &[String],
    gitignore: bool,
    case_sensitive: bool,
    threads: usize,
) -> Result<Vec<String>, GlobError> {
    let m = compile(
        patterns,
        &CompileOpts {
            gitignore,
            case_sensitive,
            root: PathBuf::from("."),
        },
    )?;
    let hits: Vec<Option<String>> = map(paths, threads, |p| {
        if m.matches(p) {
            Some(p.clone())
        } else {
            None
        }
    });
    Ok(hits.into_iter().flatten().collect())
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_crate_rs_files() {
        let opts = WalkOpts {
            root: PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            include: vec!["**/*.rs".into()],
            gitignore: false,
            ..Default::default()
        };
        let entries = walk(&opts).unwrap();
        assert!(!entries.is_empty());
        assert!(entries.iter().all(|e| e.path.ends_with(".rs")));
    }
}
