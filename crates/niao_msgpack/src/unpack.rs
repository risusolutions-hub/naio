use crate::error::MsgpackError;
use crate::options::UnpackOptions;
use crate::options::TIMESTAMP_EXT;
use crate::value::{ext_object, MsgValue};
use crate::MAX_BYTES;
use niao_bignum::BigInt;
use rmpv::Value;
use std::io::Cursor;

fn check_input_size(n: usize) -> Result<(), MsgpackError> {
    if n > MAX_BYTES {
        return Err(MsgpackError::TooLarge(n));
    }
    Ok(())
}

fn decode_timestamp(data: &[u8]) -> Result<MsgValue, MsgpackError> {
    match data.len() {
        4 => {
            let mut arr = [0u8; 4];
            arr.copy_from_slice(data);
            let sec = u32::from_be_bytes(arr) as i64;
            Ok(MsgValue::Map(vec![
                (MsgValue::String("sec".into()), MsgValue::Int(sec)),
                (MsgValue::String("nsec".into()), MsgValue::Int(0)),
            ]))
        }
        8 => {
            let mut arr = [0u8; 8];
            arr.copy_from_slice(data);
            let combined = u64::from_be_bytes(arr);
            let sec = (combined >> 32) as i64;
            let nsec = (combined & 0xFFFF_FFFF) as u32;
            Ok(MsgValue::Map(vec![
                (MsgValue::String("sec".into()), MsgValue::Int(sec)),
                (MsgValue::String("nsec".into()), MsgValue::Int(nsec as i64)),
            ]))
        }
        12 => {
            let mut sec_arr = [0u8; 8];
            sec_arr.copy_from_slice(&data[..8]);
            let mut nsec_arr = [0u8; 4];
            nsec_arr.copy_from_slice(&data[8..12]);
            let sec = u64::from_be_bytes(sec_arr) as i64;
            let nsec = u32::from_be_bytes(nsec_arr);
            Ok(MsgValue::Map(vec![
                (MsgValue::String("sec".into()), MsgValue::Int(sec)),
                (MsgValue::String("nsec".into()), MsgValue::Int(nsec as i64)),
            ]))
        }
        n => Err(MsgpackError::Decode(format!(
            "invalid timestamp extension length {n}"
        ))),
    }
}

fn maybe_bigint_from_str(s: &str, opts: &UnpackOptions) -> Option<MsgValue> {
    if !opts.bigint_as_string {
        return None;
    }
    if s.is_empty() || s.starts_with('0') && s.len() > 1 {
        return None;
    }
    if s.chars().all(|c| c.is_ascii_digit() || c == '-') {
        if let Ok(n) = s.parse::<i64>() {
            return Some(MsgValue::Int(n));
        }
        if let Ok(n) = s.parse::<BigInt>() {
            return Some(MsgValue::BigInt(n));
        }
    }
    None
}

fn key_to_string(value: &MsgValue, opts: &UnpackOptions) -> Result<String, MsgpackError> {
    match value {
        MsgValue::String(s) => Ok(s.clone()),
        MsgValue::Binary(b) => String::from_utf8(b.clone()).map_err(|_| {
            if opts.strict_map_key {
                MsgpackError::StrictMapKey("non-utf8 binary map key".into())
            } else {
                MsgpackError::StrictMapKey(format!("{value:?}"))
            }
        }),
        other if opts.strict_map_key => Err(MsgpackError::StrictMapKey(format!("{other:?}"))),
        MsgValue::Int(n) => Ok(n.to_string()),
        MsgValue::Uint(n) => Ok(n.to_string()),
        MsgValue::Bool(b) => Ok(b.to_string()),
        other => Err(MsgpackError::StrictMapKey(format!("{other:?}"))),
    }
}

