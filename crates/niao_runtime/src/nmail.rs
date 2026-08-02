//! Native nmail standard library — MIME email compose + parse (~email, pairs nsmtp).
//!
//! Import with `import "nmail"` (or `import "std/nmail"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_mail::{
    add_inline, attach, decode_header, emit, emit_bytes, emit_file, encode_header, format_addr,
    format_date, is_valid, make_msgid, parse, parse_addr, parse_addrs, parse_bytes, parse_file,
    BuildSpec, EmitOptions, MailError, MailMessage, ParseOptions, Attachment, InlinePart,
    MAX_BYTES,
};
use std::collections::HashMap;
use std::rc::Rc;

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E2895_NMAIL_TYPE, msg.into())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E2893_NMAIL_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn nmail_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E2894_NMAIL_ERROR, "nmail_error", msg.into(), span)
}

fn map_mail_err(span: Span, err: MailError) -> ValueRef {
    let code = if err.is_parse() {
        codes::E2896_NMAIL_PARSE
    } else {
        codes::E2894_NMAIL_ERROR
    };
    error_value(code, "nmail_error", err.message(), span)
}

fn string_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn optional_object_arg(args: &[ValueRef], idx: usize) -> Option<HashMap<String, ValueRef>> {
    if args.len() <= idx {
        return None;
    }
    match &*args[idx].borrow() {
        Value::Object(map) => Some(map.clone()),
        Value::Nil => None,
        _ => None,
    }
}

fn object_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<HashMap<String, ValueRef>> {
    match &*args[idx].borrow() {
        Value::Object(map) => Ok(map.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an object as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn string_field(map: &HashMap<String, ValueRef>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.clone()),
        Value::Int(n) => Some(n.to_string()),
        Value::Nil => None,
        _ => None,
    })
}

fn bool_field(map: &HashMap<String, ValueRef>, key: &str, default: bool) -> bool {
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Bool(b)) => b,
        Some(Value::Int(n)) => n != 0,
        _ => default,
    }
}

fn int_field(map: &HashMap<String, ValueRef>, key: &str) -> Option<i64> {
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Int(n)) => Some(n),
        _ => None,
    }
}

fn bytes_from_value(v: &Value, span: Span, ctx: &str) -> NiaoResult<Vec<u8>> {
    match v {
        Value::ByteArray(b) => Ok(b.clone()),
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match &*item.borrow() {
                    Value::Int(n) if (0..=255).contains(n) => out.push(*n as u8),
                    other => {
                        return Err(type_err(
                            span,
                            format!("{ctx}: byte array items must be 0..=255 ints, got {}", other.type_name()),
                        ));
                    }
                }
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!("{ctx}: expected bytes/string/array, got {}", other.type_name()),
        )),
    }
}

fn recipients_field(
    map: &HashMap<String, ValueRef>,
    key: &str,
    span: Span,
) -> NiaoResult<Vec<String>> {
    match map.get(key) {
        None => Ok(Vec::new()),
        Some(v) => match &*v.borrow() {
            Value::String(s) if s.is_empty() => Ok(Vec::new()),
            Value::String(s) => Ok(vec![s.clone()]),
            Value::Array(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    match &*item.borrow() {
                        Value::String(s) if !s.is_empty() => out.push(s.clone()),
                        Value::String(_) => {
                            return Err(type_err(span, format!("config.{key} items must not be empty")));
                        }
                        Value::Object(o) => {
                            let email = string_field(o, "email")
                                .ok_or_else(|| type_err(span, format!("config.{key} object needs email")))?;
                            let name = string_field(o, "name");
                            out.push(
                                format_addr(name.as_deref(), &email)
                                    .map_err(|e| type_err(span, e.message()))?,
                            );
                        }
                        other => {
                            return Err(type_err(
                                span,
                                format!(
                                    "config.{key} items must be strings or {{name,email}}, got {}",
                                    other.type_name()
                                ),
                            ));
                        }
                    }
                }
                Ok(out)
            }
            Value::Nil => Ok(Vec::new()),
            other => Err(type_err(
                span,
                format!(
                    "config.{key} must be string or array, got {}",
                    other.type_name()
                ),
            )),
        },
    }
}

fn attachments_from_map(
    map: &HashMap<String, ValueRef>,
    span: Span,
) -> NiaoResult<Vec<Attachment>> {
    let Some(v) = map.get("attachments") else {
        return Ok(Vec::new());
    };
    let items = match &*v.borrow() {
        Value::Array(a) => a.clone(),
        Value::Nil => return Ok(Vec::new()),
        other => {
            return Err(type_err(
                span,
                format!("config.attachments must be an array, got {}", other.type_name()),
            ));
        }
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let obj = match &*item.borrow() {
            Value::Object(m) => m.clone(),
            other => {
                return Err(type_err(
                    span,
                    format!("attachment must be an object, got {}", other.type_name()),
                ));
            }
        };
        let filename = string_field(&obj, "filename").or_else(|| string_field(&obj, "name"));
        let content_type =
            string_field(&obj, "content_type").unwrap_or_else(|| "application/octet-stream".into());
        let disposition = string_field(&obj, "disposition").unwrap_or_else(|| "attachment".into());
        let data = match obj.get("data").or_else(|| obj.get("content")) {
            Some(dv) => bytes_from_value(&dv.borrow(), span, "attachment.data")?,
            None => {
                return Err(type_err(span, "attachment requires data/content"));
            }
        };
        out.push(Attachment {
            filename,
            content_type,
            disposition,
            data,
        });
    }
    Ok(out)
}

