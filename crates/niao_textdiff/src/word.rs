//! Word-level inline diff.

use crate::error::TextDiffError;
use crate::opts::DiffOpts;
use crate::split::check_pair;
use similar::{ChangeTag, TextDiff};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordChange {
    pub tag: String,
    pub value: String,
}

pub fn word_diff(a: &str, b: &str, opts: &DiffOpts) -> Result<Vec<WordChange>, TextDiffError> {
    check_pair(a, b)?;
    let diff = TextDiff::configure()
        .algorithm(opts.algorithm)
        .diff_unicode_words(a, b);
    Ok(diff
        .iter_all_changes()
        .map(|c| WordChange {
            tag: match c.tag() {
                ChangeTag::Equal => "equal".into(),
                ChangeTag::Delete => "delete".into(),
                ChangeTag::Insert => "insert".into(),
            },
            value: c.value().to_string(),
        })
        .collect())
}

pub fn word_diff_inline(a: &str, b: &str, opts: &DiffOpts) -> Result<String, TextDiffError> {
    check_pair(a, b)?;
    let diff = TextDiff::configure()
        .algorithm(opts.algorithm)
        .diff_unicode_words(a, b);
    let mut out = String::new();
    for change in diff.iter_all_changes() {
        let value = change.value();
        match change.tag() {
            ChangeTag::Equal => out.push_str(value),
            ChangeTag::Delete => {
                out.push('{');
                out.push('-');
                out.push_str(value);
                out.push('}');
            }
            ChangeTag::Insert => {
                out.push('{');
                out.push('+');
                out.push_str(value);
                out.push('}');
            }
        }
    }
    Ok(out)
}
