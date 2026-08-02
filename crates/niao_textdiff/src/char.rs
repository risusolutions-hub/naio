//! Character-level diff (diff-match-patch subset).

use crate::error::TextDiffError;
use crate::opts::DiffOpts;
use crate::split::check_pair;
use diff_match_patch::{Diff, Dmp};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharChange {
    /// `-1` delete, `0` equal, `1` insert
    pub op: i32,
    pub text: String,
}

pub fn char_diff(a: &str, b: &str, _opts: &DiffOpts) -> Result<Vec<CharChange>, TextDiffError> {
    check_pair(a, b)?;
    let mut dmp = Dmp::new();
    dmp.diff_timeout = None;
    let mut diffs = dmp.diff_main(a, b, false);
    dmp.diff_cleanup_semantic(&mut diffs);
    Ok(map_diffs(diffs))
}

pub fn char_diff_raw(a: &str, b: &str, _opts: &DiffOpts) -> Result<Vec<CharChange>, TextDiffError> {
    check_pair(a, b)?;
    let mut dmp = Dmp::new();
    dmp.diff_timeout = None;
    let diffs = dmp.diff_main(a, b, false);
    Ok(map_diffs(diffs))
}

pub fn levenshtein(a: &str, b: &str) -> Result<usize, TextDiffError> {
    check_pair(a, b)?;
    let mut dmp = Dmp::new();
    let diffs = dmp.diff_main(a, b, false);
    Ok(dmp.diff_levenshtein(&diffs) as usize)
}

fn map_diffs(diffs: Vec<Diff>) -> Vec<CharChange> {
    diffs
        .into_iter()
        .map(|d| CharChange {
            op: d.operation,
            text: d.text,
        })
        .collect()
}
