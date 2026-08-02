//! Doc-comment extraction from Niao source (`//` and `///` lines above declarations).

/// Strip doctest markers from a comment body.
#[inline]
fn is_doctest_marker(content: &str) -> bool {
    let t = content.trim_start();
    t.starts_with(">>>") || t.starts_with("=>")
}

/// Extract leading `//` / `///` comment text immediately above `target_line` (1-based).
pub fn extract_doc_before_line(source: &str, target_line: usize) -> Option<String> {
    if target_line == 0 {
        return None;
    }
    let lines: Vec<&str> = source.lines().collect();
    let mut i = target_line.saturating_sub(2);
    let mut doc_lines: Vec<String> = Vec::new();

    loop {
        let line = lines.get(i)?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if doc_lines.is_empty() {
                if i == 0 {
                    break;
                }
                i -= 1;
                continue;
            }
            break;
        }

        let comment = if let Some(rest) = trimmed.strip_prefix("///") {
            rest.trim_start()
        } else if let Some(rest) = trimmed.strip_prefix("//") {
            rest.trim_start()
        } else {
            break;
        };

        if is_doctest_marker(comment) {
            break;
        }
        doc_lines.push(comment.to_string());
        if i == 0 {
            break;
        }
        i -= 1;
    }

    if doc_lines.is_empty() {
        None
    } else {
        doc_lines.reverse();
        Some(clean_doc(&doc_lines.join("\n")))
    }
}

/// Collapse extra blank lines; trim outer whitespace (like Python `getdoc` cleanup).
pub fn clean_doc(s: &str) -> String {
    let mut out = String::new();
    let mut prev_blank = false;
    for line in s.lines() {
        let t = line.trim_end();
        if t.is_empty() {
            if !prev_blank && !out.is_empty() {
                out.push('\n');
            }
            prev_blank = true;
        } else {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(t);
            prev_blank = false;
        }
    }
    out.trim().to_string()
}

/// Lookup doc for a top-level declaration name in source text.
pub fn doc_from_source(source: &str, name: &str) -> Option<String> {
    doc_for_decl(source, name, None)
}

/// Lookup doc for a declaration; optional kind hint: `"fn"`, `"struct"`, `"class"`.
pub fn doc_for_decl(source: &str, name: &str, kind: Option<&str>) -> Option<String> {
    let fn_open = format!("fn {name}(");
    let fn_space = format!("fn {name} ");
    let fn_brace = format!("fn {name}{{");
    let struct_pat = format!("struct {name}");
    let class_pat = format!("class {name}");
    let trait_pat = format!("trait {name}");

    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        let matched = match kind {
            Some("struct") => trimmed.starts_with(&struct_pat),
            Some("class") => trimmed.starts_with(&class_pat),
            Some("trait") => trimmed.starts_with(&trait_pat),
            Some("fn") | None => {
                trimmed.starts_with(&fn_open)
                    || trimmed.starts_with(&fn_space)
                    || trimmed.starts_with(&fn_brace)
            }
            _ => trimmed.contains(name),
        };
        if matched {
            return extract_doc_before_line(source, idx + 1);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_block_comment() {
        let src = r#"// Adds two integers.
// Returns the sum.
fn add(a, b) {
    return a + b
}
"#;
        let doc = doc_from_source(src, "add").unwrap();
        assert!(doc.contains("Adds two integers"));
        assert!(doc.contains("Returns the sum"));
    }

    #[test]
    fn skips_doctest_markers() {
        let src = r#"// >>> add(1, 2)
// => 3
fn add(a, b) {
    return a + b
}
"#;
        assert!(doc_from_source(src, "add").is_none());
    }
}
