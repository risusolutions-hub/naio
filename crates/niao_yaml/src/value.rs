//! Internal YAML value representation and serde_yaml bridge.

use niao_bignum::BigInt;
use yaml_serde::value::{Tag, TaggedValue};
use yaml_serde::{Mapping, Number, Value};

/// Owned YAML value (maps to Niao objects after runtime conversion).
#[derive(Debug, Clone, PartialEq)]
pub enum YamlValue {
    Null,
    Bool(bool),
    Int(i64),
    BigInt(BigInt),
    Float(f64),
    String(String),
    Sequence(Vec<YamlValue>),
    Mapping(Vec<(YamlValue, YamlValue)>),
    /// Tagged scalar/sequence/map (`!!timestamp`, custom tags).
    Tagged {
        tag: String,
        value: Box<YamlValue>,
    },
}

/// Standard YAML 1.2 tags allowed in safe mode (~PyYAML SafeLoader).
pub fn is_safe_tag(tag: &str) -> bool {
    let t = tag.trim();
    if t.is_empty() {
        return true;
    }
    if t.starts_with("!!") {
        let short = &t[2..];
        return matches!(
            short,
            "str"
                | "bool"
                | "int"
                | "float"
                | "null"
                | "map"
                | "seq"
                | "omap"
                | "set"
                | "binary"
                | "timestamp"
                | "merge"
                | "value"
        );
    }
    const SAFE_URI: &[&str] = &[
        "tag:yaml.org,2002:str",
        "tag:yaml.org,2002:bool",
        "tag:yaml.org,2002:int",
        "tag:yaml.org,2002:float",
        "tag:yaml.org,2002:null",
        "tag:yaml.org,2002:map",
        "tag:yaml.org,2002:seq",
        "tag:yaml.org,2002:omap",
        "tag:yaml.org,2002:set",
        "tag:yaml.org,2002:binary",
        "tag:yaml.org,2002:timestamp",
        "tag:yaml.org,2002:merge",
        "tag:yaml.org,2002:value",
    ];
    SAFE_URI.iter().any(|s| *s == t)
}

fn number_to_yaml(n: &Number) -> YamlValue {
    if let Some(i) = n.as_i64() {
        YamlValue::Int(i)
    } else if let Some(u) = n.as_u64() {
        if u <= i64::MAX as u64 {
            YamlValue::Int(u as i64)
        } else {
            YamlValue::BigInt(BigInt::from(u))
        }
    } else if let Some(f) = n.as_f64() {
        if f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64 {
            YamlValue::Int(f as i64)
        } else {
            YamlValue::Float(f)
        }
    } else {
        YamlValue::Null
    }
}

/// Convert a borrowed `yaml_serde::Value` into an owned [`YamlValue`].
pub fn yaml_to_owned(v: &Value) -> YamlValue {
    match v {
        Value::Null => YamlValue::Null,
        Value::Bool(b) => YamlValue::Bool(*b),
        Value::Number(n) => number_to_yaml(n),
        Value::String(s) => YamlValue::String(s.clone()),
        Value::Sequence(seq) => YamlValue::Sequence(seq.iter().map(yaml_to_owned).collect()),
        Value::Mapping(map) => mapping_to_yaml(map),
        Value::Tagged(t) => YamlValue::Tagged {
            tag: t.tag.to_string(),
            value: Box::new(yaml_to_owned(&t.value)),
        },
    }
}

fn mapping_to_yaml(map: &Mapping) -> YamlValue {
    let mut pairs = Vec::with_capacity(map.len());
    for (k, v) in map.iter() {
        pairs.push((yaml_to_owned(k), yaml_to_owned(v)));
    }
    YamlValue::Mapping(pairs)
}

fn yaml_to_serde(v: &YamlValue) -> Value {
    match v {
        YamlValue::Null => Value::Null,
        YamlValue::Bool(b) => Value::Bool(*b),
        YamlValue::Int(i) => Value::Number(Number::from(*i)),
        YamlValue::BigInt(n) => {
            if let Some(i) = n.to_i64() {
                Value::Number(Number::from(i))
            } else if let Some(u) = n.to_u64() {
                Value::Number(Number::from(u))
            } else {
                Value::String(n.to_string())
            }
        }
        YamlValue::Float(f) => Value::Number(Number::from(*f)),
        YamlValue::String(s) => Value::String(s.clone()),
        YamlValue::Sequence(seq) => Value::Sequence(seq.iter().map(yaml_to_serde).collect()),
        YamlValue::Mapping(pairs) => {
            let mut map = Mapping::new();
            for (k, val) in pairs {
                map.insert(yaml_to_serde(k), yaml_to_serde(val));
            }
            Value::Mapping(map)
        }
        YamlValue::Tagged { tag, value } => Value::Tagged(Box::new(TaggedValue {
            tag: Tag::new(tag),
            value: yaml_to_serde(value),
        })),
    }
}

/// Convert [`YamlValue`] to `yaml_serde::Value` for emission.
pub fn yaml_to_value(v: &YamlValue) -> Value {
    yaml_to_serde(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_tags() {
        assert!(is_safe_tag("!!str"));
        assert!(is_safe_tag("tag:yaml.org,2002:int"));
        assert!(!is_safe_tag("!!python/object"));
    }
}
