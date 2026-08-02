//! Server method table and request dispatch (jsonrpcserver-style).

use crate::error::RpcError;
use crate::message::{
    invalid_request_response, parse_error_response, parse_request_value, Id, Request, Response,
};
use niao_json_core::{parse, Value};
use std::collections::HashMap;

/// Result returned by a method handler.
pub type MethodResult = Result<Value, RpcError>;

/// Dispatch a single JSON-RPC request value into zero or one responses.
///
/// Notifications yield `None`. Structural errors yield an error response when an
/// id is present (or null id for invalid requests where required by the spec).
pub fn dispatch_request_value<F>(v: &Value, call: F) -> Option<Response>
where
    F: FnOnce(&str, Option<&Value>) -> MethodResult,
{
    match parse_request_value(v) {
        Ok(req) => dispatch_request(&req, call),
        Err((err, id)) => {
            // Notifications that are structurally invalid: per spec, no response.
            // But if we never knew the id and the object looked like a notification
            // (no id key), omit the response only when the original object had no id.
            if id.is_none() {
                if let Value::Object(obj) = v {
                    if obj.get("id").is_none() && obj.get("method").is_some() {
                        return None;
                    }
                }
            }
            Some(Response::error(id.unwrap_or(Id::Null), err))
        }
    }
}

/// Dispatch an already-parsed request.
pub fn dispatch_request<F>(req: &Request, call: F) -> Option<Response>
where
    F: FnOnce(&str, Option<&Value>) -> MethodResult,
{
    let result = call(&req.method, req.params.as_ref());
    match &req.id {
        None => None, // notification — never respond
        Some(id) => Some(match result {
            Ok(value) => Response::success(id.clone(), value),
            Err(err) => Response::error(id.clone(), err),
        }),
    }
}

/// Dispatch a raw JSON string (single object or batch). Returns:
/// - `Ok(None)` for a lone notification with no response
/// - `Ok(Some(Value))` for a response object or batch array
/// - `Err` only for empty batch / non-JSON (returns parse-error response value)
pub fn dispatch_str<F>(input: &str, mut call: F) -> Value
where
    F: FnMut(&str, Option<&Value>) -> MethodResult,
{
    let parsed = match parse(input) {
        Ok(v) => v,
        Err(_) => return parse_error_response(None).to_value(),
    };
    dispatch_value(&parsed, &mut call)
}

pub fn dispatch_value<F>(v: &Value, call: &mut F) -> Value
where
    F: FnMut(&str, Option<&Value>) -> MethodResult,
{
    match v {
        Value::Array(items) => {
            if items.is_empty() {
                return invalid_request_response(None).to_value();
            }
            let mut out = Vec::new();
            for item in items {
                if let Some(resp) = dispatch_request_value(item, |m, p| call(m, p)) {
                    out.push(resp.to_value());
                }
            }
            if out.is_empty() {
                // All notifications — nothing to return (JSON-RPC allows no response body).
                return Value::Null;
            }
            Value::array(out)
        }
        other => match dispatch_request_value(other, |m, p| call(m, p)) {
            Some(resp) => resp.to_value(),
            None => Value::Null,
        },
    }
}

/// Simple in-memory method registry used by transport helpers and tests.
#[derive(Default)]
pub struct MethodTable {
    methods: HashMap<String, Box<dyn Fn(Option<&Value>) -> MethodResult + Send + Sync>>,
}

impl MethodTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<F>(&mut self, name: impl Into<String>, handler: F)
    where
        F: Fn(Option<&Value>) -> MethodResult + Send + Sync + 'static,
    {
        self.methods.insert(name.into(), Box::new(handler));
    }

    pub fn contains(&self, name: &str) -> bool {
        self.methods.contains_key(name)
    }

    pub fn names(&self) -> Vec<String> {
        let mut n: Vec<_> = self.methods.keys().cloned().collect();
        n.sort();
        n
    }

    pub fn call(&self, method: &str, params: Option<&Value>) -> MethodResult {
        match self.methods.get(method) {
            Some(h) => h(params),
            None => Err(RpcError::method_not_found(method)),
        }
    }

    pub fn dispatch_str(&self, input: &str) -> Value {
        dispatch_str(input, |m, p| self.call(m, p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_json_core::to_string;

    #[test]
    fn success_call() {
        let mut table = MethodTable::new();
        table.register("ping", |_| Ok(Value::string("pong")));
        let out = table.dispatch_str(r#"{"jsonrpc":"2.0","method":"ping","id":1}"#);
        let s = to_string(&out);
        assert!(s.contains("\"result\":\"pong\""));
    }

    #[test]
    fn method_not_found() {
        let table = MethodTable::new();
        let out = table.dispatch_str(r#"{"jsonrpc":"2.0","method":"nope","id":1}"#);
        let s = to_string(&out);
        assert!(s.contains("-32601"));
    }

    #[test]
    fn notification_returns_null() {
        let mut table = MethodTable::new();
        table.register("log", |_| Ok(Value::Null));
        let out = table.dispatch_str(r#"{"jsonrpc":"2.0","method":"log"}"#);
        assert!(out.is_null());
    }

    #[test]
    fn batch_mixed() {
        let mut table = MethodTable::new();
        table.register("add", |p| {
            let arr = p.and_then(|v| match v {
                Value::Array(a) => Some(a),
                _ => None,
            });
            let a = arr
                .and_then(|a| a.first())
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let b = arr
                .and_then(|a| a.get(1))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            Ok(Value::int(a + b))
        });
        let out = table.dispatch_str(
            r#"[{"jsonrpc":"2.0","method":"add","params":[1,2],"id":1},{"jsonrpc":"2.0","method":"add","params":[3,4]}]"#,
        );
        match out {
            Value::Array(items) => assert_eq!(items.len(), 1),
            _ => panic!("expected single response in batch"),
        }
    }
}
