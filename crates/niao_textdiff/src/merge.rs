//! Three-way line merge (merge3 / git-style subset).

use crate::error::TextDiffError;
use crate::matcher::Matcher;
use crate::opts::{DiffOpts, Granularity};
use crate::split::{check_triple, splitlines};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeConflict {
    pub start: usize,
    pub end: usize,
    pub base: String,
    pub ours: String,
    pub theirs: String,
}

#[derive(Debug, Clone)]
pub struct MergeResult {
    pub merged: String,
    pub conflicts: Vec<MergeConflict>,
}

#[derive(Debug, Clone)]
pub struct MergeOpts {
    pub diff: DiffOpts,
    pub marker_ours: String,
    pub marker_base: String,
    pub marker_theirs: String,
    pub marker_end: String,
}

impl Default for MergeOpts {
    fn default() -> Self {
        Self {
            diff: DiffOpts::default(),
            marker_ours: "<<<<<<< ours".into(),
            marker_base: "||||||| base".into(),
            marker_theirs: "======= theirs".into(),
            marker_end: ">>>>>>>".into(),
        }
    }
}

#[derive(Debug, Clone)]
struct Edit {
    base_start: usize,
    base_end: usize,
    lines: Vec<String>,
}

/// Merge `base`, `ours`, and `theirs` at line granularity.
pub fn merge(
    base: &str,
    ours: &str,
    theirs: &str,
    opts: &MergeOpts,
) -> Result<MergeResult, TextDiffError> {
    check_triple(base, ours, theirs)?;
    let base_lines = splitlines(base, false);
    let ours_lines = splitlines(ours, false);
    let theirs_lines = splitlines(theirs, false);

    let base_s = base_lines.join("\n");
    let ours_s = ours_lines.join("\n");
    let theirs_s = theirs_lines.join("\n");

    let ours_edits = edits_from(&base_s, &ours_s, &ours_lines, &opts.diff)?;
    let theirs_edits = edits_from(&base_s, &theirs_s, &theirs_lines, &opts.diff)?;

    let mut merged = Vec::new();
    let mut conflicts = Vec::new();
    let mut i = 0usize;
    let mut oi = 0usize;
    let mut ti = 0usize;

    while i < base_lines.len() {
        let oe = ours_edits.get(oi);
        let te = theirs_edits.get(ti);
        let o_start = oe.map(|e| e.base_start).unwrap_or(base_lines.len());
        let t_start = te.map(|e| e.base_start).unwrap_or(base_lines.len());

        if o_start > i && t_start > i {
            let next = o_start.min(t_start);
            merged.extend(base_lines[i..next].iter().cloned());
            i = next;
            continue;
        }

        if o_start == i && t_start == i {
            match (oe, te) {
                (Some(o), Some(t)) => {
                    if o.base_end == t.base_end && o.lines == t.lines {
                        merged.extend(o.lines.clone());
                        i = o.base_end;
                        oi += 1;
                        ti += 1;
                        continue;
                    }
                    if o.lines == t.lines {
                        merged.extend(o.lines.clone());
                        i = o.base_end.max(t.base_end);
                        oi += 1;
                        ti += 1;
                        continue;
                    }
                    push_conflict(&mut merged, &mut conflicts, i, &base_lines, o, t, opts);
                    i = o.base_end.max(t.base_end);
                    oi += 1;
                    ti += 1;
                }
                (Some(o), None) => {
                    merged.extend(o.lines.clone());
                    i = o.base_end;
                    oi += 1;
                }
                (None, Some(t)) => {
                    merged.extend(t.lines.clone());
                    i = t.base_end;
                    ti += 1;
                }
                (None, None) => break,
            }
            continue;
        }

        if o_start == i {
            let o = oe.unwrap();
            if t_start >= o.base_end {
                merged.extend(o.lines.clone());
                i = o.base_end;
                oi += 1;
                continue;
            }
            let t = te.unwrap();
            push_conflict(&mut merged, &mut conflicts, i, &base_lines, o, t, opts);
            i = o.base_end.max(t.base_end);
            oi += 1;
            ti += 1;
            continue;
        }

        if t_start == i {
            let t = te.unwrap();
            if o_start >= t.base_end {
                merged.extend(t.lines.clone());
                i = t.base_end;
                ti += 1;
                continue;
            }
            let o = oe.unwrap();
            push_conflict(&mut merged, &mut conflicts, i, &base_lines, o, t, opts);
            i = o.base_end.max(t.base_end);
            oi += 1;
            ti += 1;
        }
    }

    let merged_text = if merged.is_empty() {
        String::new()
    } else {
        merged.join("\n") + "\n"
    };
    Ok(MergeResult {
        merged: merged_text,
        conflicts,
    })
}

fn edits_from(
    base_s: &str,
    side_s: &str,
    side_lines: &[String],
    opts: &DiffOpts,
) -> Result<Vec<Edit>, TextDiffError> {
    let m = Matcher::new(base_s, side_s, opts.clone(), Granularity::Line)?;
    let mut edits = Vec::new();
    for op in m.opcodes() {
        match op.tag.as_str() {
            "equal" => {}
            "delete" => {
                edits.push(Edit {
                    base_start: op.i1,
                    base_end: op.i2,
                    lines: Vec::new(),
                });
            }
            "insert" => {
                edits.push(Edit {
                    base_start: op.i1,
                    base_end: op.i1,
                    lines: side_lines[op.j1..op.j2].to_vec(),
                });
            }
            "replace" => {
                edits.push(Edit {
                    base_start: op.i1,
                    base_end: op.i2,
                    lines: side_lines[op.j1..op.j2].to_vec(),
                });
            }
            _ => {}
        }
    }
    Ok(edits)
}

fn push_conflict(
    merged: &mut Vec<String>,
    conflicts: &mut Vec<MergeConflict>,
    i: usize,
    base_lines: &[String],
    o: &Edit,
    t: &Edit,
    opts: &MergeOpts,
) {
    let end = o.base_end.max(t.base_end).min(base_lines.len());
    let base_snip = base_lines[i..end].join("\n");
    let ours_snip = o.lines.join("\n");
    let theirs_snip = t.lines.join("\n");
    conflicts.push(MergeConflict {
        start: merged.len(),
        end: merged.len() + 1,
        base: base_snip.clone(),
        ours: ours_snip.clone(),
        theirs: theirs_snip.clone(),
    });
    merged.push(opts.marker_ours.clone());
    merged.extend(o.lines.clone());
    merged.push(opts.marker_base.clone());
    merged.extend(base_lines[i..end].iter().cloned());
    merged.push(opts.marker_theirs.clone());
    merged.extend(t.lines.clone());
    merged.push(opts.marker_end.clone());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_overlapping_merge() {
        let base = "a\nb\nc\nd\n";
        let ours = "a\nB\nc\nd\n";
        let theirs = "a\nb\nc\nD\n";
        let res = merge(base, ours, theirs, &MergeOpts::default()).unwrap();
        assert!(res.conflicts.is_empty());
        assert!(res.merged.contains("B"));
        assert!(res.merged.contains("D"));
    }
}
