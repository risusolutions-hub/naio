//! JSON-RPC 2.0 message types.

use crate::error::{codes, EngineError, RpcError};
use niao_json_core::{Object, Value};

/// Request id: string, number, or null (null only on responses to invalid requests).
#[derive(Clone, Debug, PartialEq)]
pub enum Id {
    Null,
    Number(i64),
    String(String),
}

impl Id {
    pub fn from_value(v: &Value) -> Result<Self, EngineError> {
        match v {
            Value::Null => Ok(Id::Null),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(Id::Number(i))
                } else if let Some(f) = n.as_f64() {
                    if f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64 {
                        Ok(Id::Number(f as i64))
                    } else {
                        Err(EngineError::Invalid(
                            "JSON-RPC id number must be an integer".into(),
                        ))
                    }
                } else {
                    Err(EngineError::Invalid(
                        "JSON-RPC id number is not representable".into(),
                    ))
                }
            }
            Value::String(s) => Ok(Id::String(s.clone())),
            _ => Err(EngineError::Invalid(
                "JSON-RPC id must be string, number, or null".into(),
            )),
        }
    }

    pub fn to_value(&self) -> Value {
        match self {
            Id::Null => Value::Null,
            Id::Number(n) => Value::int(*n),
            Id::String(s) => Value::string(s.clone()),
        }
    }
}

/// A JSON-RPC 2.0 request (has id) or notification (no id).
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub method: String,
    pub params: Option<Value>,
    pub id: Option<Id>,
}

impl Request {
    pub fn call(method: impl Into<String>, params: Option<Value>, id: Id) -> Self {
        Self {
            method: method.into(),
            params,
            id: Some(id),
        }
    }

    pub fn notify(method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            method: method.into(),
            params,
            id: None,
        }
    }

    pub fn is_notification(&self) -> bool {
        self.id.is_none()
    }

    pub fn to_value(&self) -> Value {
        let mut obj = Object::with_capacity(4);
        obj.insert("jsonrpc".into(), Value::string("2.0"));
        obj.insert("method".into(), Value::string(self.method.clone()));
        if let Some(ref p) = self.params {
            obj.insert("params".into(), p.clone());
        }
        if let Some(ref id) = self.id {
            obj.insert("id".into(), id.to_value());
        }
        Value::object(obj)
    }
}

/// A JSON-RPC 2.0 response (success or error).
#[derive(Clone, Debug, PartialEq)]
pub struct Response {
    pub id: Id,
    pub body: ResponseBody,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ResponseBody {
    Success(Value),
    Error(RpcError),
}

impl Response {
    pub fn success(id: Id, result: Value) -> Self {
        Self {
            id,
            body: ResponseBody::Success(result),
        }
    }

    pub fn error(id: Id, err: RpcError) -> Self {
        Self {
            id,
            body: ResponseBody::Error(err),
        }
    }

    pub fn is_error(&self) -> bool {
        matches!(self.body, ResponseBody::Error(_))
    }

    pub fn to_value(&self) -> Value {
        let mut obj = Object::with_capacity(3);
        obj.insert("jsonrpc".into(), Value::string("2.0"));
        obj.insert("id".into(), self.id.to_value());
        match &self.body {
            ResponseBody::Success(v) => {
                obj.insert("result".into(), v.clone());
            }
            ResponseBody::Error(e) => {
                let mut err = Object::with_capacity(3);
                err.insert("code".into(), Value::int(e.code));
                err.insert("message".into(), Value::string(e.message.clone()));
                if let Some(ref data) = e.data {
                    err.insert("data".into(), data.clone());
                }
                obj.insert("error".into(), Value::object(err));
            }
        }
        Value::object(obj)
    }
}

/// Top-level wire message: single request/response or a batch.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    Request(Request),
    Response(Response),
    Batch(Vec<Message>),
}

impl Message {
    pub fn to_value(&self) -> Value {
        match self {
            Message::Request(r) => r.to_value(),
            Message::Response(r) => r.to_value(),
            Message::Batch(items) => Value::array(items.iter().map(|m| m.to_value()).collect()),
        }
    }

    pub fn is_batch(&self) -> bool {
        matches!(self, Message::Batch(_))
    }
}

fn require_jsonrpc(obj: &Object) -> Result<(), RpcError> {
    match obj.get("jsonrpc") {
        Some(Value::String(s)) if s == "2.0" => Ok(()),
        Some(_) => Err(RpcError::invalid_request(
            "jsonrpc member must be the string \"2.0\"",
        )),
        None => Err(RpcError::invalid_request("missing jsonrpc member")),
    }
}

