use crate::error::MsgpackError;
use crate::options::PackOptions;
use crate::options::TIMESTAMP_EXT;
use crate::value::MsgValue;
use crate::MAX_BYTES;
use niao_bignum::BigInt;
use rmpv::Value;
use std::collections::HashMap;

fn check_size(n: usize) -> Result<(), MsgpackError> {
    if n > MAX_BYTES {
        return Err(MsgpackError::TooLarge(n));
    }
    Ok(())
}

fn bigint_to_rmpv(n: &BigInt, opts: &PackOptions) -> Result<Value, MsgpackError> {
    if let Some(i) = n.to_i64() {
        return Ok(Value::Integer(i.into()));
    }
    if let Some(u) = n.to_u64() {
        return Ok(Value::Integer(u.into()));
    }
    if opts.bigint_as_string {
        return Ok(Value::String(n.to_string().into()));
    }
    Err(MsgpackError::Type(format!(
        "bigint {n} does not fit in 64 bits (enable bigint_as_string)"
    )))
}

fn is_timestamp_map(map: &HashMap<String, MsgValue>) -> Option<(i64, u32)> {
    let sec = map
        .get("sec")
        .or_else(|| map.get("seconds"))
        .and_then(|v| match v {
            MsgValue::Int(n) => Some(*n),
            MsgValue::Uint(n) if *n <= i64::MAX as u64 => Some(*n as i64),
            _ => None,
        })?;
    let nsec = map
        .get("nsec")
        .or_else(|| map.get("nanoseconds"))
        .and_then(|v| match v {
            MsgValue::Int(n) if *n >= 0 => Some(*n as u32),
            MsgValue::Uint(n) if *n <= u32::MAX as u64 => Some(*n as u32),
            _ => None,
        })
        .unwrap_or(0);
    Some((sec, nsec))
}

fn encode_timestamp(sec: i64, nsec: u32) -> Result<Value, MsgpackError> {
    if sec < 0 {
        return Err(MsgpackError::Encode(
            "negative timestamp seconds not supported".into(),
        ));
    }
    let data = if nsec == 0 {
        let mut buf = [0u8; 4];
        buf[..4].copy_from_slice(&(sec as u32).to_be_bytes());
        buf[..4].to_vec()
    } else if sec <= u32::MAX as i64 {
        let mut buf = [0u8; 8];
        let ts = ((sec as u64) << 32) | (nsec as u64);
        buf[..8].copy_from_slice(&ts.to_be_bytes());
        buf.to_vec()
    } else {
        let mut buf = [0u8; 12];
        buf[..8].copy_from_slice(&(sec as u64).to_be_bytes());
        buf[8..12].copy_from_slice(&nsec.to_be_bytes());
        buf.to_vec()
    };
    Ok(Value::Ext(TIMESTAMP_EXT, data.into()))
}

pub(crate) fn msg_to_rmpv(
    value: &MsgValue,
    opts: &PackOptions,
    depth: usize,
) -> Result<Value, MsgpackError> {
    if depth > 512 {
        return Err(MsgpackError::Encode("nesting depth exceeds limit".into()));
    }
    match value {
        MsgValue::Nil => Ok(Value::Nil),
        MsgValue::Bool(b) => Ok(Value::Boolean(*b)),
        MsgValue::Int(n) => Ok(Value::Integer((*n).into())),
        MsgValue::Uint(n) => {
            if *n <= i64::MAX as u64 {
                Ok(Value::Integer((*n as i64).into()))
            } else {
                Ok(Value::Integer((*n).into()))
            }
        }
        MsgValue::BigInt(n) => bigint_to_rmpv(n, opts),
        MsgValue::Float(f) => {
            if opts.use_single_float {
                let f32 = *f as f32;
                if f32.is_finite() && (f32 as f64 - *f).abs() < f64::EPSILON {
                    return Ok(Value::F32(f32));
                }
            }
            Ok(Value::F64(*f))
        }
        MsgValue::String(s) => {
            if opts.use_bin_type {
                Ok(Value::Binary(s.as_bytes().to_vec().into()))
            } else {
                Ok(Value::String(s.clone().into()))
            }
        }
        MsgValue::Binary(b) => Ok(Value::Binary(b.clone().into())),
        MsgValue::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(msg_to_rmpv(item, opts, depth + 1)?);
            }
            Ok(Value::Array(out))
        }
        MsgValue::Map(pairs) => {
            if opts.timestamp && pairs.len() == 2 {
                let mut map = HashMap::new();
                for (k, v) in pairs {
                    if let MsgValue::String(key) = k {
                        map.insert(key.clone(), v.clone());
                    }
                }
                if let Some((sec, nsec)) = is_timestamp_map(&map) {
                    if map.contains_key("sec") || map.contains_key("seconds") {
                        return encode_timestamp(sec, nsec);
                    }
                }
            }
            let mut out = Vec::with_capacity(pairs.len());
            for (k, v) in pairs {
                out.push((
                    msg_to_rmpv(k, opts, depth + 1)?,
                    msg_to_rmpv(v, opts, depth + 1)?,
                ));
            }
            Ok(Value::Map(out))
        }
        MsgValue::Ext { code, data } => Ok(Value::Ext(*code, data.clone().into())),
        MsgValue::Timestamp { sec, nsec } => encode_timestamp(*sec, *nsec),
    }
}

