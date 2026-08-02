//! Native nmqtt standard library — MQTT 3.1.1/5 client (QoS 0–2, TLS, reconnect, wills).
//!
//! Import with `import "nmqtt"` (or `import "std/nmqtt"`).
//!
//! Backed by the `niao_mqtt` crate (sync client + packet codec over rustls).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_mqtt::{
    decode_packet, encode_connect, encode_publish, packet_type_name, topic_matches, Client,
    ClientConfig, ConnectOptions, Message, MqttError, Packet, PublishPacket, Will, PROTO_MQTT311,
    PROTO_MQTT5,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

const E_ARITY: u32 = 4130;
const E_ERROR: u32 = 4131;
const E_TYPE: u32 = 4132;
const E_HANDLE: u32 = 4133;
const E_PROTOCOL: u32 = 4134;

thread_local! {
    static CLIENTS: RefCell<HashMap<i64, Client>> = RefCell::new(HashMap::new());
    static NEXT_HANDLE: RefCell<i64> = const { RefCell::new(1) };
}

fn new_handle() -> i64 {
    NEXT_HANDLE.with(|h| {
        let mut n = h.borrow_mut();
        let id = *n;
        *n = n.saturating_add(1);
        id
    })
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E_TYPE, msg.into())
}

fn mqtt_err_value(span: Span, e: MqttError) -> ValueRef {
    let code = match &e {
        MqttError::Protocol(_) | MqttError::Connack(_, _) => E_PROTOCOL,
        _ => E_ERROR,
    };
    error_value(code, "nmqtt_error", e.to_string(), span)
}

fn ok_bool(b: bool) -> ValueRef {
    Value::Bool(b).ref_cell()
}

fn ok_int(n: i64) -> ValueRef {
    Value::Int(n).ref_cell()
}

fn ok_str(s: impl Into<String>) -> ValueRef {
    Value::String(s.into()).ref_cell()
}

fn handle_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) if *n > 0 => Ok(*n),
        Value::Int(_) => Err(RuntimeError::at(
            span,
            E_HANDLE,
            format!("{name}() invalid handle"),
        )),
        other => Err(type_err(
            span,
            format!("{name}() expects handle int, got {}", other.type_name()),
        )),
    }
}

fn string_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(type_err(
            span,
            format!("{name}() expects string, got {}", other.type_name()),
        )),
    }
}

fn payload_bytes(v: &Value, name: &str, span: Span) -> NiaoResult<Vec<u8>> {
    match v {
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        Value::ByteArray(b) => Ok(b.clone()),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match &*item.borrow() {
                    Value::Int(n) if (0..=255).contains(n) => out.push(*n as u8),
                    Value::Int(_) => {
                        return Err(type_err(span, format!("{name}() byte values must be 0..=255")));
                    }
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() payload array items must be ints, got {}",
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() payload must be string, bytearray, or int[], got {}",
                other.type_name()
            ),
        )),
    }
}