/// Parse a JSON value into a request (used by dispatch). On structural failure,
/// returns an `RpcError` suitable for an error response (id may be unknown).
pub fn parse_request_value(v: &Value) -> Result<Request, (RpcError, Option<Id>)> {
    let obj = match v {
        Value::Object(o) => o,
        _ => {
            return Err((
                RpcError::invalid_request("request must be a JSON object"),
                None,
            ))
        }
    };
    if let Err(e) = require_jsonrpc(obj) {
        let id = obj.get("id").and_then(|v| Id::from_value(v).ok());
        return Err((e, id));
    }
    let id = match obj.get("id") {
        Some(v) => match Id::from_value(v) {
            Ok(id) => Some(id),
            Err(_) => {
                return Err((
                    RpcError::invalid_request("invalid id member"),
                    Some(Id::Null),
                ))
            }
        },
        None => None,
    };
    let method = match obj.get("method") {
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        Some(Value::String(_)) => {
            return Err((
                RpcError::invalid_request("method must be a non-empty string"),
                id,
            ))
        }
        Some(_) => {
            return Err((RpcError::invalid_request("method must be a string"), id));
        }
        None => {
            return Err((RpcError::invalid_request("missing method member"), id));
        }
    };
    // Notifications must not be confused with responses; reject result/error keys.
    if obj.get("result").is_some() || obj.get("error").is_some() {
        return Err((
            RpcError::invalid_request("request must not contain result or error"),
            id,
        ));
    }
    let params = match obj.get("params") {
        None => None,
        Some(Value::Array(_)) | Some(Value::Object(_)) => Some(obj.get("params").unwrap().clone()),
        Some(_) => {
            return Err((
                RpcError::invalid_request("params must be array or object"),
                id,
            ));
        }
    };
    Ok(Request { method, params, id })
}

/// Parse a JSON value as a response.
pub fn parse_response_value(v: &Value) -> Result<Response, EngineError> {
    let obj = match v {
        Value::Object(o) => o,
        _ => {
            return Err(EngineError::Invalid(
                "response must be a JSON object".into(),
            ))
        }
    };
    match obj.get("jsonrpc") {
        Some(Value::String(s)) if s == "2.0" => {}
        _ => {
            return Err(EngineError::Invalid(
                "jsonrpc member must be the string \"2.0\"".into(),
            ))
        }
    }
    let id = match obj.get("id") {
        Some(v) => Id::from_value(v)?,
        None => return Err(EngineError::Invalid("response missing id".into())),
    };
    let has_result = obj.get("result").is_some();
    let has_error = obj.get("error").is_some();
    if has_result == has_error {
        return Err(EngineError::Invalid(
            "response must contain exactly one of result or error".into(),
        ));
    }
    if has_result {
        return Ok(Response::success(id, obj.get("result").unwrap().clone()));
    }
    let err_obj = match obj.get("error") {
        Some(Value::Object(o)) => o,
        _ => {
            return Err(EngineError::Invalid(
                "error member must be an object".into(),
            ))
        }
    };
    let code = match err_obj.get("code") {
        Some(Value::Number(n)) => n
            .as_i64()
            .ok_or_else(|| EngineError::Invalid("error.code must be an integer".into()))?,
        _ => return Err(EngineError::Invalid("error missing code".into())),
    };
    let message = match err_obj.get("message") {
        Some(Value::String(s)) => s.clone(),
        _ => return Err(EngineError::Invalid("error missing message string".into())),
    };
    let data = err_obj.get("data").cloned();
    Ok(Response::error(
        id,
        RpcError {
            code,
            message,
            data,
        },
    ))
}

/// Classify and parse a top-level JSON value into a [`Message`].
pub fn parse_message_value(v: &Value) -> Result<Message, EngineError> {
    match v {
        Value::Array(items) => {
            if items.is_empty() {
                return Err(EngineError::Invalid(
                    "batch must be a non-empty array".into(),
                ));
            }
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(parse_message_value(item)?);
            }
            Ok(Message::Batch(out))
        }
        Value::Object(obj) => {
            if obj.get("method").is_some() {
                match parse_request_value(v) {
                    Ok(r) => Ok(Message::Request(r)),
                    Err((e, _)) => Err(EngineError::Invalid(e.message)),
                }
            } else if obj.get("result").is_some() || obj.get("error").is_some() {
                Ok(Message::Response(parse_response_value(v)?))
            } else {
                Err(EngineError::Invalid(
                    "object is neither a JSON-RPC request nor response".into(),
                ))
            }
        }
        _ => Err(EngineError::Invalid(
            "JSON-RPC message must be an object or array".into(),
        )),
    }
}

/// Build standard error response helpers.
pub fn parse_error_response(id: Option<Id>) -> Response {
    Response::error(
        id.unwrap_or(Id::Null),
        RpcError::parse_error(RpcError::default_message(codes::PARSE_ERROR)),
    )
}

pub fn invalid_request_response(id: Option<Id>) -> Response {
    Response::error(
        id.unwrap_or(Id::Null),
        RpcError::invalid_request(RpcError::default_message(codes::INVALID_REQUEST)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_json_core::parse;

    #[test]
    fn request_roundtrip_value() {
        let req = Request::call(
            "sum",
            Some(Value::array(vec![Value::int(1), Value::int(2)])),
            Id::Number(1),
        );
        let v = req.to_value();
        let back = parse_request_value(&v).unwrap();
        assert_eq!(back.method, "sum");
        assert_eq!(back.id, Some(Id::Number(1)));
    }

    #[test]
    fn notification_has_no_id() {
        let n = Request::notify("ping", None);
        assert!(n.is_notification());
        let s = niao_json_core::to_string(&n.to_value());
        assert!(!s.contains("\"id\""));
    }

    #[test]
    fn parse_success_response() {
        let v = parse(r#"{"jsonrpc":"2.0","result":42,"id":1}"#).unwrap();
        let r = parse_response_value(&v).unwrap();
        assert!(!r.is_error());
        assert_eq!(r.id, Id::Number(1));
    }
}
