//! Unified and diff-match-patch patch create/apply.

use crate::error::TextDiffError;
use crate::line;
use crate::opts::DiffOpts;
use crate::split::{check_input_len, check_pair};
use diff_match_patch::{Diff, Dmp};

#[derive(Debug, Clone)]
pub struct PatchApplyResult {
    pub text: String,
    pub applied: Vec<bool>,
}

/// Create a unified diff patch string.
pub fn patch_make(a: &str, b: &str, opts: &DiffOpts) -> Result<String, TextDiffError> {
    let lines = line::unified(a, b, opts)?;
    Ok(lines.join("\n") + if lines.is_empty() { "" } else { "\n" })
}

/// Create diff-match-patch patches (compact patch format).
pub fn patch_make_dmp(a: &str, b: &str, _opts: &DiffOpts) -> Result<String, TextDiffError> {
    check_pair(a, b)?;
    let mut dmp = Dmp::new();
    dmp.diff_timeout = None;
    let mut patches = dmp.patch_make1(a, b);
    Ok(dmp.patch_to_text(&mut patches))
}

/// Apply unified diff patch to text.
pub fn patch_apply(
    text: &str,
    patch: &str,
    opts: &DiffOpts,
) -> Result<PatchApplyResult, TextDiffError> {
    check_input_len(text.len())?;
    check_input_len(patch.len())?;
    if patch.contains("@@") {
        apply_unified(text, patch, opts)
    } else {
        apply_dmp(text, patch)
    }
}

fn apply_unified(
    text: &str,
    patch: &str,
    opts: &DiffOpts,
) -> Result<PatchApplyResult, TextDiffError> {
    let mut lines: Vec<String> = if text.is_empty() {
        Vec::new()
    } else {
        text.split_inclusive('\n').map(|s| s.to_string()).collect()
    };
    if !text.ends_with('\n') && !lines.is_empty() {
        if let Some(last) = lines.last_mut() {
            *last = last.trim_end_matches('\n').to_string();
        }
    }

    let mut applied = Vec::new();
    let mut hunk_lines: Vec<String> = Vec::new();
    let mut old_start = 0usize;
    let mut old_len = 0usize;
    let mut in_hunk = false;

    let flush = |lines: &mut Vec<String>,
                 hunk: &mut Vec<String>,
                 old_start: &mut usize,
                 old_len: &mut usize,
                 applied: &mut Vec<bool>|
     -> Result<(), TextDiffError> {
        if hunk.is_empty() {
            return Ok(());
        }
        let idx = old_start.saturating_sub(1);
        if idx > lines.len() {
            applied.push(false);
            hunk.clear();
            return Ok(());
        }
        let mut cursor = idx;
        let mut new_segment = Vec::new();
        let mut ok = true;
        for hl in hunk.iter() {
            if hl.is_empty() {
                continue;
            }
            let tag = hl.as_bytes()[0];
            let body = hl.get(1..).unwrap_or("");
            match tag {
                b' ' => {
                    if cursor >= lines.len()
                        || lines[cursor].trim_end_matches('\n') != body.trim_end_matches('\n')
                    {
                        ok = false;
                        break;
                    }
                    new_segment.push(lines[cursor].clone());
                    cursor += 1;
                }
                b'-' => {
                    if cursor >= lines.len()
                        || lines[cursor].trim_end_matches('\n') != body.trim_end_matches('\n')
                    {
                        ok = false;
                        break;
                    }
                    cursor += 1;
                }
                b'+' => {
                    let mut line = body.to_string();
                    if !line.ends_with('\n') && !opts.lineterm.is_empty() {
                        line.push_str(&opts.lineterm);
                    }
                    new_segment.push(line);
                }
                _ => {}
            }
        }
        if ok {
            lines.splice(idx..idx + *old_len, new_segment);
            applied.push(true);
        } else {
            applied.push(false);
        }
        hunk.clear();
        *old_len = 0;
        Ok(())
    };

    for raw in patch.lines() {
        let line = raw.to_string();
        if line.starts_with("@@") {
            flush(
                &mut lines,
                &mut hunk_lines,
                &mut old_start,
                &mut old_len,
                &mut applied,
            )?;
            in_hunk = true;
            if let Some((old, _new)) = parse_hunk_header(&line) {
                old_start = old.0;
                old_len = old.1;
            }
            continue;
        }
        if in_hunk && !line.is_empty() && matches!(line.as_bytes()[0], b' ' | b'-' | b'+') {
            hunk_lines.push(line);
        }
    }
    flush(
        &mut lines,
        &mut hunk_lines,
        &mut old_start,
        &mut old_len,
        &mut applied,
    )?;

    let mut out = String::new();
    for l in lines {
        out.push_str(&l);
        if !l.ends_with('\n') {
            out.push_str(&opts.lineterm);
        }
    }
    Ok(PatchApplyResult { text: out, applied })
}

fn parse_hunk_header(line: &str) -> Option<((usize, usize), (usize, usize))> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 3 {
        return None;
    }
    let old = parse_range(parts[1].trim_start_matches('-'))?;
    let new = parse_range(parts[2].trim_start_matches('+'))?;
    Some((old, new))
}

fn parse_range(s: &str) -> Option<(usize, usize)> {
    let s = s.trim_end_matches("@@");
    if let Some((a, b)) = s.split_once(',') {
        Some((a.parse().ok()?, b.parse().ok()?))
    } else {
        Some((s.parse().ok()?, 1))
    }
}

fn apply_dmp(text: &str, patch: &str) -> Result<PatchApplyResult, TextDiffError> {
    let mut dmp = Dmp::new();
    dmp.diff_timeout = None;
    let mut patches = dmp.patch_from_text(patch.to_string());
    let (out, results) = dmp.patch_apply(&mut patches, text);
    Ok(PatchApplyResult {
        text: out.into_iter().collect(),
        applied: results,
    })
}

/// Reconstruct `text1`/`text2` from a diff-match-patch diff list.
pub fn diff_to_texts(diffs: &[(i32, String)]) -> (String, String) {
    let mut t1 = String::new();
    let mut t2 = String::new();
    for (op, s) in diffs {
        match *op {
            -1 => t1.push_str(s),
            0 => {
                t1.push_str(s);
                t2.push_str(s);
            }
            1 => t2.push_str(s),
            _ => {}
        }
    }
    (t1, t2)
}

pub fn patch_from_diffs(a: &str, diffs: &[(i32, String)]) -> Result<String, TextDiffError> {
    check_input_len(a.len())?;
    let mut dmp = Dmp::new();
    let rust_diffs: Vec<Diff> = diffs
        .iter()
        .map(|(op, text)| Diff::new(*op, text.clone()))
        .collect();
    let mut patches = dmp.patch_make4(a, &mut rust_diffs.clone());
    Ok(dmp.patch_to_text(&mut patches))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::opts::DiffOpts;

    #[test]
    fn unified_roundtrip() {
        let a = "one\ntwo\nthree\n";
        let b = "one\nTWO\nthree\n";
        let opts = DiffOpts::default();
        let patch = patch_make(&a, &b, &opts).unwrap();
        let res = patch_apply(&a, &patch, &opts).unwrap();
        assert!(res.applied.iter().any(|x| *x));
        assert_eq!(res.text.trim_end(), b.trim_end());
    }
}
