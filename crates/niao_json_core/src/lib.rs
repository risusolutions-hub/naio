//! Zero-dependency JSON parse/stringify engine for Niao.

pub mod error;
pub mod number;
pub mod object;
pub mod parse;
pub mod value;
pub mod write;
pub mod toml;

pub use error::ParseError;
pub use number::Number;
pub use object::Object;
pub use parse::{is_valid, is_valid_bytes, parse, parse_bytes, parse_bytes_with_depth, DEFAULT_MAX_DEPTH};
pub use value::Value;
pub use write::{to_string, to_string_pretty, to_vec, write_value, Writer};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::number::Number;
use crate::object::Object;

    fn roundtrip(s: &str) {
        let v = parse(s).expect("parse");
        let out = to_string(&v);
        let v2 = parse(&out).expect("re-parse");
        assert_eq!(to_string(&v), to_string(&v2), "roundtrip failed for {s:?} -> {out:?}");
    }

    #[test]
    fn null_bool_literals() {
        roundtrip("null");
        roundtrip("true");
        roundtrip("false");
    }

    #[test]
    fn integers() {
        roundtrip("0");
        roundtrip("-0");
        roundtrip("42");
        roundtrip("-9223372036854775808");
        roundtrip("9223372036854775807");
        roundtrip("18446744073709551615");
    }

    #[test]
    fn floats() {
        roundtrip("1.0");
        roundtrip("3.14159");
        roundtrip("1e10");
        roundtrip("-1.5e-3");
        roundtrip("1e309");
        let v = parse("-0.0").unwrap();
        assert!(matches!(v, Value::Number(Number::F64(f)) if f == 0.0 && f.is_sign_negative()));
    }

    #[test]
    fn parse_empty_string() {
        assert_eq!(parse("\"\"").unwrap(), Value::String(String::new()));
    }

    #[test]
    fn parse_hello_string() {
        assert_eq!(parse("\"hello\"").unwrap(), Value::String("hello".into()));
    }

    #[test]
    fn strings() {
        roundtrip("\"\"");
        roundtrip("\"hello\"");
        roundtrip("\"\\n\\t\\r\\\\\\\"\"");
        roundtrip("\"\\u0041\"");
        let v = parse("\"\\uD800\\uDC00\"").unwrap();
        assert_eq!(v.as_str().map(str::len), Some(4)); // UTF-8 encoded U+10000
        assert!(parse(&to_string(&v)).is_ok());
    }

    #[test]
    fn arrays() {
        roundtrip("[]");
        roundtrip("[1,2,3]");
        roundtrip(r#"[null,true,false,"x"]"#);
    }

    #[test]
    fn objects() {
        roundtrip("{}");
        roundtrip(r#"{"a":1,"b":2}"#);
        let v = parse(r#"{"z":1,"a":2}"#).unwrap();
        let out = to_string(&v);
        assert!(out.contains("\"z\""));
    }

    #[test]
    fn trailing_data_rejected() {
        assert!(matches!(parse("nullx"), Err(ParseError::TrailingData)));
        assert!(matches!(parse("[] 1"), Err(ParseError::TrailingData)));
    }

    #[test]
    fn invalid_unicode_escape() {
        assert!(parse(r#""\uD800""#).is_err());
        assert!(parse(r#""\uZZZZ""#).is_err());
    }

    #[test]
    fn depth_limit() {
        let deep = "[".repeat(600) + &"]".repeat(600);
        assert!(matches!(
            parse_bytes_with_depth(deep.as_bytes(), 128),
            Err(ParseError::DepthLimit)
        ));
    }

    #[test]
    fn big_int_no_float_roundtrip() {
        let v = parse("9007199254740993").unwrap();
        assert_eq!(v.as_i64(), Some(9007199254740993));
    }

    #[test]
    fn json_test_suite_samples() {
        let cases = [
            ("n_null", "null"),
            ("n_true", "true"),
            ("n_false", "false"),
            ("n_int", "123"),
            ("n_neg_int", "-123"),
            ("n_float", "1.23"),
            ("n_exp", "1e2"),
            ("n_exp_neg", "1.2e-3"),
            ("n_zero", "0"),
            ("n_neg_zero", "-0"),
            ("s_empty", "\"\""),
            ("s_simple", "\"abc\""),
            ("s_escaped_quote", "\"\\\"foo\\\"\""),
            ("s_unicode", "\"\\u0041\""),
            ("a_empty", "[]"),
            ("a_simple", "[1,2,3]"),
            ("o_empty", "{}"),
            ("o_simple", "{\"a\":1}"),
            ("o_nested", "{\"a\":{\"b\":2}}"),
        ];
        for (name, input) in cases {
            roundtrip(input);
            let _ = name;
        }
    }

    #[test]
    fn reject_bad_inputs() {
        assert!(parse("").is_err());
        assert!(parse("{").is_err());
        assert!(parse("[1,]").is_err());
        assert!(parse(r#"{"a":}"#).is_err());
        assert!(parse("01").is_err());
        assert!(parse("1.").is_err());
        assert!(parse("1e").is_err());
        assert!(parse(r#""\x""#).is_err());
    }

    #[test]
    fn pretty_print() {
        let v = parse(r#"{"a":[1,2],"b":null}"#).unwrap();
        let pretty = to_string_pretty(&v, 2);
        assert!(pretty.contains('\n'));
        assert_eq!(parse(&pretty).unwrap(), v);
    }

    #[test]
    fn object_small_map() {
        let mut obj = Object::new();
        for i in 0..20 {
            obj.insert(format!("k{i}"), Value::int(i as i64));
        }
        assert_eq!(obj.len(), 20);
        assert!(obj.get("k19").is_some());
    }

    #[test]
    fn is_valid_helper() {
        assert!(is_valid("[]"));
        assert!(!is_valid("[1,]"));
    }
}
