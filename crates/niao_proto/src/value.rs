use crate::error::{ProtoError, ProtoResult};
use bytes::Bytes;
use prost_reflect::{DynamicMessage, FieldDescriptor, Kind, MapKey, ReflectMessage, Value};
use serde_json::{Map, Number, Value as JsonValue};
use std::collections::HashMap;

/// Portable field value used by the Niao runtime bridge.
#[derive(Debug, Clone)]
pub enum NiaoFieldValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    Array(Vec<NiaoFieldValue>),
    Object(HashMap<String, NiaoFieldValue>),
    Message(ProtoMessageRef),
}

#[derive(Debug, Clone)]
pub struct ProtoMessageRef {
    pub full_name: String,
    pub fields: HashMap<String, NiaoFieldValue>,
}

pub fn dynamic_to_json_value(val: &Value) -> ProtoResult<JsonValue> {
    Ok(match val {
        Value::Bool(b) => JsonValue::Bool(*b),
        Value::I32(n) => JsonValue::Number(Number::from(*n)),
        Value::I64(n) => JsonValue::Number(Number::from(*n)),
        Value::U32(n) => JsonValue::Number(Number::from(*n)),
        Value::U64(n) => JsonValue::Number(Number::from(*n)),
        Value::F32(f) => JsonValue::Number(
            Number::from_f64(*f as f64).ok_or_else(|| ProtoError::Json("non-finite f32".into()))?,
        ),
        Value::F64(f) => JsonValue::Number(
            Number::from_f64(*f).ok_or_else(|| ProtoError::Json("non-finite f64".into()))?,
        ),
        Value::String(s) => JsonValue::String(s.clone()),
        Value::Bytes(b) => JsonValue::String(base64_encode(b)),
        Value::EnumNumber(n) => JsonValue::Number(Number::from(*n)),
        Value::List(items) => JsonValue::Array(
            items
                .iter()
                .map(dynamic_to_json_value)
                .collect::<ProtoResult<_>>()?,
        ),
        Value::Map(pairs) => {
            let mut map = Map::new();
            for (k, v) in pairs {
                map.insert(map_key_to_string(k)?, dynamic_to_json_value(v)?);
            }
            JsonValue::Object(map)
        }
        Value::Message(m) => message_to_json_object(m)?,
    })
}

fn map_key_to_string(key: &MapKey) -> ProtoResult<String> {
    Ok(match key {
        MapKey::Bool(b) => b.to_string(),
        MapKey::I32(n) => n.to_string(),
        MapKey::I64(n) => n.to_string(),
        MapKey::U32(n) => n.to_string(),
        MapKey::U64(n) => n.to_string(),
        MapKey::String(s) => s.clone(),
    })
}

fn string_to_map_key(kind: &Kind, s: &str) -> ProtoResult<MapKey> {
    Ok(match kind {
        Kind::Bool => MapKey::Bool(
            s.parse()
                .map_err(|_| ProtoError::Type("invalid bool map key".into()))?,
        ),
        Kind::Int32 | Kind::Sint32 | Kind::Sfixed32 => MapKey::I32(
            s.parse()
                .map_err(|_| ProtoError::Type("invalid int32 map key".into()))?,
        ),
        Kind::Int64 | Kind::Sint64 | Kind::Sfixed64 => MapKey::I64(
            s.parse()
                .map_err(|_| ProtoError::Type("invalid int64 map key".into()))?,
        ),
        Kind::Uint32 | Kind::Fixed32 => MapKey::U32(
            s.parse()
                .map_err(|_| ProtoError::Type("invalid uint32 map key".into()))?,
        ),
        Kind::Uint64 | Kind::Fixed64 => MapKey::U64(
            s.parse()
                .map_err(|_| ProtoError::Type("invalid uint64 map key".into()))?,
        ),
        Kind::String => MapKey::String(s.to_string()),
        other => {
            return Err(ProtoError::Type(format!(
                "unsupported map key kind: {other:?}"
            )))
        }
    })
}

fn message_to_json_object(msg: &DynamicMessage) -> ProtoResult<JsonValue> {
    let mut map = Map::new();
    for field in msg.descriptor().fields() {
        if msg.has_field(&field) {
            let val = msg.get_field(&field);
            map.insert(
                field.json_name().to_string(),
                dynamic_to_json_value(val.as_ref())?,
            );
        }
    }
    Ok(JsonValue::Object(map))
}