fn inline_from_map(map: &HashMap<String, ValueRef>, span: Span) -> NiaoResult<Vec<InlinePart>> {
    let Some(v) = map.get("inline").or_else(|| map.get("related")) else {
        return Ok(Vec::new());
    };
    let items = match &*v.borrow() {
        Value::Array(a) => a.clone(),
        Value::Nil => return Ok(Vec::new()),
        other => {
            return Err(type_err(
                span,
                format!("config.inline must be an array, got {}", other.type_name()),
            ));
        }
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let obj = match &*item.borrow() {
            Value::Object(m) => m.clone(),
            other => {
                return Err(type_err(
                    span,
                    format!("inline part must be an object, got {}", other.type_name()),
                ));
            }
        };
        let cid = string_field(&obj, "cid")
            .or_else(|| string_field(&obj, "content_id"))
            .ok_or_else(|| type_err(span, "inline part requires cid"))?;
        let filename = string_field(&obj, "filename");
        let content_type =
            string_field(&obj, "content_type").unwrap_or_else(|| "application/octet-stream".into());
        let data = match obj.get("data").or_else(|| obj.get("content")) {
            Some(dv) => bytes_from_value(&dv.borrow(), span, "inline.data")?,
            None => return Err(type_err(span, "inline part requires data/content")),
        };
        out.push(InlinePart {
            cid,
            filename,
            content_type,
            data,
        });
    }
    Ok(out)
}

fn headers_from_map(map: &HashMap<String, ValueRef>) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    if let Some(v) = map.get("headers") {
        if let Value::Object(h) = &*v.borrow() {
            for (k, vv) in h {
                if let Some(s) = match &*vv.borrow() {
                    Value::String(s) => Some(s.clone()),
                    Value::Int(n) => Some(n.to_string()),
                    _ => None,
                } {
                    out.insert(k.to_ascii_lowercase(), s);
                }
            }
        }
    }
    out
}

fn build_spec_from_map(map: &HashMap<String, ValueRef>, span: Span) -> NiaoResult<BuildSpec> {
    let from = string_field(map, "from");
    let to = recipients_field(map, "to", span)?;
    let cc = recipients_field(map, "cc", span)?;
    let bcc = recipients_field(map, "bcc", span)?;
    Ok(BuildSpec {
        from,
        to,
        cc,
        bcc,
        reply_to: string_field(map, "reply_to"),
        subject: string_field(map, "subject"),
        text: string_field(map, "text").or_else(|| string_field(map, "body")),
        html: string_field(map, "html"),
        date: string_field(map, "date"),
        message_id: string_field(map, "message_id"),
        headers: headers_from_map(map),
        attachments: attachments_from_map(map, span)?,
        inline: inline_from_map(map, span)?,
        auto_date: bool_field(map, "auto_date", true),
        auto_message_id: bool_field(map, "auto_message_id", true),
        msgid_domain: string_field(map, "msgid_domain"),
    })
}

