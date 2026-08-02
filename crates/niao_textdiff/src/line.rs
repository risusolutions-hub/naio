//! Line-level diff: compare, unified, context, ndiff.

use crate::error::TextDiffError;
use crate::opts::DiffOpts;
use crate::split::{check_pair, join_output, normalize_lines, splitlines};
use similar::{ChangeTag, TextDiff};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub tag: String,
    pub value: String,
}

fn make_diff<'a>(old: &'a [&'a str], new: &'a [&'a str], opts: &DiffOpts) -> TextDiff<'a, 'a, str> {
    TextDiff::configure()
        .algorithm(opts.algorithm)
        .diff_slices(old, new)
}

fn line_vecs(
    a: &str,
    b: &str,
    opts: &DiffOpts,
) -> Result<(Vec<String>, Vec<String>, Vec<String>, Vec<String>), TextDiffError> {
    check_pair(a, b)?;
    let old = splitlines(a, false);
    let new = splitlines(b, false);
    let norm_old = normalize_lines(&old, opts);
    let norm_new = normalize_lines(&new, opts);
    Ok((old, new, norm_old, norm_new))
}

fn refs(lines: &[String]) -> Vec<&str> {
    lines.iter().map(|s| s.as_str()).collect()
}

fn line_at(lines: &[String], idx: Option<usize>, fallback: &str) -> String {
    idx.and_then(|i| lines.get(i))
        .cloned()
        .unwrap_or_else(|| fallback.to_string())
}

/// ndiff-style line compare (difflib.Differ.compare).
pub fn compare(a: &str, b: &str, opts: &DiffOpts) -> Result<Vec<String>, TextDiffError> {
    let (old, new, norm_old, norm_new) = line_vecs(a, b, opts)?;
    let old_refs = refs(&norm_old);
    let new_refs = refs(&norm_new);
    let diff = make_diff(&old_refs, &new_refs, opts);
    let mut out = Vec::new();
    for change in diff.iter_all_changes() {
        let prefix = match change.tag() {
            ChangeTag::Equal => "  ",
            ChangeTag::Delete => "- ",
            ChangeTag::Insert => "+ ",
        };
        let value = match change.tag() {
            ChangeTag::Equal | ChangeTag::Delete => {
                line_at(&old, change.old_index(), change.value())
            }
            ChangeTag::Insert => line_at(&new, change.new_index(), change.value()),
        };
        out.push(format!("{prefix}{value}"));
    }
    Ok(out)
}

/// Unified diff (difflib.unified_diff).
pub fn unified(a: &str, b: &str, opts: &DiffOpts) -> Result<Vec<String>, TextDiffError> {
    let (_, _, norm_old, norm_new) = line_vecs(a, b, opts)?;
    let old_refs = refs(&norm_old);
    let new_refs = refs(&norm_new);
    let diff = make_diff(&old_refs, &new_refs, opts);
    let from = if opts.fromfile.is_empty() {
        ""
    } else {
        &opts.fromfile
    };
    let to = if opts.tofile.is_empty() {
        ""
    } else {
        &opts.tofile
    };
    let text = diff
        .unified_diff()
        .context_radius(opts.context)
        .header(from, to)
        .to_string();
    if text.is_empty() {
        return Ok(Vec::new());
    }
    Ok(text.lines().map(|l| l.to_string()).collect())
}

/// Context diff (difflib.context_diff).
pub fn context(a: &str, b: &str, opts: &DiffOpts) -> Result<Vec<String>, TextDiffError> {
    let (old, new, norm_old, norm_new) = line_vecs(a, b, opts)?;
    let old_refs = refs(&norm_old);
    let new_refs = refs(&norm_new);
    let diff = make_diff(&old_refs, &new_refs, opts);
    let n = opts.context.max(1);
    let groups = diff.grouped_ops(n);
    let mut out = Vec::new();
    out.push("***************".into());
    out.push(format!(
        "*** {} {} *** {} {} ***",
        opts.fromfile, opts.fromfiledate, opts.tofile, opts.tofiledate
    ));
    for group in groups {
        if group.is_empty() {
            continue;
        }
        let first = &group[0];
        let last = group.last().unwrap();
        let i1 = first.old_range().start;
        let i2 = last.old_range().end;
        let j1 = first.new_range().start;
        let j2 = last.new_range().end;
        out.push(format!("*** {},{} ****", i1 + 1, i2.max(i1 + 1)));
        out.push(format!("--- {},{} ----", j1 + 1, j2.max(j1 + 1)));
        for op in &group {
            for change in diff.iter_changes(op) {
                let line = match change.tag() {
                    ChangeTag::Equal | ChangeTag::Delete => {
                        line_at(&old, change.old_index(), change.value())
                    }
                    ChangeTag::Insert => line_at(&new, change.new_index(), change.value()),
                };
                let prefix = match change.tag() {
                    ChangeTag::Equal => "  ",
                    ChangeTag::Delete => "- ",
                    ChangeTag::Insert => "+ ",
                };
                out.push(format!("{prefix}{line}"));
            }
        }
    }
    if out.len() <= 2 && old == new {
        out.clear();
    }
    Ok(out)
}

/// Structured line changes.
pub fn line_changes(a: &str, b: &str, opts: &DiffOpts) -> Result<Vec<Change>, TextDiffError> {
    let (_, _, norm_old, norm_new) = line_vecs(a, b, opts)?;
    let old_refs = refs(&norm_old);
    let new_refs = refs(&norm_new);
    let diff = make_diff(&old_refs, &new_refs, opts);
    Ok(diff
        .iter_all_changes()
        .map(|c| Change {
            tag: match c.tag() {
                ChangeTag::Equal => "equal".into(),
                ChangeTag::Delete => "delete".into(),
                ChangeTag::Insert => "insert".into(),
            },
            value: c.value().to_string(),
        })
        .collect())
}

pub fn compare_joined(a: &str, b: &str, opts: &DiffOpts) -> Result<String, TextDiffError> {
    Ok(join_output(&compare(a, b, opts)?, opts))
}

pub fn unified_joined(a: &str, b: &str, opts: &DiffOpts) -> Result<String, TextDiffError> {
    Ok(join_output(&unified(a, b, opts)?, opts))
}

pub fn context_joined(a: &str, b: &str, opts: &DiffOpts) -> Result<String, TextDiffError> {
    Ok(join_output(&context(a, b, opts)?, opts))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unified_diff_basic() {
        let opts = DiffOpts::default();
        let lines = unified("a\nb\nc\n", "a\nx\nc\n", &opts).unwrap();
        assert!(lines.iter().any(|l| l.starts_with("@@")));
        assert!(lines.iter().any(|l| l.starts_with("-b")));
        assert!(lines.iter().any(|l| l.starts_with("+x")));
    }

    #[test]
    fn compare_ndiff() {
        let opts = DiffOpts::default();
        let lines = compare("a\nb\n", "a\nc\n", &opts).unwrap();
        assert!(lines.iter().any(|l| l.starts_with("- b")));
        assert!(lines.iter().any(|l| l.starts_with("+ c")));
    }
}