pub fn json_value_to_dynamic(field: &FieldDescriptor, value: &JsonValue) -> ProtoResult<Value> {
    if value.is_null() {
        return Ok(Value::Bool(false));
    }
    if field.is_map() {
        let obj = value.as_object().ok_or_else(|| {
            ProtoError::Type(format!("map field {} expects object", field.name()))
        })?;
        let kind = field.kind();
        let entry_desc = kind
            .as_message()
            .expect("map field should reference map entry message");
        let key_field = entry_desc.get_field_by_name("key").expect("map key");
        let val_field = entry_desc.get_field_by_name("value").expect("map value");
        let mut pairs = HashMap::new();
        for (k, v) in obj {
            let key = string_to_map_key(&key_field.kind(), k)?;
            let val = json_value_to_dynamic(&val_field, v)?;
            pairs.insert(key, val);
        }
        return Ok(Value::Map(pairs));
    }
    if field.is_list() {
        let arr = value.as_array().ok_or_else(|| {
            ProtoError::Type(format!("repeated field {} expects array", field.name()))
        })?;
        let kind = field.kind();
        let mut out = Vec::with_capacity(arr.len());
        for item in arr {
            out.push(json_to_list_element(kind.clone(), field.name(), item)?);
        }
        return Ok(Value::List(out));
    }
    scalar_from_json(field, value)
}

fn json_to_list_element(kind: Kind, field_name: &str, value: &JsonValue) -> ProtoResult<Value> {
    match kind {
        Kind::Message(m) => {
            let obj = value
                .as_object()
                .ok_or_else(|| ProtoError::Type(format!("{field_name}[] expects object")))?;
            Ok(Value::Message(json_object_to_message(&m, obj)?))
        }
        other => scalar_from_kind(&other, field_name, value),
    }
}

fn scalar_from_json(field: &FieldDescriptor, value: &JsonValue) -> ProtoResult<Value> {
    scalar_from_kind(&field.kind(), field.name(), value)
}

fn scalar_from_kind(kind: &Kind, field_name: &str, value: &JsonValue) -> ProtoResult<Value> {
    match kind {
        Kind::Bool => {
            Ok(Value::Bool(value.as_bool().ok_or_else(|| {
                ProtoError::Type(format!("{field_name} expects bool"))
            })?))
        }
        Kind::String => Ok(Value::String(
            value
                .as_str()
                .ok_or_else(|| ProtoError::Type(format!("{field_name} expects string")))?
                .to_string(),
        )),
        Kind::Bytes => {
            if let Some(s) = value.as_str() {
                return Ok(Value::Bytes(Bytes::from(base64_decode(s)?)));
            }
            if let Some(arr) = value.as_array() {
                let mut bytes = Vec::with_capacity(arr.len());
                for item in arr {
                    let n = item
                        .as_i64()
                        .ok_or_else(|| ProtoError::Type("bytes array expects ints".into()))?;
                    bytes.push(n as u8);
                }
                return Ok(Value::Bytes(Bytes::from(bytes)));
            }
            Err(ProtoError::Type(format!(
                "{field_name} expects base64 string or byte array"
            )))
        }
        Kind::Enum(e) => {
            if let Some(s) = value.as_str() {
                let ev = e
                    .get_value_by_name(s)
                    .ok_or_else(|| ProtoError::Type(format!("unknown enum value {s}")))?;
                return Ok(Value::EnumNumber(ev.number()));
            }
            let n = value.as_i64().ok_or_else(|| {
                ProtoError::Type(format!("{field_name} expects enum name or number"))
            })?;
            Ok(Value::EnumNumber(n as i32))
        }
        Kind::Message(m) => {
            let obj = value
                .as_object()
                .ok_or_else(|| ProtoError::Type(format!("{field_name} expects object")))?;
            Ok(Value::Message(json_object_to_message(m, obj)?))
        }
        Kind::Double => {
            Ok(Value::F64(value.as_f64().ok_or_else(|| {
                ProtoError::Type(format!("{field_name} expects number"))
            })?))
        }
        Kind::Float => Ok(Value::F32(
            value
                .as_f64()
                .ok_or_else(|| ProtoError::Type(format!("{field_name} expects number")))?
                as f32,
        )),
        Kind::Int32 | Kind::Sint32 | Kind::Sfixed32 => {
            Ok(Value::I32(value.as_i64().unwrap_or(0) as i32))
        }
        Kind::Int64 | Kind::Sint64 | Kind::Sfixed64 => Ok(Value::I64(value.as_i64().unwrap_or(0))),
        Kind::Uint32 | Kind::Fixed32 => Ok(Value::U32(value.as_u64().unwrap_or(0) as u32)),
        Kind::Uint64 | Kind::Fixed64 => Ok(Value::U64(value.as_u64().unwrap_or(0))),
    }
}