fn message_to_niao(msg: &MailMessage) -> Value {
    let mut headers = HashMap::new();
    for (k, v) in &msg.headers {
        headers.insert(k.clone(), Value::String(v.clone()).ref_cell());
    }
    let to: Vec<ValueRef> = msg
        .to
        .iter()
        .map(|s| Value::String(s.clone()).ref_cell())
        .collect();
    let cc: Vec<ValueRef> = msg
        .cc
        .iter()
        .map(|s| Value::String(s.clone()).ref_cell())
        .collect();
    let bcc: Vec<ValueRef> = msg
        .bcc
        .iter()
        .map(|s| Value::String(s.clone()).ref_cell())
        .collect();
    let attachments: Vec<ValueRef> = msg
        .attachments
        .iter()
        .map(|a| {
            let mut m = HashMap::new();
            if let Some(f) = &a.filename {
                m.insert("filename".into(), Value::String(f.clone()).ref_cell());
            }
            m.insert(
                "content_type".into(),
                Value::String(a.content_type.clone()).ref_cell(),
            );
            m.insert(
                "disposition".into(),
                Value::String(a.disposition.clone()).ref_cell(),
            );
            m.insert("size".into(), Value::Int(a.size() as i64).ref_cell());
            m.insert("data".into(), Value::ByteArray(a.data.clone()).ref_cell());
            Value::Object(m).ref_cell()
        })
        .collect();
    let inline: Vec<ValueRef> = msg
        .inline
        .iter()
        .map(|p| {
            let mut m = HashMap::new();
            m.insert("cid".into(), Value::String(p.cid.clone()).ref_cell());
            if let Some(f) = &p.filename {
                m.insert("filename".into(), Value::String(f.clone()).ref_cell());
            }
            m.insert(
                "content_type".into(),
                Value::String(p.content_type.clone()).ref_cell(),
            );
            m.insert("size".into(), Value::Int(p.data.len() as i64).ref_cell());
            m.insert("data".into(), Value::ByteArray(p.data.clone()).ref_cell());
            Value::Object(m).ref_cell()
        })
        .collect();
    let parts: Vec<ValueRef> = msg
        .parts
        .iter()
        .map(|p| {
            let mut m = HashMap::new();
            m.insert("index".into(), Value::Int(p.index as i64).ref_cell());
            m.insert(
                "content_type".into(),
                Value::String(p.content_type.clone()).ref_cell(),
            );
            if let Some(d) = &p.disposition {
                m.insert("disposition".into(), Value::String(d.clone()).ref_cell());
            }
            if let Some(f) = &p.filename {
                m.insert("filename".into(), Value::String(f.clone()).ref_cell());
            }
            if let Some(c) = &p.cid {
                m.insert("cid".into(), Value::String(c.clone()).ref_cell());
            }
            m.insert("multipart".into(), Value::Bool(p.is_multipart).ref_cell());
            m.insert("size".into(), Value::Int(p.size as i64).ref_cell());
            m.insert("data".into(), Value::ByteArray(p.data.clone()).ref_cell());
            if let Some(t) = &p.text {
                m.insert("text".into(), Value::String(t.clone()).ref_cell());
            }
            Value::Object(m).ref_cell()
        })
        .collect();

    let mut m = HashMap::new();
    m.insert("kind".into(), Value::String("message".into()).ref_cell());
    m.insert("headers".into(), Value::Object(headers).ref_cell());
    m.insert(
        "from".into(),
        msg.from
            .as_ref()
            .map(|s| Value::String(s.clone()).ref_cell())
            .unwrap_or_else(|| Value::Nil.ref_cell()),
    );
    m.insert("to".into(), Value::Array(to).ref_cell());
    m.insert("cc".into(), Value::Array(cc).ref_cell());
    m.insert("bcc".into(), Value::Array(bcc).ref_cell());
    m.insert(
        "reply_to".into(),
        msg.reply_to
            .as_ref()
            .map(|s| Value::String(s.clone()).ref_cell())
            .unwrap_or_else(|| Value::Nil.ref_cell()),
    );
    m.insert(
        "subject".into(),
        msg.subject
            .as_ref()
            .map(|s| Value::String(s.clone()).ref_cell())
            .unwrap_or_else(|| Value::Nil.ref_cell()),
    );
    m.insert(
        "date".into(),
        msg.date
            .as_ref()
            .map(|s| Value::String(s.clone()).ref_cell())
            .unwrap_or_else(|| Value::Nil.ref_cell()),
    );
    m.insert(
        "message_id".into(),
        msg.message_id
            .as_ref()
            .map(|s| Value::String(s.clone()).ref_cell())
            .unwrap_or_else(|| Value::Nil.ref_cell()),
    );
    m.insert(
        "content_type".into(),
        Value::String(msg.content_type.clone()).ref_cell(),
    );
    m.insert(
        "text".into(),
        msg.text
            .as_ref()
            .map(|s| Value::String(s.clone()).ref_cell())
            .unwrap_or_else(|| Value::Nil.ref_cell()),
    );
    m.insert(
        "html".into(),
        msg.html
            .as_ref()
            .map(|s| Value::String(s.clone()).ref_cell())
            .unwrap_or_else(|| Value::Nil.ref_cell()),
    );
    m.insert("attachments".into(), Value::Array(attachments).ref_cell());
    m.insert("inline".into(), Value::Array(inline).ref_cell());
    m.insert("parts".into(), Value::Array(parts).ref_cell());
    m.insert("multipart".into(), Value::Bool(msg.multipart).ref_cell());
    Value::Object(m)
}

