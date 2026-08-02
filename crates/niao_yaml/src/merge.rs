//! Post-parse transforms (merge keys) and safe-mode source checks.

use crate::error::YamlError;
use crate::value::YamlValue;

/// Reject dangerous constructs in source text before parse (PyYAML safe_load parity).
pub fn safe_precheck(text: &str) -> Result<(), YamlError> {
    let lower = text.to_ascii_lowercase();
    const BLOCKED: &[&str] = &[
        "!!python/",
        "!!python:",
        "!!python ",
        "tag:yaml.org,2002:python",
    ];
    for needle in BLOCKED {
        if lower.contains(needle) {
            return Err(YamlError::UnsafeTag((*needle).into()));
        }
    }
    Ok(())
}

fn keys_equal(a: &YamlValue, b: &YamlValue) -> bool {
    a == b
}

/// Expand YAML merge keys (`<<:` / `<<`) in mappings (~PyYAML merge semantics).
pub fn resolve_merge_keys(value: &mut YamlValue) {
    match value {
        YamlValue::Sequence(seq) => {
            for item in seq.iter_mut() {
                resolve_merge_keys(item);
            }
        }
        YamlValue::Tagged { value: inner, .. } => resolve_merge_keys(inner),
        YamlValue::Mapping(pairs) => {
            for (_, v) in pairs.iter_mut() {
                resolve_merge_keys(v);
            }

            let merge_idx = pairs
                .iter()
                .position(|(k, _)| matches!(k, YamlValue::String(s) if s == "<<" || s == "<<:"));
            let Some(merge_idx) = merge_idx else {
                return;
            };

            let merge_val = pairs.remove(merge_idx).1;
            let sources: Vec<YamlValue> = match merge_val {
                YamlValue::Mapping(m) => vec![YamlValue::Mapping(m)],
                YamlValue::Sequence(seq) => seq,
                other => vec![other],
            };

            for src in sources {
                let YamlValue::Mapping(mut from) = src else {
                    continue;
                };
                for (k, v) in from.drain(..) {
                    if pairs.iter().any(|(ek, _)| keys_equal(ek, &k)) {
                        continue;
                    }
                    pairs.push((k, v));
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{parse, ParseOptions};

    #[test]
    fn merge_key_expansion() {
        let src = "defaults: &def\n  x: 1\nitem:\n  <<: *def\n  y: 2\n";
        let mut v = parse(
            src,
            &ParseOptions {
                safe: true,
                multi: false,
            },
        )
        .unwrap();
        // parse() applies resolve_merge_keys
        match v {
            YamlValue::Mapping(ref m) => {
                let item = m
                    .iter()
                    .find(|(k, _)| matches!(k, YamlValue::String(s) if s == "item"))
                    .map(|(_, v)| v)
                    .unwrap();
                match item {
                    YamlValue::Mapping(im) => {
                        assert!(im.iter().any(|(k, v)| {
                            matches!(k, YamlValue::String(s) if s == "x")
                                && matches!(v, YamlValue::Int(1))
                        }));
                        assert!(im.iter().any(|(k, v)| {
                            matches!(k, YamlValue::String(s) if s == "y")
                                && matches!(v, YamlValue::Int(2))
                        }));
                        assert!(!im
                            .iter()
                            .any(|(k, _)| matches!(k, YamlValue::String(s) if s == "<<")));
                    }
                    _ => panic!("expected item map"),
                }
            }
            _ => panic!("expected root map"),
        }
    }

    #[test]
    fn blocks_python_in_source() {
        assert!(safe_precheck("!!python/object:os.system\n").is_err());
    }
}