fn json_object_to_message(
    desc: &prost_reflect::MessageDescriptor,
    obj: &Map<String, JsonValue>,
) -> ProtoResult<DynamicMessage> {
    let mut msg = DynamicMessage::new(desc.clone());
    for field in desc.fields() {
        let key = field.json_name();
        if let Some(val) = obj.get(key).or_else(|| obj.get(field.name())) {
            if val.is_null() {
                continue;
            }
            let dynamic = json_value_to_dynamic(&field, val)?;
            msg.set_field(&field, dynamic);
        }
    }
    Ok(msg)
}

pub fn niao_to_dynamic(field: &FieldDescriptor, value: &NiaoFieldValue) -> ProtoResult<Value> {
    if field.is_map() {
        let NiaoFieldValue::Object(map) = value else {
            return Err(ProtoError::Type(format!(
                "map field {} expects object",
                field.name()
            )));
        };
        let kind = field.kind();
        let entry_desc = kind
            .as_message()
            .expect("map field should reference map entry message");
        let key_field = entry_desc.get_field_by_name("key").expect("map key");
        let val_field = entry_desc.get_field_by_name("value").expect("map value");
        let mut pairs = HashMap::new();
        for (k, v) in map {
            let key = string_to_map_key(&key_field.kind(), k)?;
            let val = niao_to_dynamic(&val_field, v)?;
            pairs.insert(key, val);
        }
        return Ok(Value::Map(pairs));
    }
    if field.is_list() {
        let NiaoFieldValue::Array(items) = value else {
            return Err(ProtoError::Type(format!(
                "repeated field {} expects array",
                field.name()
            )));
        };
        let kind = field.kind();
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            out.push(niao_to_list_element(kind.clone(), field.name(), item)?);
        }
        return Ok(Value::List(out));
    }
    scalar_from_niao(field, value)
}

fn niao_to_list_element(
    kind: Kind,
    field_name: &str,
    value: &NiaoFieldValue,
) -> ProtoResult<Value> {
    match kind {
        Kind::Message(m) => {
            let obj = niao_object(value, field_name)?;
            Ok(Value::Message(niao_object_to_message(&m, obj)?))
        }
        other => scalar_from_niao_kind(&other, field_name, value),
    }
}

fn niao_object<'a>(
    value: &'a NiaoFieldValue,
    field_name: &str,
) -> ProtoResult<&'a HashMap<String, NiaoFieldValue>> {
    match value {
        NiaoFieldValue::Object(obj) => Ok(obj),
        NiaoFieldValue::Message(m) => Ok(&m.fields),
        _ => Err(ProtoError::Type(format!("{field_name}[] expects object"))),
    }
}

fn scalar_from_niao(field: &FieldDescriptor, value: &NiaoFieldValue) -> ProtoResult<Value> {
    scalar_from_niao_kind(&field.kind(), field.name(), value)
}

fn scalar_from_niao_kind(
    kind: &Kind,
    field_name: &str,
    value: &NiaoFieldValue,
) -> ProtoResult<Value> {
    match kind {
        Kind::Bool => match value {
            NiaoFieldValue::Bool(b) => Ok(Value::Bool(*b)),
            NiaoFieldValue::Int(n) => Ok(Value::Bool(*n != 0)),
            _ => Err(ProtoError::Type(format!("{field_name} expects bool"))),
        },
        Kind::String => match value {
            NiaoFieldValue::String(s) => Ok(Value::String(s.clone())),
            _ => Err(ProtoError::Type(format!("{field_name} expects string"))),
        },
        Kind::Bytes => match value {
            NiaoFieldValue::Bytes(b) => Ok(Value::Bytes(Bytes::from(b.clone()))),
            NiaoFieldValue::String(s) => Ok(Value::Bytes(Bytes::from(base64_decode(s)?))),
            NiaoFieldValue::Array(items) => {
                let mut bytes = Vec::with_capacity(items.len());
                for item in items {
                    let NiaoFieldValue::Int(n) = item else {
                        return Err(ProtoError::Type("bytes array expects ints".into()));
                    };
                    bytes.push(*n as u8);
                }
                Ok(Value::Bytes(Bytes::from(bytes)))
            }
            _ => Err(ProtoError::Type(format!("{field_name} expects bytes"))),
        },
        Kind::Enum(e) => match value {
            NiaoFieldValue::String(s) => {
                let ev = e
                    .get_value_by_name(s)
                    .ok_or_else(|| ProtoError::Type(format!("unknown enum value {s}")))?;
                Ok(Value::EnumNumber(ev.number()))
            }
            NiaoFieldValue::Int(n) => Ok(Value::EnumNumber(*n as i32)),
            _ => Err(ProtoError::Type(format!("{field_name} expects enum"))),
        },
        Kind::Message(m) => match value {
            NiaoFieldValue::Object(obj) => Ok(Value::Message(niao_object_to_message(m, obj)?)),
            NiaoFieldValue::Message(mref) => {
                if mref.full_name != m.full_name() {
                    return Err(ProtoError::Type(format!(
                        "expected message {}, got {}",
                        m.full_name(),
                        mref.full_name
                    )));
                }
                Ok(Value::Message(niao_object_to_message(m, &mref.fields)?))
            }
            _ => Err(ProtoError::Type(format!(
                "{field_name} expects message object"
            ))),
        },
        Kind::Double => match value {
            NiaoFieldValue::Float(f) => Ok(Value::F64(*f)),
            NiaoFieldValue::Int(n) => Ok(Value::F64(*n as f64)),
            _ => Err(ProtoError::Type(format!("{field_name} expects number"))),
        },
        Kind::Float => match value {
            NiaoFieldValue::Float(f) => Ok(Value::F32(*f as f32)),
            NiaoFieldValue::Int(n) => Ok(Value::F32(*n as f32)),
            _ => Err(ProtoError::Type(format!("{field_name} expects number"))),
        },
        Kind::Int32 | Kind::Sint32 | Kind::Sfixed32 => match value {
            NiaoFieldValue::Int(n) => Ok(Value::I32(*n as i32)),
            _ => Err(ProtoError::Type(format!("{field_name} expects int"))),
        },
        Kind::Int64 | Kind::Sint64 | Kind::Sfixed64 => match value {
            NiaoFieldValue::Int(n) => Ok(Value::I64(*n)),
            _ => Err(ProtoError::Type(format!("{field_name} expects int"))),
        },
        Kind::Uint32 | Kind::Fixed32 => match value {
            NiaoFieldValue::Int(n) => Ok(Value::U32(*n as u32)),
            _ => Err(ProtoError::Type(format!("{field_name} expects int"))),
        },
        Kind::Uint64 | Kind::Fixed64 => match value {
            NiaoFieldValue::Int(n) => Ok(Value::U64(*n as u64)),
            _ => Err(ProtoError::Type(format!("{field_name} expects int"))),
        },
    }
}

