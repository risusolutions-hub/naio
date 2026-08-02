//! Cloud Functions HTTP trigger invoke.

use super::{
    bearer_auth, gcp_error, ok_string, ok_value, value_to_json_string, with_config_mut, GcpResult,
};
use crate::{Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::collections::HashMap;

fn fn_error(span: Span, msg: impl Into<String>) -> ValueRef {
    gcp_error(codes::E4541_NGCP_ERROR, "ngcp_function_error", msg, span)
}

/// `ngcp.function_invoke(cfg, url, payload, method?) → {status, body}`
///
/// Invokes an HTTP Cloud Function (or Cloud Run) endpoint. `payload` may be a
/// string (sent as-is) or object/array (JSON-serialised). Default method: POST.
///
/// // >>> ngcp.function_invoke != nil
/// // => true
pub fn function_invoke(args: &[ValueRef], span: Span) -> GcpResult {
    if args.len() < 3 || args.len() > 4 {
        return Err(crate::RuntimeError::at(
            span,
            codes::E4540_NGCP_ARITY,
            "ngcp_function_invoke() expects 3-4 arguments: config, url, payload, method?",
        ));
    }
    let id = super::int_arg(args, 0, "ngcp_function_invoke", span)?;
    let url = super::str_arg(args, 1, "ngcp_function_invoke", span)?;
    if url.is_empty() {
        return Ok(gcp_error(
            codes::E4542_NGCP_TYPE,
            "ngcp_error",
            "ngcp_function_invoke() url must be non-empty",
            span,
        ));
    }
    let payload = match &*args[2].borrow() {
        Value::String(s) => s.clone(),
        Value::Nil => "null".to_string(),
        other => value_to_json_string(other, span)?,
    };
    let method = if args.len() == 4 {
        super::str_arg(args, 3, "ngcp_function_invoke", span)?.to_uppercase()
    } else {
        "POST".to_string()
    };

    match with_config_mut(id, span, |cfg| {
        let token = match bearer_auth(cfg) {
            Ok(t) => t,
            Err(e) => return fn_error(span, e),
        };
        let builder = match method.as_str() {
            "GET" => niao_http::get(&url),
            "PUT" => niao_http::put(&url),
            "DELETE" => niao_http::delete(&url),
            "PATCH" => niao_http::request(niao_http::Method::Patch, &url),
            _ => niao_http::post(&url),
        };
        let result = if method == "GET" || method == "DELETE" {
            builder
                .set("Authorization", format!("Bearer {token}"))
                .send()
        } else {
            builder
                .set("Authorization", format!("Bearer {token}"))
                .set("Content-Type", "application/json")
                .send_string(&payload)
        };
        match result {
            Ok(resp) => {
                let status = resp.status as i64;
                let body = String::from_utf8_lossy(&resp.body).into_owned();
                if status >= 400 {
                    return fn_error(span, format!("HTTP {status}: {body}"));
                }
                let mut map = HashMap::new();
                map.insert("status".into(), ok_value(Value::Int(status)));
                map.insert("body".into(), ok_string(body));
                Value::Object(map).ref_cell()
            }
            Err(e) => fn_error(span, e.to_string()),
        }
    }) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}
