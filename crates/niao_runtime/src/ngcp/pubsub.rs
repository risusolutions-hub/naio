//! Pub/Sub REST — publish / pull / acknowledge.

use super::{
    bearer_auth, gcp_error, json_escape, ok_string, ok_value, with_config_mut, GcpResult,
};
use crate::{Value, ValueRef};
use niao_ast::Span;
use niao_codec::base64;
use niao_errors::codes;
use std::collections::HashMap;

fn ps_error(span: Span, msg: impl Into<String>) -> ValueRef {
    gcp_error(codes::E4541_NGCP_ERROR, "ngcp_pubsub_error", msg, span)
}

/// `ngcp.pubsub_publish(cfg, topic, data, attrs?) → {message_ids[]}`
///
/// // >>> ngcp.pubsub_publish != nil
/// // => true
pub fn pubsub_publish(args: &[ValueRef], span: Span) -> GcpResult {
    if args.len() < 3 || args.len() > 4 {
        return Err(crate::RuntimeError::at(
            span,
            codes::E4540_NGCP_ARITY,
            "ngcp_pubsub_publish() expects 3-4 arguments: config, topic, data, attrs?",
        ));
    }
    let id = super::int_arg(args, 0, "ngcp_pubsub_publish", span)?;
    let topic = super::str_arg(args, 1, "ngcp_pubsub_publish", span)?;
    let data = super::bytes_arg(args, 2, "ngcp_pubsub_publish", span)?;
    let attrs_json = if args.len() == 4 {
        match &*args[3].borrow() {
            Value::Object(m) => {
                let mut parts = Vec::new();
                for (k, v) in m {
                    let vs = match &*v.borrow() {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    parts.push(format!("\"{}\":\"{}\"", json_escape(k), json_escape(&vs)));
                }
                Some(format!("{{{}}}", parts.join(",")))
            }
            Value::Nil => None,
            other => {
                return Ok(gcp_error(
                    codes::E4542_NGCP_TYPE,
                    "ngcp_error",
                    format!(
                        "ngcp_pubsub_publish() attrs expects object, got {}",
                        other.type_name()
                    ),
                    span,
                ));
            }
        }
    } else {
        None
    };

    match with_config_mut(id, span, |cfg| {
        let token = match bearer_auth(cfg) {
            Ok(t) => t,
            Err(e) => return ps_error(span, e),
        };
        let b64 = base64::encode_standard(&data);
        let msg = if let Some(attrs) = attrs_json {
            format!(
                "{{\"data\":\"{}\",\"attributes\":{}}}",
                json_escape(&b64),
                attrs
            )
        } else {
            format!("{{\"data\":\"{}\"}}", json_escape(&b64))
        };
        let body = format!("{{\"messages\":[{msg}]}}");
        let url = format!(
            "https://pubsub.googleapis.com/v1/projects/{}/topics/{}:publish",
            crate::ngcp::auth::uri_encode_path(&cfg.project),
            crate::ngcp::auth::uri_encode_path(&topic)
        );
        match niao_http::post(&url)
            .set("Authorization", format!("Bearer {token}"))
            .set("Content-Type", "application/json")
            .send_string(&body)
        {
            Ok(resp) => {
                if (resp.status as i64) >= 400 {
                    return ps_error(span, String::from_utf8_lossy(&resp.body));
                }
                let text = String::from_utf8_lossy(&resp.body);
                let ids = extract_string_array(&text, "messageIds");
                let arr: Vec<ValueRef> = ids.into_iter().map(ok_string).collect();
                let mut map = HashMap::new();
                map.insert("message_ids".into(), ok_value(Value::Array(arr)));
                Value::Object(map).ref_cell()
            }
            Err(e) => ps_error(span, e.to_string()),
        }
    }) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// `ngcp.pubsub_pull(cfg, subscription, max?) → messages[]`
///
/// Each message: `{ack_id, data, attributes{}, message_id}`.
///
/// // >>> ngcp.pubsub_pull != nil
/// // => true
pub fn pubsub_pull(args: &[ValueRef], span: Span) -> GcpResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(crate::RuntimeError::at(
            span,
            codes::E4540_NGCP_ARITY,
            "ngcp_pubsub_pull() expects 2-3 arguments: config, subscription, max?",
        ));
    }
    let id = super::int_arg(args, 0, "ngcp_pubsub_pull", span)?;
    let sub = super::str_arg(args, 1, "ngcp_pubsub_pull", span)?;
    let max = if args.len() == 3 {
        super::int_arg(args, 2, "ngcp_pubsub_pull", span)?.max(1)
    } else {
        10
    };

    match with_config_mut(id, span, |cfg| {
        let token = match bearer_auth(cfg) {
            Ok(t) => t,
            Err(e) => return ps_error(span, e),
        };
        let body = format!("{{\"maxMessages\":{max}}}");
        let url = format!(
            "https://pubsub.googleapis.com/v1/projects/{}/subscriptions/{}:pull",
            crate::ngcp::auth::uri_encode_path(&cfg.project),
            crate::ngcp::auth::uri_encode_path(&sub)
        );
        match niao_http::post(&url)
            .set("Authorization", format!("Bearer {token}"))
            .set("Content-Type", "application/json")
            .send_string(&body)
        {
            Ok(resp) => {
                if (resp.status as i64) >= 400 {
                    return ps_error(span, String::from_utf8_lossy(&resp.body));
                }
                let text = String::from_utf8_lossy(&resp.body);
                Value::Array(parse_received_messages(&text)).ref_cell()
            }
            Err(e) => ps_error(span, e.to_string()),
        }
    }) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// `ngcp.pubsub_ack(cfg, subscription, ack_ids[]) → true`
///
/// // >>> ngcp.pubsub_ack != nil
/// // => true
pub fn pubsub_ack(args: &[ValueRef], span: Span) -> GcpResult {
    if args.len() != 3 {
        return Err(crate::RuntimeError::at(
            span,
            codes::E4540_NGCP_ARITY,
            "ngcp_pubsub_ack() expects 3 arguments: config, subscription, ack_ids",
        ));
    }
    let id = super::int_arg(args, 0, "ngcp_pubsub_ack", span)?;
    let sub = super::str_arg(args, 1, "ngcp_pubsub_ack", span)?;
    let ids = match &*args[2].borrow() {
        Value::Array(arr) => {
            let mut out = Vec::new();
            for v in arr {
                match &*v.borrow() {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Ok(gcp_error(
                            codes::E4542_NGCP_TYPE,
                            "ngcp_error",
                            format!(
                                "ngcp_pubsub_ack() ack_ids must be strings, got {}",
                                other.type_name()
                            ),
                            span,
                        ));
                    }
                }
            }
            out
        }
        other => {
            return Ok(gcp_error(
                codes::E4542_NGCP_TYPE,
                "ngcp_error",
                format!(
                    "ngcp_pubsub_ack() expects ack_ids array, got {}",
                    other.type_name()
                ),
                span,
            ));
        }
    };

    match with_config_mut(id, span, |cfg| {
        let token = match bearer_auth(cfg) {
            Ok(t) => t,
            Err(e) => return ps_error(span, e),
        };
        let id_list = ids
            .iter()
            .map(|s| format!("\"{}\"", json_escape(s)))
            .collect::<Vec<_>>()
            .join(",");
        let body = format!("{{\"ackIds\":[{id_list}]}}");
        let url = format!(
            "https://pubsub.googleapis.com/v1/projects/{}/subscriptions/{}:acknowledge",
            crate::ngcp::auth::uri_encode_path(&cfg.project),
            crate::ngcp::auth::uri_encode_path(&sub)
        );
        match niao_http::post(&url)
            .set("Authorization", format!("Bearer {token}"))
            .set("Content-Type", "application/json")
            .send_string(&body)
        {
            Ok(resp) => {
                if (resp.status as i64) >= 400 {
                    return ps_error(span, String::from_utf8_lossy(&resp.body));
                }
                Value::Bool(true).ref_cell()
            }
            Err(e) => ps_error(span, e.to_string()),
        }
    }) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

fn extract_string_array(json: &str, key: &str) -> Vec<String> {
    let needle = format!("\"{key}\"");
    let Some(start) = json.find(&needle) else {
        return Vec::new();
    };
    let after = json[start + needle.len()..].trim_start();
    let Some(after) = after.strip_prefix(':') else {
        return Vec::new();
    };
    let after = after.trim_start();
    let Some(after) = after.strip_prefix('[') else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut rest = after;
    loop {
        rest = rest.trim_start();
        if rest.starts_with(']') || rest.is_empty() {
            break;
        }
        if let Some(r) = rest.strip_prefix('"') {
            if let Some(end) = r.find('"') {
                out.push(r[..end].to_string());
                rest = r[end + 1..].trim_start().trim_start_matches(',');
                continue;
            }
        }
        break;
    }
    out
}

fn parse_received_messages(json: &str) -> Vec<ValueRef> {
    // Lightweight split on receivedMessages objects.
    let mut out = Vec::new();
    let mut rest = json;
    while let Some(idx) = rest.find("\"ackId\"") {
        let chunk_end = rest[idx..]
            .find("},{")
            .map(|i| idx + i)
            .unwrap_or(rest.len());
        let chunk = &rest[idx..chunk_end];
        let ack = extract_field(chunk, "ackId").unwrap_or_default();
        let data_b64 = extract_field(chunk, "data").unwrap_or_default();
        let data = base64::decode_standard(&data_b64)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or(data_b64);
        let mid = extract_field(chunk, "messageId").unwrap_or_default();
        let mut map = HashMap::new();
        map.insert("ack_id".into(), ok_string(ack));
        map.insert("data".into(), ok_string(data));
        map.insert("message_id".into(), ok_string(mid));
        map.insert(
            "attributes".into(),
            ok_value(Value::Object(HashMap::new())),
        );
        out.push(Value::Object(map).ref_cell());
        rest = &rest[chunk_end..];
        if rest.starts_with("},{") {
            rest = &rest[2..];
        }
    }
    out
}

fn extract_field(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let start = json.find(&needle)?;
    let after = json[start + needle.len()..].trim_start();
    let after = after.strip_prefix(':')?.trim_start();
    let after = after.strip_prefix('"')?;
    let end = after.find('"')?;
    Some(after[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_string_array_message_ids() {
        let j = r#"{"messageIds":["1","2","3"]}"#;
        assert_eq!(extract_string_array(j, "messageIds"), vec!["1", "2", "3"]);
    }

    #[test]
    fn parse_received_messages_one() {
        let j = r#"{"receivedMessages":[{"ackId":"ack1","message":{"data":"aGVsbG8=","messageId":"m1"}}]}"#;
        let msgs = parse_received_messages(j);
        assert_eq!(msgs.len(), 1);
        let cell = msgs[0].borrow();
        match &*cell {
            Value::Object(m) => {
                assert_eq!(
                    match &*m.get("ack_id").unwrap().borrow() {
                        Value::String(s) => s.as_str(),
                        _ => "",
                    },
                    "ack1"
                );
                assert_eq!(
                    match &*m.get("data").unwrap().borrow() {
                        Value::String(s) => s.as_str(),
                        _ => "",
                    },
                    "hello"
                );
            }
            _ => panic!("expected object"),
        }
    }
}