fn niao_to_message(v: &ValueRef, span: Span) -> NiaoResult<MailMessage> {
    let map = match &*v.borrow() {
        Value::Object(m) => m.clone(),
        other => {
            return Err(type_err(
                span,
                format!("message must be an object, got {}", other.type_name()),
            ));
        }
    };
    // Prefer rebuild from known fields / raw rebuild via build spec.
    let mut spec = build_spec_from_map(&map, span)?;
    if spec.from.is_none() {
        // parsed message may store from at top level already handled
        if let Some(f) = string_field(&map, "from") {
            spec.from = Some(f);
        }
    }
    if spec.to.is_empty() {
        if let Ok(t) = recipients_field(&map, "to", span) {
            spec.to = t;
        }
    }
    // Allow emit of partially-filled messages for parse→emit roundtrips without required checks.
    let mut msg = MailMessage::new();
    msg.from = spec.from.or_else(|| string_field(&map, "from"));
    msg.to = if spec.to.is_empty() {
        recipients_field(&map, "to", span).unwrap_or_default()
    } else {
        spec.to
    };
    msg.cc = spec.cc;
    msg.bcc = spec.bcc;
    msg.reply_to = spec.reply_to.or_else(|| string_field(&map, "reply_to"));
    msg.subject = spec.subject.or_else(|| string_field(&map, "subject"));
    msg.text = spec.text;
    msg.html = spec.html;
    msg.date = spec.date.or_else(|| string_field(&map, "date"));
    msg.message_id = spec.message_id.or_else(|| string_field(&map, "message_id"));
    msg.attachments = spec.attachments;
    msg.inline = spec.inline;
    msg.content_type =
        string_field(&map, "content_type").unwrap_or_else(|| "text/plain; charset=utf-8".into());
    msg.multipart = bool_field(&map, "multipart", false)
        || msg.html.is_some()
        || !msg.attachments.is_empty()
        || !msg.inline.is_empty();
    if let Some(h) = map.get("headers") {
        if let Value::Object(headers) = &*h.borrow() {
            for (k, vv) in headers {
                if let Value::String(s) = &*vv.borrow() {
                    msg.headers.insert(k.to_ascii_lowercase(), s.clone());
                }
            }
        }
    }
    Ok(msg)
}

fn parse_opts(map: Option<&HashMap<String, ValueRef>>) -> ParseOptions {
    ParseOptions {
        relaxed: map.map(|m| bool_field(m, "relaxed", false)).unwrap_or(false),
    }
}

fn emit_opts(map: Option<&HashMap<String, ValueRef>>) -> EmitOptions {
    EmitOptions {
        crlf: map.map(|m| bool_field(m, "crlf", true)).unwrap_or(true),
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

// >>> nmail.build({from: "a@b.com", to: "c@d.com", subject: "Hi", text: "Hello"}).subject
// => "Hi"
fn nmail_build(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmail.build", span)?;
    let map = object_arg(args, 0, "nmail.build", span)?;
    match build_spec_from_map(&map, span)?.build() {
        Ok(msg) => Ok(message_to_niao(&msg).ref_cell()),
        Err(e) => Ok(map_mail_err(span, e)),
    }
}

// >>> nmail.valid("From: a@b.com\r\nTo: c@d.com\r\nSubject: x\r\n\r\nHi")
// => true
fn nmail_valid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmail.valid", span)?;
    let s = string_arg(args, 0, "nmail.valid", span)?;
    Ok(Value::Bool(is_valid(&s)).ref_cell())
}

// >>> nmail.parse("From: a@b.com\r\nTo: c@d.com\r\nSubject: Hi\r\n\r\nBody").subject
// => "Hi"
fn nmail_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmail.parse", span)?;
    let s = string_arg(args, 0, "nmail.parse", span)?;
    let opts = parse_opts(optional_object_arg(args, 1).as_ref());
    match parse(&s, &opts) {
        Ok(msg) => Ok(message_to_niao(&msg).ref_cell()),
        Err(e) => Ok(map_mail_err(span, e)),
    }
}

// >>> let b = nmail.emit_bytes(nmail.build({from:"a@b.com",to:"c@d.com",text:"x"})); nmail.parse_bytes(b).text
// => "x"
fn nmail_parse_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmail.parse_bytes", span)?;
    let data = bytes_from_value(&args[0].borrow(), span, "nmail.parse_bytes")?;
    if data.len() > MAX_BYTES {
        return Ok(map_mail_err(span, MailError::TooLarge(MAX_BYTES)));
    }
    let opts = parse_opts(optional_object_arg(args, 1).as_ref());
    match parse_bytes(&data, &opts) {
        Ok(msg) => Ok(message_to_niao(&msg).ref_cell()),
        Err(e) => Ok(map_mail_err(span, e)),
    }
}

// >>> nmail.parse_file("message.eml")
fn nmail_parse_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmail.parse_file", span)?;
    let path = string_arg(args, 0, "nmail.parse_file", span)?;
    let opts = parse_opts(optional_object_arg(args, 1).as_ref());
    match parse_file(&path, &opts) {
        Ok(msg) => Ok(message_to_niao(&msg).ref_cell()),
        Err(e) => Ok(map_mail_err(span, e)),
    }
}

// >>> nmail.emit(nmail.build({from:"a@b.com",to:"b@c.com",subject:"S",text:"T"})).contains("Subject: S")
// => true
fn nmail_emit(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmail.emit", span)?;
    let msg = niao_to_message(&args[0], span)?;
    let opts = emit_opts(optional_object_arg(args, 1).as_ref());
    match emit(&msg, &opts) {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(map_mail_err(span, e)),
    }
}

