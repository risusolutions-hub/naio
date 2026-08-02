//! Normalize and split text for diffing.

use crate::error::TextDiffError;
use crate::opts::DiffOpts;

/// Maximum input size (16 MiB) — matches other text-processing libs.
pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;

pub fn check_input_len(len: usize) -> Result<(), TextDiffError> {
    if len > MAX_INPUT_BYTES {
        return Err(TextDiffError::new(format!(
            "input size {len} exceeds limit {MAX_INPUT_BYTES}"
        )));
    }
    Ok(())
}

pub fn check_pair(a: &str, b: &str) -> Result<(), TextDiffError> {
    check_input_len(a.len())?;
    check_input_len(b.len())?;
    Ok(())
}

pub fn check_triple(a: &str, b: &str, c: &str) -> Result<(), TextDiffError> {
    check_input_len(a.len())?;
    check_input_len(b.len())?;
    check_input_len(c.len())?;
    Ok(())
}

/// Split text into lines, optionally keeping line terminators (difflib.splitlines).
pub fn splitlines(text: &str, keepends: bool) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut start = 0usize;
    for (i, b) in text.bytes().enumerate() {
        if b == b'\n' {
            let end = i + 1;
            if keepends {
                out.push(text[start..end].to_string());
            } else {
                let line = &text[start..i];
                out.push(strip_cr(line).to_string());
            }
            start = end;
        }
    }
    if start < text.len() {
        let tail = &text[start..];
        if keepends {
            out.push(tail.to_string());
        } else {
            out.push(strip_cr(tail).to_string());
        }
    }
    out
}

fn strip_cr(s: &str) -> &str {
    s.strip_suffix('\r').unwrap_or(s)
}

pub fn normalize_line(s: &str, opts: &DiffOpts) -> String {
    let mut line = s.to_string();
    if opts.ignore_whitespace {
        line = line.split_whitespace().collect::<Vec<_>>().join(" ");
    }
    if opts.ignore_case {
        line = line.to_lowercase();
    }
    line
}

pub fn normalize_lines(lines: &[String], opts: &DiffOpts) -> Vec<String> {
    if !opts.ignore_whitespace && !opts.ignore_case {
        return lines.to_vec();
    }
    lines.iter().map(|l| normalize_line(l, opts)).collect()
}

pub fn join_output(lines: &[String], opts: &DiffOpts) -> String {
    if opts.lineterm.is_empty() {
        return lines.concat();
    }
    let mut out = String::new();
    for line in lines {
        out.push_str(line);
        if !line.ends_with('\n') && !line.ends_with('\r') {
            out.push_str(&opts.lineterm);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitlines_keepends() {
        let lines = splitlines("a\nb\n", true);
        assert_eq!(lines, vec!["a\n", "b\n"]);
    }

    #[test]
    fn splitlines_strip() {
        let lines = splitlines("a\r\nb", false);
        assert_eq!(lines, vec!["a", "b"]);
    }
}
