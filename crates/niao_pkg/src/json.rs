use niao_json_core::serde::{from_value, parse_json, to_string_pretty_value, to_value};
use niao_json_core::{parse, to_string_pretty, ParseError, Value};
use serde::{Deserialize, Serialize};

pub fn parse_struct<T: for<'de> Deserialize<'de>>(text: &str) -> Result<T, String> {
    parse_json(text)
}

pub fn stringify_pretty<T: Serialize>(value: &T) -> String {
    to_string_pretty_value(value).unwrap_or_default()
}

pub fn stringify_pretty_result<T: Serialize>(value: &T) -> Result<String, String> {
    to_string_pretty_value(value)
}

pub fn parse_value(text: &str) -> Result<Value, ParseError> {
    parse(text)
}

pub fn value_from_str(text: &str) -> Result<Value, String> {
    parse(text).map_err(|e| e.to_string())
}

pub fn value_to_string_pretty(value: &Value) -> String {
    to_string_pretty(value, 2)
}

pub fn struct_from_value<T: for<'de> Deserialize<'de>>(value: &Value) -> Result<T, String> {
    from_value(value)
}

pub fn struct_to_value<T: Serialize>(value: &T) -> Result<Value, String> {
    to_value(value)
}