/// Pack a value tree into MessagePack bytes.
pub fn pack(value: &MsgValue, opts: &PackOptions) -> Result<Vec<u8>, MsgpackError> {
    let rmpv = msg_to_rmpv(value, opts, 0)?;
    let mut buf = Vec::new();
    rmpv::encode::write_value(&mut buf, &rmpv)?;
    check_size(buf.len())?;
    Ok(buf)
}

/// Pack multiple values sequentially (streaming frame).
pub fn pack_all(values: &[MsgValue], opts: &PackOptions) -> Result<Vec<u8>, MsgpackError> {
    let mut buf = Vec::new();
    for value in values {
        let rmpv = msg_to_rmpv(value, opts, 0)?;
        rmpv::encode::write_value(&mut buf, &rmpv)?;
        check_size(buf.len())?;
    }
    Ok(buf)
}

/// Build a MessagePack extension value from code + payload.
pub fn pack_ext(code: i8, data: &[u8]) -> Result<Vec<u8>, MsgpackError> {
    let value = Value::Ext(code, data.to_vec().into());
    let mut buf = Vec::new();
    rmpv::encode::write_value(&mut buf, &value)?;
    Ok(buf)
}

/// Encode a timestamp extension payload.
pub fn pack_timestamp(sec: i64, nsec: u32) -> Result<Vec<u8>, MsgpackError> {
    pack(&MsgValue::Timestamp { sec, nsec }, &PackOptions::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::UnpackOptions;
    use crate::unpack::unpack;

    #[test]
    fn roundtrip_primitives() {
        let values = vec![
            MsgValue::Nil,
            MsgValue::Bool(true),
            MsgValue::Int(-42),
            MsgValue::Int(999),
            MsgValue::Float(3.14),
            MsgValue::String("hello".into()),
            MsgValue::Binary(vec![1, 2, 3]),
        ];
        for v in values {
            let bytes = pack(&v, &PackOptions::default()).unwrap();
            let out = unpack(&bytes, &Default::default()).unwrap();
            assert_eq!(v, out);
        }
    }

    #[test]
    fn timestamp_ext_roundtrip() {
        let ts = MsgValue::Map(vec![
            (MsgValue::String("sec".into()), MsgValue::Int(1_600_000_000)),
            (MsgValue::String("nsec".into()), MsgValue::Int(500)),
        ]);
        let opts = PackOptions {
            timestamp: true,
            ..Default::default()
        };
        let bytes = pack(&ts, &opts).unwrap();
        let out = unpack(&bytes, &UnpackOptions::default()).unwrap();
        match out {
            MsgValue::Map(pairs) => {
                let map: HashMap<_, _> = pairs
                    .into_iter()
                    .filter_map(|(k, v)| match k {
                        MsgValue::String(s) => Some((s, v)),
                        _ => None,
                    })
                    .collect();
                assert_eq!(map.get("sec"), Some(&MsgValue::Int(1_600_000_000)));
            }
            other => panic!("expected timestamp map, got {other:?}"),
        }
    }
}
