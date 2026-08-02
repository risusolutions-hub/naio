//! Encode / decode JSON-RPC messages as text.

use crate::error::EngineError;
use crate::message::{parse_message_value, Message};
use niao_json_core::{parse, to_string, Value};

/// Maximum accepted payload size (16 MiB).
pub const MAX_BYTES: usize = 16 * 1024 * 1024;

/// Encode a message value tree to compact JSON text.
pub fn encode_value(v: &Value) -> String {
    to_string(v)
}

/// Encode a [`Message`] to compact JSON text.
pub fn encode(msg: &Message) -> String {
    to_string(&msg.to_value())
}

/// Encode a batch of values (already JSON-RPC objects) as a JSON array.
pub fn encode_batch_values(items: &[Value]) -> Result<String, EngineError> {
    if items.is_empty() {
        return Err(EngineError::Invalid("batch must be non-empty".into()));
    }
    Ok(to_string(&Value::array(items.to_vec())))
}

/// Decode JSON text into a [`Message`].
pub fn decode(text: &str) -> Result<Message, EngineError> {
    if text.len() > MAX_BYTES {
        return Err(EngineError::Limit(format!(
            "payload exceeds {MAX_BYTES} bytes"
        )));
    }
    let v = parse(text)?;
    parse_message_value(&v)
}

/// Decode JSON text into a raw [`Value`] (no RPC validation beyond JSON).
pub fn decode_raw(text: &str) -> Result<Value, EngineError> {
    if text.len() > MAX_BYTES {
        return Err(EngineError::Limit(format!(
            "payload exceeds {MAX_BYTES} bytes"
        )));
    }
    Ok(parse(text)?)
}

/// True when `text` is a valid JSON-RPC request, response, or non-empty batch.
pub fn valid(text: &str) -> bool {
    decode(text).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{Id, Request};

    #[test]
    fn encode_decode_request() {
        let msg = Message::Request(Request::call("echo", None, Id::Number(7)));
        let text = encode(&msg);
        let back = decode(&text).unwrap();
        match back {
            Message::Request(r) => {
                assert_eq!(r.method, "echo");
                assert_eq!(r.id, Some(Id::Number(7)));
            }
            _ => panic!("expected request"),
        }
    }

    #[test]
    fn reject_empty_batch() {
        assert!(decode("[]").is_err());
    }

    #[test]
    fn valid_helpers() {
        assert!(valid(r#"{"jsonrpc":"2.0","method":"x","id":1}"#));
        assert!(!valid("{"));
        assert!(!valid("null"));
    }
}