// >>> nmail.emit_bytes(nmail.build({from:"a@b.com",to:"b@c.com",text:"hi"})).len > 0
// => true
fn nmail_emit_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmail.emit_bytes", span)?;
    let msg = niao_to_message(&args[0], span)?;
    let opts = emit_opts(optional_object_arg(args, 1).as_ref());
    match emit_bytes(&msg, &opts) {
        Ok(b) => Ok(Value::ByteArray(b).ref_cell()),
        Err(e) => Ok(map_mail_err(span, e)),
    }
}

// >>> nmail.emit_file("/tmp/out.eml", nmail.build({from:"a@b.com",to:"b@c.com",text:"hi"}))
// => true
fn nmail_emit_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nmail.emit_file", span)?;
    let path = string_arg(args, 0, "nmail.emit_file", span)?;
    let msg = niao_to_message(&args[1], span)?;
    let opts = emit_opts(optional_object_arg(args, 2).as_ref());
    match emit_file(&path, &msg, &opts) {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(map_mail_err(span, e)),
    }
}

// >>> nmail.get(nmail.build({from:"a@b.com",to:"b@c.com",subject:"S",text:"t"}), "subject")
// => "S"
fn nmail_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "nmail.get", span)?;
    let msg = niao_to_message(&args[0], span)?;
    let name = string_arg(args, 1, "nmail.get", span)?;
    Ok(match msg.get_header(&name) {
        Some(v) => Value::String(v.to_string()).ref_cell(),
        None => Value::Nil.ref_cell(),
    })
}

// >>> nmail.set_header(nmail.build({from:"a@b.com",to:"b@c.com",text:"t"}), "X-Tag", "1").headers["x-tag"]
// => "1"
fn nmail_set_header(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 3, "nmail.set_header", span)?;
    let mut msg = niao_to_message(&args[0], span)?;
    let name = string_arg(args, 1, "nmail.set_header", span)?;
    let value = string_arg(args, 2, "nmail.set_header", span)?;
    if name.is_empty() {
        return Ok(nmail_err(span, "header name must not be empty"));
    }
    msg.set_header(&name, value);
    Ok(message_to_niao(&msg).ref_cell())
}

// >>> nmail.headers(nmail.build({from:"a@b.com",to:"b@c.com",subject:"S",text:"t"})).subject
// => "S"
fn nmail_headers(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmail.headers", span)?;
    let msg = niao_to_message(&args[0], span)?;
    let mut h = HashMap::new();
    for (k, v) in msg.headers {
        h.insert(k, Value::String(v).ref_cell());
    }
    Ok(Value::Object(h).ref_cell())
}

// >>> nmail.subject(nmail.build({from:"a@b.com",to:"b@c.com",subject:"Hi",text:"t"}))
// => "Hi"
fn nmail_subject(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmail.subject", span)?;
    let msg = niao_to_message(&args[0], span)?;
    Ok(msg
        .subject
        .map(|s| Value::String(s).ref_cell())
        .unwrap_or_else(|| Value::Nil.ref_cell()))
}

// >>> nmail.from_addr(nmail.build({from:"a@b.com",to:"b@c.com",text:"t"}))
// => "a@b.com"
fn nmail_from_addr(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmail.from_addr", span)?;
    let msg = niao_to_message(&args[0], span)?;
    Ok(msg
        .from
        .map(|s| Value::String(s).ref_cell())
        .unwrap_or_else(|| Value::Nil.ref_cell()))
}

// >>> len(nmail.to_addrs(nmail.build({from:"a@b.com",to:"b@c.com",text:"t"})))
// => 1
fn nmail_to_addrs(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmail.to_addrs", span)?;
    let msg = niao_to_message(&args[0], span)?;
    let arr: Vec<ValueRef> = msg
        .to
        .into_iter()
        .map(|s| Value::String(s).ref_cell())
        .collect();
    Ok(Value::Array(arr).ref_cell())
}

// >>> len(nmail.cc_addrs(nmail.build({from:"a@b.com",to:"b@c.com",cc:"c@d.com",text:"t"})))
// => 1
fn nmail_cc_addrs(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmail.cc_addrs", span)?;
    let msg = niao_to_message(&args[0], span)?;
    let arr: Vec<ValueRef> = msg
        .cc
        .into_iter()
        .map(|s| Value::String(s).ref_cell())
        .collect();
    Ok(Value::Array(arr).ref_cell())
}

// >>> len(nmail.bcc_addrs(nmail.build({from:"a@b.com",to:"b@c.com",bcc:"c@d.com",text:"t"})))
// => 1
fn nmail_bcc_addrs(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmail.bcc_addrs", span)?;
    let msg = niao_to_message(&args[0], span)?;
    let arr: Vec<ValueRef> = msg
        .bcc
        .into_iter()
        .map(|s| Value::String(s).ref_cell())
        .collect();
    Ok(Value::Array(arr).ref_cell())
}

