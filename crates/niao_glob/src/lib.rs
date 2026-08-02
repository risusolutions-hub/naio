//! Glob patterns, `**` recursion, gitignore-style matching, walk with filters.

mod error;
mod fnmatch;
mod glob_fs;
mod matcher;
mod pattern;
mod walk;

pub use error::GlobError;
pub use fnmatch::{
    filter_paths_normalized, filter_strs, match_basename, match_str, parallel_filter,
};
pub use glob_fs::{glob_many, glob_paths, paths_matching_globs, GlobOpts};
pub use matcher::{compile, match_any, CompileOpts, CompiledMatcher, MatchKind};
pub use pattern::{
    basename, build_fnmatch, build_fnmatch_glob, build_path_glob, build_path_glob_glob, escape,
    has_magic, normalize_path, translate,
};
pub use walk::{parallel_classify, walk, walk_paths, WalkEntry, WalkOpts};
