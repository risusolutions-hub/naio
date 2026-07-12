//! naws Lambda operations: invoke.

use super::{aws_error, get_config, ok_string, ok_value, AwsResult};
use crate::{Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::collections::HashMap;

use super::sigv4::{now_amz, sign, uri_encode, SignInput};

/// `naws.lambda_invoke(config_id, fn_name, payload) → {status, body}`
///
/// `fn_name` may be the function name, ARN, or `name:qualifier`.
/// `payload` may be a string (raw JSON) or an object (serialized to JSON).
pub fn lambda_invoke(args: &[ValueRef], span: Span) -> AwsResult {
    if args.len() != 3 {
        return Err(crate::RuntimeError::at(
            span,
            codes::E2800_NAWS_ARITY,
            "naws_lambda_invoke() expects 3 arguments: config, fn_name, payload",
        ));
    }
    let config_id = super::int_arg(args, 0, "naws_lambda_invoke", span)?;
    let cfg = get_config(config_id, span)?;
    let fn_name = super::str_arg(args, 1, "naws_lambda_invoke", span)?;
    let payload_str = payload_to_json(args[2].clone());

    let host = format!("lambda.{}.amazonaws.com", cfg.region);
    // URL-encode fn_name for path (colons for qualifiers are encoded as %3A in path but
    // Lambda's API accepts the raw colon too — use the safer encoded form).
    let encoded_name = uri_encode(&fn_name, true);
    let path = format!("/2015-03-31/functions/{encoded_name}/invocations");
    let body_bytes = payload_str.as_bytes();
    let (amz_dt, amz_d) = now_amz();

    let ct = "application/json";
    let extra = [("content-type", ct)];
    let inp = SignInput {
        method: "POST",
        host: &host,
        path: &path,
        query: "",
        region: &cfg.region,
        service: "lambda",
        access_key: &cfg.access_key,
        secret_key: &cfg.secret_key,
        session_token: cfg.session_token.as_deref(),
        body: body_bytes,
        amz_datetime: &amz_dt,
        amz_date: &amz_d,
        extra_headers: &extra,
    };
    let signed = sign(&inp);

    let url = format!("https://{}{}", host, path);
    let mut builder = niao_http::post(&url);
    for (k, v) in &signed.headers {
        builder = builder.set(k.clone(), v.clone());
    }
    builder = builder.set("Content-Type", ct);

    match builder.send_string(&payload_str) {
        Ok(resp) => {
            let status = resp.status as i64;
            let body_str = String::from_utf8_lossy(&resp.body).into_owned();
            if status >= 400 {
                return Ok(aws_error(
                    codes::E2801_NAWS_ERROR,
                    "naws_lambda_error",
                    body_str,
                    span,
                ));
            }
            let mut map = HashMap::new();
            map.insert("status".into(), ok_value(Value::Int(status)));
            map.insert("body".into(), ok_string(body_str));
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(aws_error(
            codes::E2801_NAWS_ERROR,
            "naws_lambda_error",
            e.to_string(),
            span,
        )),
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Convert a Niao value to a JSON string for the Lambda payload.
fn payload_to_json(val: ValueRef) -> String {
    match &*val.borrow() {
        Value::String(s) => s.clone(),
        Value::Nil => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Object(map) => {
            let fields: Vec<String> = map
                .iter()
                .map(|(k, v)| {
                    let escaped = super::json_escape(k);
                    format!("\"{}\":{}", escaped, payload_to_json(v.clone()))
                })
                .collect();
            format!("{{{}}}", fields.join(","))
        }
        Value::Array(items) => {
            let elems: Vec<String> = items.iter().map(|v| payload_to_json(v.clone())).collect();
            format!("[{}]", elems.join(","))
        }
        _ => "null".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_string_passthrough() {
        let v = Value::String(r#"{"key":"val"}"#.into()).ref_cell();
        assert_eq!(payload_to_json(v), r#"{"key":"val"}"#);
    }

    #[test]
    fn payload_object_serialized() {
        let mut map = HashMap::new();
        map.insert("x".into(), Value::Int(1).ref_cell());
        let v = Value::Object(map).ref_cell();
        let json = payload_to_json(v);
        assert!(json.contains("\"x\":1"));
    }
}