// >>> nmail.reply_to(nmail.build({from:"a@b.com",to:"b@c.com",reply_to:"r@d.com",text:"t"}))
// => "r@d.com"
fn nmail_reply_to(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmail.reply_to", span)?;
    let msg = niao_to_message(&args[0], span)?;
    Ok(msg
        .reply_to
        .map(|s| Value::String(s).ref_cell())
        .unwrap_or_else(|| Value::Nil.ref_cell()))
}

// >>> nmail.date(nmail.build({from:"a@b.com",to:"b@c.com",date:"Thu, 01 Jan 1970 00:00:00 +0000",text:"t"})).contains("1970")
// => true
fn nmail_date(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmail.date", span)?;
    let msg = niao_to_message(&args[0], span)?;
    Ok(msg
        .date
        .map(|s| Value::String(s).ref_cell())
        .unwrap_or_else(|| Value::Nil.ref_cell()))
}

// >>> nmail.message_id(nmail.build({from:"a@b.com",to:"b@c.com",message_id:"<x@y>",text:"t"}))
// => "<x@y>"
fn nmail_message_id(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmail.message_id", span)?;
    let msg = niao_to_message(&args[0], span)?;
    Ok(msg
        .message_id
        .map(|s| Value::String(s).ref_cell())
        .unwrap_or_else(|| Value::Nil.ref_cell()))
}

// >>> nmail.text(nmail.build({from:"a@b.com",to:"b@c.com",text:"hello"}))
// => "hello"
fn nmail_text(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmail.text", span)?;
    let msg = niao_to_message(&args[0], span)?;
    Ok(msg
        .text
        .map(|s| Value::String(s).ref_cell())
        .unwrap_or_else(|| Value::Nil.ref_cell()))
}

// >>> nmail.html(nmail.build({from:"a@b.com",to:"b@c.com",html:"<p>x</p>"}))
// => "<p>x</p>"
fn nmail_html(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmail.html", span)?;
    let msg = niao_to_message(&args[0], span)?;
    Ok(msg
        .html
        .map(|s| Value::String(s).ref_cell())
        .unwrap_or_else(|| Value::Nil.ref_cell()))
}

// >>> len(nmail.attachments(nmail.attach(nmail.build({from:"a@b.com",to:"b@c.com",text:"t"}), {filename:"a.txt",data:"x"})))
// => 1
fn nmail_attachments(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmail.attachments", span)?;
    let msg = message_to_niao(&niao_to_message(&args[0], span)?);
    match msg {
        Value::Object(m) => Ok(m
            .get("attachments")
            .cloned()
            .unwrap_or_else(|| Value::Array(vec![]).ref_cell())),
        _ => Ok(Value::Array(vec![]).ref_cell()),
    }
}

// >>> len(nmail.inline_parts(nmail.add_inline(nmail.build({from:"a@b.com",to:"b@c.com",text:"t"}), {cid:"i1",data:"x"})))
// => 1
fn nmail_inline_parts(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmail.inline_parts", span)?;
    let msg = message_to_niao(&niao_to_message(&args[0], span)?);
    match msg {
        Value::Object(m) => Ok(m
            .get("inline")
            .cloned()
            .unwrap_or_else(|| Value::Array(vec![]).ref_cell())),
        _ => Ok(Value::Array(vec![]).ref_cell()),
    }
}

// >>> len(nmail.parts(nmail.parse("From: a@b.com\r\nTo: b@c.com\r\nSubject: x\r\n\r\nHi")))
// => 1
fn nmail_parts(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmail.parts", span)?;
    let msg = message_to_niao(&niao_to_message(&args[0], span)?);
    match msg {
        Value::Object(m) => Ok(m
            .get("parts")
            .cloned()
            .unwrap_or_else(|| Value::Array(vec![]).ref_cell())),
        _ => Ok(Value::Array(vec![]).ref_cell()),
    }
}

// >>> len(nmail.walk(nmail.parse("From: a@b.com\r\nTo: b@c.com\r\nSubject: x\r\n\r\nHi")))
// => 1
fn nmail_walk(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    nmail_parts(args, span)
}

// >>> nmail.is_multipart(nmail.build({from:"a@b.com",to:"b@c.com",text:"a",html:"<p>a</p>"}))
// => true
fn nmail_is_multipart(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmail.is_multipart", span)?;
    let msg = niao_to_message(&args[0], span)?;
    Ok(Value::Bool(msg.multipart).ref_cell())
}

// >>> nmail.content_type(nmail.build({from:"a@b.com",to:"b@c.com",text:"t"})).contains("text/plain")
// => true
fn nmail_content_type(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmail.content_type", span)?;
    let msg = niao_to_message(&args[0], span)?;
    Ok(Value::String(msg.content_type).ref_cell())
}