fn niao_object_to_message(
    desc: &prost_reflect::MessageDescriptor,
    obj: &HashMap<String, NiaoFieldValue>,
) -> ProtoResult<DynamicMessage> {
    let mut msg = DynamicMessage::new(desc.clone());
    for field in desc.fields() {
        if let Some(val) = obj.get(field.name()) {
            if matches!(val, NiaoFieldValue::Null) {
                continue;
            }
            let dynamic = niao_to_dynamic(&field, val)?;
            msg.set_field(&field, dynamic);
        }
    }
    Ok(msg)
}

pub fn dynamic_to_niao(val: &Value) -> ProtoResult<NiaoFieldValue> {
    Ok(match val {
        Value::Bool(b) => NiaoFieldValue::Bool(*b),
        Value::I32(n) => NiaoFieldValue::Int(*n as i64),
        Value::I64(n) => NiaoFieldValue::Int(*n),
        Value::U32(n) => NiaoFieldValue::Int(*n as i64),
        Value::U64(n) => NiaoFieldValue::Int(*n as i64),
        Value::F32(f) => NiaoFieldValue::Float(*f as f64),
        Value::F64(f) => NiaoFieldValue::Float(*f),
        Value::String(s) => NiaoFieldValue::String(s.clone()),
        Value::Bytes(b) => NiaoFieldValue::Bytes(b.to_vec()),
        Value::EnumNumber(n) => NiaoFieldValue::Int(*n as i64),
        Value::List(items) => NiaoFieldValue::Array(
            items
                .iter()
                .map(dynamic_to_niao)
                .collect::<ProtoResult<Vec<_>>>()?,
        ),
        Value::Map(pairs) => {
            let mut map = HashMap::new();
            for (k, v) in pairs {
                map.insert(map_key_to_string(k)?, dynamic_to_niao(v)?);
            }
            NiaoFieldValue::Object(map)
        }
        Value::Message(m) => {
            let mut fields = HashMap::new();
            for field in m.descriptor().fields() {
                if m.has_field(&field) {
                    fields.insert(
                        field.name().to_string(),
                        dynamic_to_niao(m.get_field(&field).as_ref())?,
                    );
                }
            }
            NiaoFieldValue::Message(ProtoMessageRef {
                full_name: m.descriptor().full_name().to_string(),
                fields,
            })
        }
    })
}

fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 63) as usize] as char);
        out.push(TABLE[((triple >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((triple >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(triple & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn base64_decode(s: &str) -> ProtoResult<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &c in bytes {
        if c == b'=' {
            break;
        }
        let v = val(c).ok_or_else(|| ProtoError::Type("invalid base64".into()))?;
        buf = (buf << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Ok(out)
}