fn optional_object(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> NiaoResult<Option<HashMap<String, ValueRef>>> {
    if args.len() <= idx {
        return Ok(None);
    }
    match &*args[idx].borrow() {
        Value::Nil => Ok(None),
        Value::Object(m) => Ok(Some(m.clone())),
        other => Err(type_err(
            span,
            format!("{name}() opts must be object, got {}", other.type_name()),
        )),
    }
}

fn obj_string(map: &HashMap<String, ValueRef>, key: &str, span: Span) -> NiaoResult<Option<String>> {
    match map.get(key) {
        None => Ok(None),
        Some(v) => match &*v.borrow() {
            Value::Nil => Ok(None),
            Value::String(s) => Ok(Some(s.clone())),
            other => Err(type_err(
                span,
                format!("config.{key} must be string, got {}", other.type_name()),
            )),
        },
    }
}

fn obj_required_string(
    map: &HashMap<String, ValueRef>,
    key: &str,
    span: Span,
) -> NiaoResult<String> {
    match obj_string(map, key, span)? {
        Some(s) if !s.is_empty() => Ok(s),
        Some(_) => Err(type_err(span, format!("config.{key} must not be empty"))),
        None => Err(type_err(span, format!("config: missing field '{key}'"))),
    }
}

fn obj_int(map: &HashMap<String, ValueRef>, key: &str, span: Span) -> NiaoResult<Option<i64>> {
    match map.get(key) {
        None => Ok(None),
        Some(v) => match &*v.borrow() {
            Value::Nil => Ok(None),
            Value::Int(n) => Ok(Some(*n)),
            other => Err(type_err(
                span,
                format!("config.{key} must be int, got {}", other.type_name()),
            )),
        },
    }
}

fn obj_bool(map: &HashMap<String, ValueRef>, key: &str, default: bool) -> bool {
    match map.get(key) {
        Some(v) => matches!(&*v.borrow(), Value::Bool(b) if *b),
        None => default,
    }
}

fn parse_protocol(map: &HashMap<String, ValueRef>, span: Span) -> NiaoResult<u8> {
    match map.get("protocol") {
        None => Ok(PROTO_MQTT311),
        Some(v) => match &*v.borrow() {
            Value::Nil => Ok(PROTO_MQTT311),
            Value::Int(4) => Ok(PROTO_MQTT311),
            Value::Int(5) => Ok(PROTO_MQTT5),
            Value::String(s) if s == "3.1.1" || s == "MQTT3.1.1" || s == "mqttv311" => {
                Ok(PROTO_MQTT311)
            }
            Value::String(s) if s == "5" || s == "5.0" || s == "MQTT5" || s == "mqttv5" => {
                Ok(PROTO_MQTT5)
            }
            Value::Int(n) => Err(type_err(
                span,
                format!("config.protocol int must be 4 or 5, got {n}"),
            )),
            other => Err(type_err(
                span,
                format!(
                    "config.protocol must be \"3.1.1\", \"5\", 4, or 5, got {}",
                    other.type_name()
                ),
            )),
        },
    }
}

fn parse_will(map: &HashMap<String, ValueRef>, span: Span) -> NiaoResult<Option<Will>> {
    match map.get("will") {
        None => Ok(None),
        Some(v) => match &*v.borrow() {
            Value::Nil => Ok(None),
            Value::Object(w) => {
                let topic = obj_required_string(w, "topic", span)?;
                let payload = match w.get("payload") {
                    None => Vec::new(),
                    Some(p) => payload_bytes(&p.borrow(), "will.payload", span)?,
                };
                let qos = obj_int(w, "qos", span)?.unwrap_or(0);
                if !(0..=2).contains(&qos) {
                    return Err(type_err(span, "will.qos must be 0..=2"));
                }
                let retain = obj_bool(w, "retain", false);
                Ok(Some(Will {
                    topic,
                    payload,
                    qos: qos as u8,
                    retain,
                }))
            }
            other => Err(type_err(
                span,
                format!("config.will must be object, got {}", other.type_name()),
            )),
        },
    }
}

fn parse_reconnect(map: &HashMap<String, ValueRef>, span: Span) -> NiaoResult<(bool, u64, u64)> {
    match map.get("reconnect") {
        None => Ok((false, 1000, 30_000)),
        Some(v) => match &*v.borrow() {
            Value::Nil => Ok((false, 1000, 30_000)),
            Value::Bool(b) => Ok((*b, 1000, 30_000)),
            Value::Object(r) => {
                let enabled = obj_bool(r, "enabled", true);
                let delay = obj_int(r, "delay_ms", span)?.unwrap_or(1000);
                let max_delay = obj_int(r, "max_delay_ms", span)?.unwrap_or(30_000);
                if delay < 0 || max_delay < 0 {
                    return Err(type_err(span, "reconnect delays must be non-negative"));
                }
                Ok((enabled, delay as u64, max_delay as u64))
            }
            other => Err(type_err(
                span,
                format!(
                    "config.reconnect must be bool or object, got {}",
                    other.type_name()
                ),
            )),
        },
    }
}

fn parse_connect_config(
    map: &HashMap<String, ValueRef>,
    span: Span,
) -> NiaoResult<(ClientConfig, ConnectOptions)> {
    let host = obj_required_string(map, "host", span)?;
    let tls = obj_bool(map, "tls", false);
    let port = match obj_int(map, "port", span)? {
        Some(p) if (0..=65535).contains(&p) => p as u16,
        Some(_) => return Err(type_err(span, "config.port must be 0..=65535")),
        None => {
            if tls {
                8883
            } else {
                1883
            }
        }
    };
    let keepalive = match obj_int(map, "keepalive", span)? {
        Some(k) if (0..=65535).contains(&k) => k as u16,
        Some(_) => return Err(type_err(span, "config.keepalive must be 0..=65535")),
        None => 60,
    };
    let clean = if map.contains_key("clean_start") {
        obj_bool(map, "clean_start", true)
    } else {
        obj_bool(map, "clean_session", true)
    };
    let client_id = obj_string(map, "client_id", span)?.unwrap_or_default();
    let username = obj_string(map, "username", span)?;
    let password = match map.get("password") {
        None => None,
        Some(v) => match &*v.borrow() {
            Value::Nil => None,
            other => Some(payload_bytes(other, "password", span)?),
        },
    };
    let will = parse_will(map, span)?;
    let protocol_level = parse_protocol(map, span)?;
    let (reconnect, reconnect_delay_ms, reconnect_max_delay_ms) = parse_reconnect(map, span)?;

    let connect = ConnectOptions {
        client_id,
        protocol_level,
        clean_session: clean,
        keepalive,
        username,
        password,
        will,
    };
    let cfg = ClientConfig {
        host,
        port,
        tls,
        connect: connect.clone(),
        reconnect,
        reconnect_delay_ms,
        reconnect_max_delay_ms,
    };
    Ok((cfg, connect))
}

fn message_to_value(msg: Message) -> ValueRef {
    let mut m = HashMap::new();
    m.insert("topic".into(), ok_str(msg.topic));
    // Prefer string when valid UTF-8, else byte array
    let payload = match String::from_utf8(msg.payload.clone()) {
        Ok(s) => ok_str(s),
        Err(_) => Value::ByteArray(msg.payload).ref_cell(),
    };
    m.insert("payload".into(), payload);
    m.insert("qos".into(), ok_int(msg.qos as i64));
    m.insert("retain".into(), ok_bool(msg.retain));
    m.insert("dup".into(), ok_bool(msg.dup));
    Value::Object(m).ref_cell()
}

fn with_client<F>(id: i64, span: Span, f: F) -> NiaoResult<ValueRef>
where
    F: FnOnce(&mut Client) -> NiaoResult<ValueRef>,
{
    CLIENTS.with(|c| {
        let mut map = c.borrow_mut();
        match map.get_mut(&id) {
            Some(client) => f(client),
            None => Err(RuntimeError::at(
                span,
                E_HANDLE,
                format!("nmqtt: invalid or closed handle {id}"),
            )),
        }
    })
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

// >>> import "nmqtt"
// >>> nmqtt.topic_matches("a/+/c", "a/b/c")
// => true
fn nmqtt_topic_matches(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmqtt_topic_matches", span)?;
    let filter = string_arg(args, 0, "nmqtt_topic_matches", span)?;
    let topic = string_arg(args, 1, "nmqtt_topic_matches", span)?;
    Ok(ok_bool(topic_matches(&filter, &topic)))
}

// >>> let pkt = nmqtt.encode_connect({host: "localhost", client_id: "demo"})
// >>> nmqtt.packet_type(pkt)
// => "CONNECT"
fn nmqtt_encode_connect(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmqtt_encode_connect", span)?;
    let map = match &*args[0].borrow() {
        Value::Object(m) => m.clone(),
        other => {
            return Err(type_err(
                span,
                format!(
                    "nmqtt_encode_connect() expects config object, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    // host required for connect(); for encode allow host-less by injecting placeholder
    let mut map = map;
    if !map.contains_key("host") {
        map.insert(
            "host".into(),
            Value::String("127.0.0.1".into()).ref_cell(),
        );
    }
    let (_, opts) = parse_connect_config(&map, span)?;
    match encode_connect(&opts) {
        Ok(bytes) => Ok(Value::ByteArray(bytes).ref_cell()),
        Err(e) => Ok(mqtt_err_value(span, e)),
    }
}

// >>> let p = nmqtt.encode_publish("t/a", "hi", {qos: 0})
// >>> nmqtt.packet_type(p)
// => "PUBLISH"
fn nmqtt_encode_publish(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nmqtt_encode_publish", span)?;
    let topic = string_arg(args, 0, "nmqtt_encode_publish", span)?;
    let payload = payload_bytes(&args[1].borrow(), "nmqtt_encode_publish", span)?;
    let opts = optional_object(args, 2, "nmqtt_encode_publish", span)?;
    let mut qos = 0i64;
    let mut retain = false;
    let mut packet_id = None;
    if let Some(o) = opts {
        qos = obj_int(&o, "qos", span)?.unwrap_or(0);
        retain = obj_bool(&o, "retain", false);
        packet_id = obj_int(&o, "packet_id", span)?;
    }
    if !(0..=2).contains(&qos) {
        return Err(type_err(span, "qos must be 0..=2"));
    }
    if qos > 0 && packet_id.is_none() {
        packet_id = Some(1);
    }
    let pubpkt = PublishPacket {
        topic,
        payload,
        qos: qos as u8,
        retain,
        dup: false,
        packet_id: packet_id.map(|n| n as u16),
    };
    match encode_publish(&pubpkt) {
        Ok(bytes) => Ok(Value::ByteArray(bytes).ref_cell()),
        Err(e) => Ok(mqtt_err_value(span, e)),
    }
}

// >>> let p = nmqtt.encode_publish("x", "")
// >>> let d = nmqtt.decode_packet(p)
// >>> d.type
// => "PUBLISH"
fn nmqtt_decode_packet(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmqtt_decode_packet", span)?;
    let bytes = payload_bytes(&args[0].borrow(), "nmqtt_decode_packet", span)?;
    match decode_packet(&bytes) {
        Ok((pkt, _)) => Ok(packet_to_value(pkt)),
        Err(e) => Ok(mqtt_err_value(span, e)),
    }
}

fn packet_to_value(pkt: Packet) -> ValueRef {
    let mut m = HashMap::new();
    match pkt {
        Packet::Connect(c) => {
            m.insert("type".into(), ok_str("CONNECT"));
            m.insert("client_id".into(), ok_str(c.client_id));
            m.insert("protocol_level".into(), ok_int(c.protocol_level as i64));
            m.insert("clean_session".into(), ok_bool(c.clean_session));
            m.insert("keepalive".into(), ok_int(c.keepalive as i64));
            if let Some(u) = c.username {
                m.insert("username".into(), ok_str(u));
            }
            if let Some(w) = c.will {
                let mut wm = HashMap::new();
                wm.insert("topic".into(), ok_str(w.topic));
                wm.insert("qos".into(), ok_int(w.qos as i64));
                wm.insert("retain".into(), ok_bool(w.retain));
                m.insert("will".into(), Value::Object(wm).ref_cell());
            }
        }
        Packet::Connack {
            session_present,
            return_code,
        } => {
            m.insert("type".into(), ok_str("CONNACK"));
            m.insert("session_present".into(), ok_bool(session_present));
            m.insert("return_code".into(), ok_int(return_code as i64));
        }
        Packet::Publish(p) => {
            m.insert("type".into(), ok_str("PUBLISH"));
            m.insert("topic".into(), ok_str(p.topic));
            m.insert(
                "payload".into(),
                match String::from_utf8(p.payload.clone()) {
                    Ok(s) => ok_str(s),
                    Err(_) => Value::ByteArray(p.payload).ref_cell(),
                },
            );
            m.insert("qos".into(), ok_int(p.qos as i64));
            m.insert("retain".into(), ok_bool(p.retain));
            m.insert("dup".into(), ok_bool(p.dup));
            if let Some(id) = p.packet_id {
                m.insert("packet_id".into(), ok_int(id as i64));
            }
        }
        Packet::Pingreq => {
            m.insert("type".into(), ok_str("PINGREQ"));
        }
        Packet::Pingresp => {
            m.insert("type".into(), ok_str("PINGRESP"));
        }
        Packet::Disconnect => {
            m.insert("type".into(), ok_str("DISCONNECT"));
        }
        Packet::Puback(id) => {
            m.insert("type".into(), ok_str("PUBACK"));
            m.insert("packet_id".into(), ok_int(id as i64));
        }
        Packet::Pubrec(id) => {
            m.insert("type".into(), ok_str("PUBREC"));
            m.insert("packet_id".into(), ok_int(id as i64));
        }
        Packet::Pubrel(id) => {
            m.insert("type".into(), ok_str("PUBREL"));
            m.insert("packet_id".into(), ok_int(id as i64));
        }
        Packet::Pubcomp(id) => {
            m.insert("type".into(), ok_str("PUBCOMP"));
            m.insert("packet_id".into(), ok_int(id as i64));
        }
        Packet::Subscribe { packet_id, filters } => {
            m.insert("type".into(), ok_str("SUBSCRIBE"));
            m.insert("packet_id".into(), ok_int(packet_id as i64));
            let arr: Vec<ValueRef> = filters
                .into_iter()
                .map(|(t, q)| {
                    let mut fm = HashMap::new();
                    fm.insert("topic".into(), ok_str(t));
                    fm.insert("qos".into(), ok_int(q as i64));
                    Value::Object(fm).ref_cell()
                })
                .collect();
            m.insert("filters".into(), Value::Array(arr).ref_cell());
        }
        Packet::Suback { packet_id, codes } => {
            m.insert("type".into(), ok_str("SUBACK"));
            m.insert("packet_id".into(), ok_int(packet_id as i64));
            let arr: Vec<ValueRef> = codes.into_iter().map(|c| ok_int(c as i64)).collect();
            m.insert("codes".into(), Value::Array(arr).ref_cell());
        }
        Packet::Unsubscribe { packet_id, filters } => {
            m.insert("type".into(), ok_str("UNSUBSCRIBE"));
            m.insert("packet_id".into(), ok_int(packet_id as i64));
            let arr: Vec<ValueRef> = filters.into_iter().map(ok_str).collect();
            m.insert("filters".into(), Value::Array(arr).ref_cell());
        }
        Packet::Unsuback(id) => {
            m.insert("type".into(), ok_str("UNSUBACK"));
            m.insert("packet_id".into(), ok_int(id as i64));
        }
    }
    Value::Object(m).ref_cell()
}

// >>> nmqtt.packet_type(nmqtt.encode_publish("t", "x"))
// => "PUBLISH"
fn nmqtt_packet_type(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmqtt_packet_type", span)?;
    let bytes = payload_bytes(&args[0].borrow(), "nmqtt_packet_type", span)?;
    match packet_type_name(&bytes) {
        Ok(name) => Ok(ok_str(name)),
        Err(e) => Ok(mqtt_err_value(span, e)),
    }
}

// >>> // nmqtt.connect({host: "broker.hivemq.com", client_id: "demo"})
fn nmqtt_connect(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmqtt_connect", span)?;
    let map = match &*args[0].borrow() {
        Value::Object(m) => m.clone(),
        other => {
            return Err(type_err(
                span,
                format!(
                    "nmqtt_connect() expects config object, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    let (cfg, _) = parse_connect_config(&map, span)?;
    let mut client = match Client::new(cfg) {
        Ok(c) => c,
        Err(e) => return Ok(mqtt_err_value(span, e)),
    };
    match client.connect() {
        Ok(()) => {
            let id = new_handle();
            CLIENTS.with(|c| c.borrow_mut().insert(id, client));
            Ok(ok_int(id))
        }
        Err(e) => Ok(mqtt_err_value(span, e)),
    }
}

// >>> // nmqtt.publish(id, "telemetry/t", "21.5", {qos: 1})
fn nmqtt_publish(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nmqtt_publish", span)?;
    let id = handle_arg(args, 0, "nmqtt_publish", span)?;
    let topic = string_arg(args, 1, "nmqtt_publish", span)?;
    let payload = payload_bytes(&args[2].borrow(), "nmqtt_publish", span)?;
    let opts = optional_object(args, 3, "nmqtt_publish", span)?;
    let mut qos = 0i64;
    let mut retain = false;
    if let Some(o) = opts {
        qos = obj_int(&o, "qos", span)?.unwrap_or(0);
        retain = obj_bool(&o, "retain", false);
    }
    if !(0..=2).contains(&qos) {
        return Err(type_err(span, "qos must be 0..=2"));
    }
    with_client(id, span, |c| match c.publish(&topic, &payload, qos as u8, retain) {
        Ok(()) => Ok(ok_bool(true)),
        Err(e) => Ok(mqtt_err_value(span, e)),
    })
}

fn collect_topics(v: &Value, name: &str, span: Span) -> NiaoResult<Vec<String>> {
    match v {
        Value::String(s) if !s.is_empty() => Ok(vec![s.clone()]),
        Value::String(_) => Err(type_err(span, format!("{name}() topic must not be empty"))),
        Value::Array(items) => {
            if items.is_empty() {
                return Err(type_err(span, format!("{name}() topic list must not be empty")));
            }
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match &*item.borrow() {
                    Value::String(s) if !s.is_empty() => out.push(s.clone()),
                    Value::String(_) => {
                        return Err(type_err(
                            span,
                            format!("{name}() topic strings must not be empty"),
                        ));
                    }
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() topics must be strings, got {}",
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() topic must be string or string[], got {}",
                other.type_name()
            ),
        )),
    }
}

// >>> // nmqtt.subscribe(id, "commands/#", 1)
fn nmqtt_subscribe(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nmqtt_subscribe", span)?;
    let id = handle_arg(args, 0, "nmqtt_subscribe", span)?;
    let topics = collect_topics(&args[1].borrow(), "nmqtt_subscribe", span)?;
    let qos = if args.len() >= 3 {
        match &*args[2].borrow() {
            Value::Int(n) if (0..=2).contains(n) => *n as u8,
            Value::Int(_) => return Err(type_err(span, "qos must be 0..=2")),
            other => {
                return Err(type_err(
                    span,
                    format!("qos must be int, got {}", other.type_name()),
                ));
            }
        }
    } else {
        0
    };
    let filters: Vec<(String, u8)> = topics.into_iter().map(|t| (t, qos)).collect();
    with_client(id, span, |c| match c.subscribe_many(&filters) {
        Ok(()) => Ok(ok_bool(true)),
        Err(e) => Ok(mqtt_err_value(span, e)),
    })
}

// >>> // nmqtt.unsubscribe(id, "commands/#")
fn nmqtt_unsubscribe(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmqtt_unsubscribe", span)?;
    let id = handle_arg(args, 0, "nmqtt_unsubscribe", span)?;
    let topics = collect_topics(&args[1].borrow(), "nmqtt_unsubscribe", span)?;
    with_client(id, span, |c| match c.unsubscribe_many(&topics) {
        Ok(()) => Ok(ok_bool(true)),
        Err(e) => Ok(mqtt_err_value(span, e)),
    })
}

// >>> // let msg = nmqtt.recv(id, 5000)  // message object or nil
fn nmqtt_recv(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmqtt_recv", span)?;
    let id = handle_arg(args, 0, "nmqtt_recv", span)?;
    let timeout = if args.len() >= 2 {
        match &*args[1].borrow() {
            Value::Nil => None,
            Value::Int(n) if *n < 0 => {
                return Err(type_err(span, "timeout_ms must be non-negative"));
            }
            Value::Int(n) => Some(Duration::from_millis(*n as u64)),
            other => {
                return Err(type_err(
                    span,
                    format!("timeout_ms must be int, got {}", other.type_name()),
                ));
            }
        }
    } else {
        None
    };
    with_client(id, span, |c| match c.recv(timeout) {
        Ok(Some(msg)) => Ok(message_to_value(msg)),
        Ok(None) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(mqtt_err_value(span, e)),
    })
}

// >>> // nmqtt.disconnect(id)
fn nmqtt_disconnect(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmqtt_disconnect", span)?;
    let id = handle_arg(args, 0, "nmqtt_disconnect", span)?;
    with_client(id, span, |c| {
        let _ = c.disconnect();
        Ok(ok_bool(true))
    })
}

// >>> // nmqtt.reconnect(id)
fn nmqtt_reconnect(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmqtt_reconnect", span)?;
    let id = handle_arg(args, 0, "nmqtt_reconnect", span)?;
    with_client(id, span, |c| match c.reconnect() {
        Ok(()) => Ok(ok_bool(true)),
        Err(e) => Ok(mqtt_err_value(span, e)),
    })
}

// >>> // nmqtt.is_connected(id)
fn nmqtt_is_connected(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmqtt_is_connected", span)?;
    let id = handle_arg(args, 0, "nmqtt_is_connected", span)?;
    with_client(id, span, |c| Ok(ok_bool(c.is_connected())))
}

// >>> // nmqtt.client_id(id)
fn nmqtt_client_id(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmqtt_client_id", span)?;
    let id = handle_arg(args, 0, "nmqtt_client_id", span)?;
    with_client(id, span, |c| Ok(ok_str(c.client_id())))
}

// >>> // nmqtt.ping(id)
fn nmqtt_ping(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmqtt_ping", span)?;
    let id = handle_arg(args, 0, "nmqtt_ping", span)?;
    with_client(id, span, |c| match c.ping() {
        Ok(()) => Ok(ok_bool(true)),
        Err(e) => Ok(mqtt_err_value(span, e)),
    })
}

// >>> // nmqtt.close(id)
fn nmqtt_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmqtt_close", span)?;
    let id = handle_arg(args, 0, "nmqtt_close", span)?;
    CLIENTS.with(|c| {
        let mut map = c.borrow_mut();
        if let Some(mut client) = map.remove(&id) {
            let _ = client.disconnect();
            Ok(ok_bool(true))
        } else {
            Err(RuntimeError::at(
                span,
                E_HANDLE,
                format!("nmqtt: invalid or closed handle {id}"),
            ))
        }
    })
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nmqtt_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nmqtt_fns![
    ("nmqtt_connect", "connect", nmqtt_connect),
    ("nmqtt_publish", "publish", nmqtt_publish),
    ("nmqtt_subscribe", "subscribe", nmqtt_subscribe),
    ("nmqtt_unsubscribe", "unsubscribe", nmqtt_unsubscribe),
    ("nmqtt_recv", "recv", nmqtt_recv),
    ("nmqtt_disconnect", "disconnect", nmqtt_disconnect),
    ("nmqtt_reconnect", "reconnect", nmqtt_reconnect),
    ("nmqtt_is_connected", "is_connected", nmqtt_is_connected),
    ("nmqtt_client_id", "client_id", nmqtt_client_id),
    ("nmqtt_ping", "ping", nmqtt_ping),
    ("nmqtt_close", "close", nmqtt_close),
    ("nmqtt_topic_matches", "topic_matches", nmqtt_topic_matches),
    ("nmqtt_encode_connect", "encode_connect", nmqtt_encode_connect),
    ("nmqtt_encode_publish", "encode_publish", nmqtt_encode_publish),
    ("nmqtt_decode_packet", "decode_packet", nmqtt_decode_packet),
    ("nmqtt_packet_type", "packet_type", nmqtt_packet_type),
];

fn all_builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nmqtt";
pub const MODULE_PATHS: &[&str] = &["nmqtt", "std/nmqtt"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;
    use niao_mqtt::MockBroker;
    use std::thread;
    use std::time::Duration;

    fn span() -> Span {
        Span::dummy()
    }

    #[test]
    fn topic_matches_doctest() {
        let v = nmqtt_topic_matches(
            &[
                Value::String("a/+/c".into()).ref_cell(),
                Value::String("a/b/c".into()).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        assert_eq!(*v.borrow(), Value::Bool(true));
    }

    #[test]
    fn encode_publish_doctest() {
        let p = nmqtt_encode_publish(
            &[
                Value::String("t/a".into()).ref_cell(),
                Value::String("hi".into()).ref_cell(),
                Value::Object({
                    let mut m = HashMap::new();
                    m.insert("qos".into(), Value::Int(0).ref_cell());
                    m
                })
                .ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let t = nmqtt_packet_type(&[p], span()).unwrap();
        assert_eq!(*t.borrow(), Value::String("PUBLISH".into()));
    }

    #[test]
    fn encode_connect_and_decode() {
        let mut cfg = HashMap::new();
        cfg.insert("host".into(), Value::String("localhost".into()).ref_cell());
        cfg.insert("client_id".into(), Value::String("demo".into()).ref_cell());
        cfg.insert(
            "will".into(),
            Value::Object({
                let mut w = HashMap::new();
                w.insert("topic".into(), Value::String("status".into()).ref_cell());
                w.insert("payload".into(), Value::String("offline".into()).ref_cell());
                w
            })
            .ref_cell(),
        );
        let pkt = nmqtt_encode_connect(&[Value::Object(cfg).ref_cell()], span()).unwrap();
        let dec = nmqtt_decode_packet(&[pkt], span()).unwrap();
        match &*dec.borrow() {
            Value::Object(m) => {
                assert_eq!(
                    *m.get("type").unwrap().borrow(),
                    Value::String("CONNECT".into())
                );
                assert_eq!(
                    *m.get("client_id").unwrap().borrow(),
                    Value::String("demo".into())
                );
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn connect_arity_and_type() {
        let err = nmqtt_connect(&[], span()).unwrap_err();
        assert_eq!(err.code(), E_ARITY);
        let err = nmqtt_connect(&[Value::Int(1).ref_cell()], span()).unwrap_err();
        assert_eq!(err.code(), E_TYPE);
    }

    #[test]
    fn invalid_handle() {
        let err = nmqtt_is_connected(&[Value::Int(99999).ref_cell()], span()).unwrap_err();
        assert_eq!(err.code(), E_HANDLE);
    }

    #[test]
    fn empty_topic_publish_encode() {
        let v = nmqtt_encode_publish(
            &[
                Value::String("".into()).ref_cell(),
                Value::String("x".into()).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn mqtt5_encode() {
        let mut cfg = HashMap::new();
        cfg.insert("host".into(), Value::String("h".into()).ref_cell());
        cfg.insert("client_id".into(), Value::String("v5".into()).ref_cell());
        cfg.insert("protocol".into(), Value::String("5".into()).ref_cell());
        let pkt = nmqtt_encode_connect(&[Value::Object(cfg).ref_cell()], span()).unwrap();
        let dec = nmqtt_decode_packet(&[pkt], span()).unwrap();
        match &*dec.borrow() {
            Value::Object(m) => {
                assert_eq!(*m.get("protocol_level").unwrap().borrow(), Value::Int(5));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn mock_broker_pub_sub() {
        let broker = MockBroker::start();
        let port = broker.port() as i64;
        let mut cfg = HashMap::new();
        cfg.insert(
            "host".into(),
            Value::String("127.0.0.1".into()).ref_cell(),
        );
        cfg.insert("port".into(), Value::Int(port).ref_cell());
        cfg.insert(
            "client_id".into(),
            Value::String("sub-rt".into()).ref_cell(),
        );
        let sub = nmqtt_connect(&[Value::Object(cfg.clone()).ref_cell()], span()).unwrap();
        let sub_id = match &*sub.borrow() {
            Value::Int(n) => *n,
            Value::Error(e) => panic!("connect failed: {}", e.message),
            other => panic!("expected handle, got {other:?}"),
        };
        nmqtt_subscribe(
            &[
                Value::Int(sub_id).ref_cell(),
                Value::String("demo/#".into()).ref_cell(),
                Value::Int(0).ref_cell(),
            ],
            span(),
        )
        .unwrap();

        cfg.insert(
            "client_id".into(),
            Value::String("pub-rt".into()).ref_cell(),
        );
        let pubc = nmqtt_connect(&[Value::Object(cfg).ref_cell()], span()).unwrap();
        let pub_id = match &*pubc.borrow() {
            Value::Int(n) => *n,
            other => panic!("expected handle, got {other:?}"),
        };
        nmqtt_publish(
            &[
                Value::Int(pub_id).ref_cell(),
                Value::String("demo/hello".into()).ref_cell(),
                Value::String("world".into()).ref_cell(),
            ],
            span(),
        )
        .unwrap();

        thread::sleep(Duration::from_millis(50));
        let msg = nmqtt_recv(
            &[
                Value::Int(sub_id).ref_cell(),
                Value::Int(2000).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        match &*msg.borrow() {
            Value::Object(m) => {
                assert_eq!(
                    *m.get("topic").unwrap().borrow(),
                    Value::String("demo/hello".into())
                );
                assert_eq!(
                    *m.get("payload").unwrap().borrow(),
                    Value::String("world".into())
                );
            }
            other => panic!("expected message, got {other:?}"),
        }

        nmqtt_close(&[Value::Int(pub_id).ref_cell()], span()).unwrap();
        nmqtt_close(&[Value::Int(sub_id).ref_cell()], span()).unwrap();
        broker.shutdown();
    }

    #[test]
    fn topic_edge_cases() {
        assert!(!topic_matches("", "a"));
        assert!(!topic_matches("a", ""));
        assert!(topic_matches("#", "a/b/c"));
        assert!(!topic_matches("sport/#", "$SYS/broker"));
        assert!(topic_matches("$SYS/#", "$SYS/broker"));
    }
}
