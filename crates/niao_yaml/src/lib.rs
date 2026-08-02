//! YAML 1.2 parse and emit for Niao (`nyaml`).
//!
//! Backed by [`yaml_serde`] (libyaml). Supports safe-by-default loading,
//! anchors/aliases, multi-document streams, and configurable emit style.
//!
//! ```ignore
//! use niao_yaml::{parse, emit, ParseOptions, EmitOptions};
//! let v = parse("key: value\n", &ParseOptions::default()).unwrap();
//! let text = emit(&v, &EmitOptions::default()).unwrap();
//! ```

mod emit;
mod error;
mod merge;
mod parse;
mod value;

pub use emit::{emit, emit_all, emit_pretty, EmitOptions};
pub use error::YamlError;
pub use parse::{is_valid, parse, parse_all, ParseOptions};
pub use value::{is_safe_tag, yaml_to_owned, YamlValue};

/// Maximum input/output size (64 MiB guard).
pub const MAX_BYTES: usize = 64 * 1024 * 1024;

/// Parse with safe defaults (alias for `parse` with `ParseOptions::default`).
pub fn safe_parse(text: &str) -> Result<YamlValue, YamlError> {
    parse(text, &ParseOptions::default())
}

/// Parse all documents with safe defaults.
pub fn safe_parse_all(text: &str) -> Result<Vec<YamlValue>, YamlError> {
    parse_all(text, &ParseOptions::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_alias_roundtrip() {
        let src = "base: &base\n  id: 1\nref:\n  <<: *base\n";
        let v = parse(src, &ParseOptions::default()).unwrap();
        let out = emit(&v, &EmitOptions::default()).unwrap();
        let v2 = parse(&out, &ParseOptions::default()).unwrap();
        assert_eq!(v, v2);
    }
}