pub(crate) fn rmpv_to_msg(
    value: Value,
    opts: &UnpackOptions,
    depth: usize,
) -> Result<MsgValue, MsgpackError> {
    if depth > opts.max_depth {
        return Err(MsgpackError::Decode("nesting depth exceeds limit".into()));
    }
    match value {
        Value::Nil => Ok(MsgValue::Nil),
        Value::Boolean(b) => Ok(MsgValue::Bool(b)),
        Value::Integer(n) => {
            if let Some(i) = n.as_i64() {
                Ok(MsgValue::Int(i))
            } else if let Some(u) = n.as_u64() {
                if u <= i64::MAX as u64 {
                    Ok(MsgValue::Int(u as i64))
                } else {
                    Ok(MsgValue::Uint(u))
                }
            } else {
                Err(MsgpackError::Decode("integer out of range".into()))
            }
        }
        Value::F32(f) => Ok(MsgValue::Float(f as f64)),
        Value::F64(f) => Ok(MsgValue::Float(f)),
        Value::String(s) => {
            let s = s
                .as_str()
                .ok_or_else(|| MsgpackError::Decode("invalid UTF-8 string".into()))?;
            if opts.raw {
                return Ok(MsgValue::Binary(s.as_bytes().to_vec()));
            }
            if let Some(v) = maybe_bigint_from_str(s, opts) {
                return Ok(v);
            }
            Ok(MsgValue::String(s.to_string()))
        }
        Value::Binary(b) => Ok(MsgValue::Binary(b.to_vec())),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(rmpv_to_msg(item, opts, depth + 1)?);
            }
            Ok(MsgValue::Array(out))
        }
        Value::Map(pairs) => {
            let mut out = Vec::with_capacity(pairs.len());
            for (k, v) in pairs {
                let key_msg = rmpv_to_msg(k, opts, depth + 1)?;
                let key_str = key_to_string(&key_msg, opts)?;
                let val_msg = rmpv_to_msg(v, opts, depth + 1)?;
                out.push((MsgValue::String(key_str), val_msg));
            }
            Ok(MsgValue::Map(out))
        }
        Value::Ext(code, data) => {
            let bytes = data.to_vec();
            if opts.timestamp && code == TIMESTAMP_EXT {
                return decode_timestamp(&bytes);
            }
            let map = ext_object(code, bytes);
            Ok(MsgValue::Map(
                map.into_iter()
                    .map(|(k, v)| (MsgValue::String(k), v))
                    .collect(),
            ))
        }
    }
}

/// Unpack one MessagePack value from bytes.
pub fn unpack(data: &[u8], opts: &UnpackOptions) -> Result<MsgValue, MsgpackError> {
    check_input_size(data.len())?;
    let mut cursor = Cursor::new(data);
    let value = rmpv::decode::read_value(&mut cursor)?;
    let consumed = cursor.position() as usize;
    let out = rmpv_to_msg(value, opts, 0)?;
    if consumed < data.len() {
        // trailing bytes are allowed for compatibility with streaming callers
    }
    Ok(out)
}

/// Unpack every top-level value in a byte buffer.
pub fn unpack_all(data: &[u8], opts: &UnpackOptions) -> Result<Vec<MsgValue>, MsgpackError> {
    check_input_size(data.len())?;
    let mut cursor = Cursor::new(data);
    let mut out = Vec::new();
    while (cursor.position() as usize) < data.len() {
        let value = match rmpv::decode::read_value(&mut cursor) {
            Ok(v) => v,
            Err(e) => {
                if out.is_empty() {
                    return Err(e.into());
                }
                break;
            }
        };
        out.push(rmpv_to_msg(value, opts, 0)?);
    }
    Ok(out)
}

/// Return true when `data` contains at least one valid MessagePack value.
pub fn is_valid(data: &[u8]) -> bool {
    if data.is_empty() || data.len() > MAX_BYTES {
        return false;
    }
    let mut cursor = Cursor::new(data);
    rmpv::decode::read_value(&mut cursor).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::pack;

    #[test]
    fn nested_map() {
        let value = MsgValue::Map(vec![
            (
                MsgValue::String("a".into()),
                MsgValue::Array(vec![MsgValue::Int(1), MsgValue::Int(2)]),
            ),
            (MsgValue::String("b".into()), MsgValue::String("x".into())),
        ]);
        let bytes = pack(&value, &Default::default()).unwrap();
        let out = unpack(&bytes, &Default::default()).unwrap();
        assert_eq!(value, out);
    }

    #[test]
    fn strict_map_key_rejects_int_keys() {
        let value = Value::Map(vec![(Value::Integer(1.into()), Value::Integer(2.into()))]);
        let mut buf = Vec::new();
        rmpv::encode::write_value(&mut buf, &value).unwrap();
        let err = unpack(&buf, &UnpackOptions::default()).unwrap_err();
        assert!(matches!(err, MsgpackError::StrictMapKey(_)));
    }
}