// >>> nmail.payload({text: "hi", data: "hi"})
// => "hi"
fn nmail_payload(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmail.payload", span)?;
    let part = object_arg(args, 0, "nmail.payload", span)?;
    let as_text = optional_object_arg(args, 1)
        .map(|m| bool_field(&m, "decode", true) && bool_field(&m, "text", true))
        .unwrap_or(true);
    if as_text {
        if let Some(t) = string_field(&part, "text") {
            return Ok(Value::String(t).ref_cell());
        }
    }
    if let Some(v) = part.get("data") {
        return Ok(v.clone());
    }
    Ok(Value::Nil.ref_cell())
}

// >>> nmail.attach(nmail.build({from:"a@b.com",to:"b@c.com",text:"t"}), {filename:"a.txt",data:"x"}).attachments[0].filename
// => "a.txt"
fn nmail_attach(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "nmail.attach", span)?;
    let msg = niao_to_message(&args[0], span)?;
    let opts = object_arg(args, 1, "nmail.attach", span)?;
    let filename = string_field(&opts, "filename").or_else(|| string_field(&opts, "name"));
    let content_type =
        string_field(&opts, "content_type").unwrap_or_else(|| "application/octet-stream".into());
    let disposition = string_field(&opts, "disposition").unwrap_or_else(|| "attachment".into());
    let data = match opts.get("data").or_else(|| opts.get("content")) {
        Some(dv) => bytes_from_value(&dv.borrow(), span, "attach.data")?,
        None => return Ok(nmail_err(span, "attach requires data/content")),
    };
    Ok(message_to_niao(&attach(msg, filename, content_type, disposition, data)).ref_cell())
}

// >>> nmail.add_inline(nmail.build({from:"a@b.com",to:"b@c.com",text:"t"}), {cid:"logo",data:"x"}).inline[0].cid
// => "logo"
fn nmail_add_inline(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "nmail.add_inline", span)?;
    let msg = niao_to_message(&args[0], span)?;
    let opts = object_arg(args, 1, "nmail.add_inline", span)?;
    let cid = match string_field(&opts, "cid").or_else(|| string_field(&opts, "content_id")) {
        Some(c) => c,
        None => return Ok(nmail_err(span, "add_inline requires cid")),
    };
    let filename = string_field(&opts, "filename");
    let content_type =
        string_field(&opts, "content_type").unwrap_or_else(|| "application/octet-stream".into());
    let data = match opts.get("data").or_else(|| opts.get("content")) {
        Some(dv) => bytes_from_value(&dv.borrow(), span, "inline.data")?,
        None => return Ok(nmail_err(span, "add_inline requires data/content")),
    };
    Ok(message_to_niao(&add_inline(msg, cid, filename, content_type, data)).ref_cell())
}

// >>> nmail.format_addr("Ada", "ada@example.com")
// => "Ada <ada@example.com>"
fn nmail_format_addr(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmail.format_addr", span)?;
    if args.len() == 1 {
        match &*args[0].borrow() {
            Value::Object(m) => {
                let email = string_field(m, "email")
                    .ok_or_else(|| type_err(span, "format_addr object needs email"))?;
                let name = string_field(m, "name");
                match format_addr(name.as_deref(), &email) {
                    Ok(s) => Ok(Value::String(s).ref_cell()),
                    Err(e) => Ok(map_mail_err(span, e)),
                }
            }
            Value::String(email) => match format_addr(None, email) {
                Ok(s) => Ok(Value::String(s).ref_cell()),
                Err(e) => Ok(map_mail_err(span, e)),
            },
            other => Err(type_err(
                span,
                format!("format_addr expects object or string, got {}", other.type_name()),
            )),
        }
    } else {
        let name = string_arg(args, 0, "nmail.format_addr", span)?;
        let email = string_arg(args, 1, "nmail.format_addr", span)?;
        match format_addr(Some(&name), &email) {
            Ok(s) => Ok(Value::String(s).ref_cell()),
            Err(e) => Ok(map_mail_err(span, e)),
        }
    }
}

// >>> nmail.parse_addr("Ada <ada@example.com>").email
// => "ada@example.com"
fn nmail_parse_addr(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmail.parse_addr", span)?;
    let s = string_arg(args, 0, "nmail.parse_addr", span)?;
    match parse_addr(&s) {
        Ok(a) => {
            let mut m = HashMap::new();
            m.insert(
                "name".into(),
                a.name
                    .map(|n| Value::String(n).ref_cell())
                    .unwrap_or_else(|| Value::Nil.ref_cell()),
            );
            m.insert("email".into(), Value::String(a.email).ref_cell());
            Ok(Value::Object(m).ref_cell())
        }
        Err(e) => Ok(map_mail_err(span, e)),
    }
}

// >>> len(nmail.parse_addrs("a@b.com, Bob <b@c.com>"))
// => 2
fn nmail_parse_addrs(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmail.parse_addrs", span)?;
    let s = string_arg(args, 0, "nmail.parse_addrs", span)?;
    match parse_addrs(&s) {
        Ok(list) => {
            let arr: Vec<ValueRef> = list
                .into_iter()
                .map(|a| {
                    let mut m = HashMap::new();
                    m.insert(
                        "name".into(),
                        a.name
                            .map(|n| Value::String(n).ref_cell())
                            .unwrap_or_else(|| Value::Nil.ref_cell()),
                    );
                    m.insert("email".into(), Value::String(a.email).ref_cell());
                    Value::Object(m).ref_cell()
                })
                .collect();
            Ok(Value::Array(arr).ref_cell())
        }
        Err(e) => Ok(map_mail_err(span, e)),
    }
}

