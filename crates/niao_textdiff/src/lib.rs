//! Line/word text diff, unified patches, 3-way merge.

mod char;
mod error;
mod line;
mod matcher;
mod merge;
mod opts;
mod parallel;
mod patch;
mod restore;
mod split;
mod word;

pub use char::{char_diff, char_diff_raw, levenshtein, CharChange};
pub use error::TextDiffError;
pub use line::{
    compare, compare_joined, context, context_joined, line_changes, unified, unified_joined, Change,
};
pub use matcher::{
    matching_blocks, opcodes, parse_algorithm, quick_ratio, ratio, real_quick_ratio, MatchBlock,
    Matcher, Opcode,
};
pub use merge::{merge, MergeConflict, MergeOpts, MergeResult};
pub use opts::{DiffOpts, Granularity};
pub use parallel::{parallel_diff, parallel_ratio, parallel_unified, DiffPair, ParallelDiffResult};
pub use patch::{
    diff_to_texts, patch_apply, patch_from_diffs, patch_make, patch_make_dmp, PatchApplyResult,
};
pub use restore::restore;
pub use split::{
    check_input_len, check_pair, check_triple, join_output, splitlines, MAX_INPUT_BYTES,
};
pub use word::{word_diff, word_diff_inline, WordChange};
