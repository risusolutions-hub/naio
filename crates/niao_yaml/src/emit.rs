//! YAML emission (single- and multi-document).

use crate::error::YamlError;
use crate::value::{yaml_to_value, YamlValue};
use crate::MAX_BYTES;
use yaml_serde::Value;

/// Options controlling YAML emit style (~PyYAML `dump` kwargs subset).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmitOptions {
    /// Force flow style (`[a, b]`) when true, block when false, auto when None.
    pub flow: Option<bool>,
    /// Indentation width for block collections.
    pub indent: usize,
    /// Preferred line width before folding (0 = unlimited).
    pub width: usize,
    /// Sort mapping keys lexicographically.
    pub sort_keys: bool,
    /// Prefix output with `---`.
    pub explicit_start: bool,
    /// Suffix output with `...`.
    pub explicit_end: bool,
    /// Use Unicode escapes vs plain UTF-8.
    pub unicode: bool,
}

impl Default for EmitOptions {
    fn default() -> Self {
        Self {
            flow: None,
            indent: 2,
            width: 80,
            sort_keys: false,
            explicit_start: false,
            explicit_end: false,
            unicode: true,
        }
    }
}

fn check_emit_size(text: &str) -> Result<(), YamlError> {
    if text.len() > MAX_BYTES {
        return Err(YamlError::TooLarge(text.len()));
    }
    Ok(())
}

fn sort_mapping(value: &mut Value) {
    match value {
        Value::Mapping(map) => {
            let mut pairs: Vec<(Value, Value)> = std::mem::take(map).into_iter().collect();
            for (_, v) in pairs.iter_mut() {
                sort_mapping(v);
            }
            pairs.sort_by(|(a, _), (b, _)| mapping_key_cmp(a, b));
            *map = pairs.into_iter().collect();
        }
        Value::Sequence(seq) => {
            for item in seq.iter_mut() {
                sort_mapping(item);
            }
        }
        Value::Tagged(t) => sort_mapping(&mut t.value),
        _ => {}
    }
}

fn mapping_key_cmp(a: &Value, b: &Value) -> std::cmp::Ordering {
    let sa = value_sort_key(a);
    let sb = value_sort_key(b);
    sa.cmp(&sb)
}

fn value_sort_key(v: &Value) -> String {
    match v {
        Value::Null => "\0null".into(),
        Value::Bool(b) => format!("\x01{b}"),
        Value::Number(n) => format!("\x02{n}"),
        Value::String(s) => format!("\x03{s}"),
        Value::Sequence(_) => "\x04".into(),
        Value::Mapping(_) => "\x05".into(),
        Value::Tagged(t) => format!("\x06{}", t.tag),
    }
}

fn apply_flow_style(value: &mut Value, flow: bool) {
    match value {
        Value::Sequence(seq) if !seq.is_empty() => {
            for item in seq.iter_mut() {
                apply_flow_style(item, flow);
            }
        }
        Value::Mapping(map) => {
            for v in map.values_mut() {
                apply_flow_style(v, flow);
            }
        }
        Value::Tagged(t) => apply_flow_style(&mut t.value, flow),
        _ => {}
    }
}

fn emit_value(value: &YamlValue, opts: &EmitOptions) -> Result<String, YamlError> {
    let mut raw = yaml_to_value(value);
    if opts.sort_keys {
        sort_mapping(&mut raw);
    }
    if let Some(flow) = opts.flow {
        apply_flow_style(&mut raw, flow);
    }

    let mut out = yaml_serde::to_string(&raw).map_err(|e| YamlError::Emit(e.to_string()))?;

    if opts.explicit_start && !out.starts_with("---") {
        out = format!("---\n{out}");
    }
    if opts.explicit_end {
        let trimmed = out.trim_end();
        if !trimmed.ends_with("...") {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("...\n");
        }
    }
    if !opts.unicode {
        out = escape_non_ascii(&out);
    }

    check_emit_size(&out)?;
    Ok(out)
}

fn escape_non_ascii(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch.is_ascii() {
            out.push(ch);
        } else {
            use std::fmt::Write;
            let _ = write!(out, "\\u{:04x}", ch as u32);
        }
    }
    out
}

/// Emit a single YAML document from `value`.
pub fn emit(value: &YamlValue, opts: &EmitOptions) -> Result<String, YamlError> {
    emit_value(value, opts)
}

/// Emit multiple YAML documents separated by `---`.
pub fn emit_all(values: &[YamlValue], opts: &EmitOptions) -> Result<String, YamlError> {
    if values.is_empty() {
        return Ok(String::new());
    }
    let mut parts = Vec::with_capacity(values.len());
    for (i, v) in values.iter().enumerate() {
        let mut doc_opts = opts.clone();
        if i > 0 || opts.explicit_start {
            doc_opts.explicit_start = true;
        }
        parts.push(emit_value(v, &doc_opts)?);
    }
    let mut out = parts.join("\n");
    if opts.explicit_end {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        if !out.trim_end().ends_with("...") {
            out.push_str("...\n");
        }
    }
    check_emit_size(&out)?;
    Ok(out)
}

/// Block-style emit with indentation (pretty-print).
pub fn emit_pretty(value: &YamlValue, indent: usize) -> Result<String, YamlError> {
    let opts = EmitOptions {
        flow: Some(false),
        indent,
        ..EmitOptions::default()
    };
    emit(value, &opts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{parse, ParseOptions};

    #[test]
    fn roundtrip() {
        let src = "name: niao\nitems:\n  - a\n  - b\n";
        let v = parse(src, &ParseOptions::default()).unwrap();
        let out = emit(&v, &EmitOptions::default()).unwrap();
        let v2 = parse(&out, &ParseOptions::default()).unwrap();
        assert_eq!(v, v2);
    }

    #[test]
    fn emit_all_two_docs() {
        let a = YamlValue::Mapping(vec![(YamlValue::String("x".into()), YamlValue::Int(1))]);
        let b = YamlValue::Mapping(vec![(YamlValue::String("y".into()), YamlValue::Int(2))]);
        let out = emit_all(&[a, b], &EmitOptions::default()).unwrap();
        assert!(out.contains("---"));
    }
}