// >>> nmail.make_msgid("example.com").contains("@example.com")
// => true
fn nmail_make_msgid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nmail.make_msgid", span)?;
    let domain = if args.is_empty() {
        None
    } else {
        match &*args[0].borrow() {
            Value::Nil => None,
            Value::String(s) => Some(s.clone()),
            other => {
                return Err(type_err(
                    span,
                    format!("make_msgid expects string or nil, got {}", other.type_name()),
                ));
            }
        }
    };
    Ok(Value::String(make_msgid(domain.as_deref())).ref_cell())
}

// >>> nmail.format_date(0).contains("1970")
// => true
fn nmail_format_date(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nmail.format_date", span)?;
    let secs = if args.is_empty() {
        None
    } else {
        match &*args[0].borrow() {
            Value::Nil => None,
            Value::Int(n) => Some(*n),
            other => {
                return Err(type_err(
                    span,
                    format!("format_date expects int or nil, got {}", other.type_name()),
                ));
            }
        }
    };
    Ok(Value::String(format_date(secs)).ref_cell())
}

// >>> nmail.encode_header("café").contains("=?UTF-8?B?")
// => true
// >>> nmail.decode_header(nmail.encode_header("café"))
// => "café"
fn nmail_encode_header(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmail.encode_header", span)?;
    let s = string_arg(args, 0, "nmail.encode_header", span)?;
    Ok(Value::String(encode_header(&s)).ref_cell())
}

// >>> nmail.decode_header("=?UTF-8?Q?caf=C3=A9?=")
// => "café"
fn nmail_decode_header(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nmail.decode_header", span)?;
    let s = string_arg(args, 0, "nmail.decode_header", span)?;
    match decode_header(&s) {
        Ok(d) => Ok(Value::String(d).ref_cell()),
        Err(e) => Ok(map_mail_err(span, e)),
    }
}

macro_rules! nmail_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nmail_fns![
    ("nmail_build", "build", nmail_build),
    ("nmail_parse", "parse", nmail_parse),
    ("nmail_parse_bytes", "parse_bytes", nmail_parse_bytes),
    ("nmail_parse_file", "parse_file", nmail_parse_file),
    ("nmail_valid", "valid", nmail_valid),
    ("nmail_emit", "emit", nmail_emit),
    ("nmail_emit_bytes", "emit_bytes", nmail_emit_bytes),
    ("nmail_emit_file", "emit_file", nmail_emit_file),
    ("nmail_get", "get", nmail_get),
    ("nmail_set_header", "set_header", nmail_set_header),
    ("nmail_headers", "headers", nmail_headers),
    ("nmail_subject", "subject", nmail_subject),
    ("nmail_from_addr", "from_addr", nmail_from_addr),
    ("nmail_to_addrs", "to_addrs", nmail_to_addrs),
    ("nmail_cc_addrs", "cc_addrs", nmail_cc_addrs),
    ("nmail_bcc_addrs", "bcc_addrs", nmail_bcc_addrs),
    ("nmail_reply_to", "reply_to", nmail_reply_to),
    ("nmail_date", "date", nmail_date),
    ("nmail_message_id", "message_id", nmail_message_id),
    ("nmail_text", "text", nmail_text),
    ("nmail_html", "html", nmail_html),
    ("nmail_attachments", "attachments", nmail_attachments),
    ("nmail_inline_parts", "inline_parts", nmail_inline_parts),
    ("nmail_parts", "parts", nmail_parts),
    ("nmail_walk", "walk", nmail_walk),
    ("nmail_is_multipart", "is_multipart", nmail_is_multipart),
    ("nmail_content_type", "content_type", nmail_content_type),
    ("nmail_payload", "payload", nmail_payload),
    ("nmail_attach", "attach", nmail_attach),
    ("nmail_add_inline", "add_inline", nmail_add_inline),
    ("nmail_format_addr", "format_addr", nmail_format_addr),
    ("nmail_parse_addr", "parse_addr", nmail_parse_addr),
    ("nmail_parse_addrs", "parse_addrs", nmail_parse_addrs),
    ("nmail_make_msgid", "make_msgid", nmail_make_msgid),
    ("nmail_format_date", "format_date", nmail_format_date),
    ("nmail_encode_header", "encode_header", nmail_encode_header),
    ("nmail_decode_header", "decode_header", nmail_decode_header),
];

fn all_builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs()
        .into_iter()
        .map(|(flat, _, f)| (flat, f))
        .collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nmail";
pub const MODULE_PATHS: &[&str] = &["nmail", "std/nmail"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}
