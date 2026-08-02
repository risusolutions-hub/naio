//! YAML parsing (single- and multi-document).

use crate::error::YamlError;
use crate::merge::{resolve_merge_keys, safe_precheck};
use crate::value::{is_safe_tag, yaml_to_owned, YamlValue};
use crate::MAX_BYTES;
use serde::de::Deserialize;
use yaml_serde::{Deserializer, Value};

/// Options controlling YAML parse behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseOptions {
    /// Reject custom / Python-style tags (PyYAML `safe_load` semantics).
    pub safe: bool,
    /// When false, `parse()` errors if more than one `---` document is present.
    pub multi: bool,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            safe: true,
            multi: false,
        }
    }
}

fn check_size(text: &str) -> Result<(), YamlError> {
    if text.len() > MAX_BYTES {
        return Err(YamlError::TooLarge(text.len()));
    }
    Ok(())
}

fn reject_unsafe(value: &Value, safe: bool) -> Result<(), YamlError> {
    if !safe {
        return Ok(());
    }
    match value {
        Value::Tagged(t) => {
            let tag = t.tag.to_string();
            if !is_safe_tag(&tag) {
                return Err(YamlError::UnsafeTag(tag));
            }
            reject_unsafe(&t.value, safe)?;
        }
        Value::Sequence(seq) => {
            for item in seq {
                reject_unsafe(item, safe)?;
            }
        }
        Value::Mapping(map) => {
            for (k, v) in map {
                reject_unsafe(k, safe)?;
                reject_unsafe(v, safe)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn deserialize_doc<'de, D>(de: D, opts: &ParseOptions) -> Result<YamlValue, YamlError>
where
    D: serde::Deserializer<'de>,
{
    let raw = Value::deserialize(de).map_err(|e| YamlError::Parse(e.to_string()))?;
    reject_unsafe(&raw, opts.safe)?;
    Ok(yaml_to_owned(&raw))
}

/// Parse a single YAML document from `text`.
pub fn parse(text: &str, opts: &ParseOptions) -> Result<YamlValue, YamlError> {
    if text.is_empty() {
        return Err(YamlError::EmptyInput);
    }
    check_size(text)?;
    if opts.safe {
        safe_precheck(text)?;
    }

    let mut docs = Vec::new();
    for doc in Deserializer::from_str(text) {
        let mut v = deserialize_doc(doc, opts)?;
        resolve_merge_keys(&mut v);
        docs.push(v);
    }

    if docs.is_empty() {
        return Ok(YamlValue::Null);
    }
    if docs.len() > 1 && !opts.multi {
        return Err(YamlError::MultiDocSingle);
    }
    Ok(docs.remove(0))
}

/// Parse all YAML documents in `text` (multi-doc streams).
pub fn parse_all(text: &str, opts: &ParseOptions) -> Result<Vec<YamlValue>, YamlError> {
    if text.is_empty() {
        return Err(YamlError::EmptyInput);
    }
    check_size(text)?;
    if opts.safe {
        safe_precheck(text)?;
    }

    let mut out = Vec::new();
    for doc in Deserializer::from_str(text) {
        let mut v = deserialize_doc(doc, opts)?;
        resolve_merge_keys(&mut v);
        out.push(v);
    }
    if out.is_empty() {
        out.push(YamlValue::Null);
    }
    Ok(out)
}

/// Return `true` when `text` is valid YAML (any number of documents).
pub fn is_valid(text: &str) -> bool {
    if text.is_empty() || text.len() > MAX_BYTES {
        return false;
    }
    let opts = ParseOptions {
        safe: false,
        multi: true,
    };
    parse_all(text, &opts).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapping_pairs_get<'a>(
        pairs: &'a [(YamlValue, YamlValue)],
        key: &str,
    ) -> Option<&'a YamlValue> {
        pairs.iter().find_map(|(k, v)| match k {
            YamlValue::String(s) if s == key => Some(v),
            _ => None,
        })
    }

    #[test]
    fn parse_scalar() {
        let v = parse("hello: world\n", &ParseOptions::default()).unwrap();
        match v {
            YamlValue::Mapping(m) => {
                assert_eq!(
                    mapping_pairs_get(&m, "hello"),
                    Some(&YamlValue::String("world".into()))
                );
            }
            _ => panic!("expected map"),
        }
    }

    #[test]
    fn anchors_resolve() {
        let src = "defaults: &def\n  x: 1\nitem:\n  <<: *def\n  y: 2\n";
        let v = parse(src, &ParseOptions::default()).unwrap();
        match v {
            YamlValue::Mapping(m) => {
                let item = mapping_pairs_get(&m, "item").unwrap();
                match item {
                    YamlValue::Mapping(im) => {
                        assert_eq!(mapping_pairs_get(im, "x"), Some(&YamlValue::Int(1)));
                        assert_eq!(mapping_pairs_get(im, "y"), Some(&YamlValue::Int(2)));
                    }
                    _ => panic!("expected map item"),
                }
            }
            _ => panic!("expected map"),
        }
    }

    #[test]
    fn multi_doc_requires_parse_all() {
        let src = "---\n{a: 1}\n---\n{b: 2}\n";
        assert!(matches!(
            parse(src, &ParseOptions::default()),
            Err(YamlError::MultiDocSingle)
        ));
        let all = parse_all(src, &ParseOptions::default()).unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn safe_rejects_python_tag() {
        let src = "!!python/object:os.system\n";
        assert!(matches!(
            parse(src, &ParseOptions::default()),
            Err(YamlError::UnsafeTag(_))
        ));
        let seq_src = "!!python/object/apply:os.system\n- calc\n";
        assert!(matches!(
            parse(seq_src, &ParseOptions::default()),
            Err(YamlError::UnsafeTag(_))
        ));
        let opts = ParseOptions {
            safe: false,
            ..Default::default()
        };
        assert!(parse(seq_src, &opts).is_ok());
    }
}
