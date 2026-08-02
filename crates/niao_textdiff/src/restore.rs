//! Restore text from ndiff output (difflib.restore).

use crate::error::TextDiffError;

/// Reconstruct text from ndiff/compare lines. `which` is 1 (old) or 2 (new).
pub fn restore(which: i32, diff: &[String]) -> Result<String, TextDiffError> {
    if which != 1 && which != 2 {
        return Err(TextDiffError::new("restore() which must be 1 or 2"));
    }
    let want = if which == 1 { b'-' } else { b'+' };
    let mut parts = Vec::new();
    for line in diff {
        if line.is_empty() {
            continue;
        }
        let tag = line.as_bytes()[0];
        if tag != b' ' && tag != want {
            continue;
        }
        let body = line.get(1..).unwrap_or("").strip_prefix(' ').unwrap_or("");
        parts.push(body.to_string());
    }
    if parts.is_empty() {
        return Ok(String::new());
    }
    let mut out = parts.join("\n");
    out.push('\n');
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::line::compare;
    use crate::opts::DiffOpts;

    #[test]
    fn roundtrip_restore() {
        let a = "hello\nworld\n";
        let b = "hello\nthere\n";
        let diff = compare(a, b, &DiffOpts::default()).unwrap();
        let r1 = restore(1, &diff).unwrap();
        let r2 = restore(2, &diff).unwrap();
        assert_eq!(r1, a);
        assert_eq!(r2, b);
    }
}
