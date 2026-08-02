//! CBOR decoder with tag hooks and depth limits.

use crate::error::{CborError, CborResult};
use crate::tags;
use crate::value::{bigint_from_tagged_bytes, CborValue};
use crate::DecodeOptions;
use minicbor::data::Type;
use minicbor::decode::{Decoder, Error as DecodeError};

pub fn decode_all(data: &[u8], opts: &DecodeOptions) -> CborResult<Vec<CborValue>> {
    check_len(data.len(), opts.max_bytes)?;
    let mut out = Vec::new();
    let mut offset = 0usize;
    while offset < data.len() {
        let (val, consumed) = decode_one(&data[offset..], opts)?;
        offset += consumed;
        out.push(val);
    }
    Ok(out)
}

pub fn decode(data: &[u8], opts: &DecodeOptions) -> CborResult<CborValue> {
    check_len(data.len(), opts.max_bytes)?;
    let (val, consumed) = decode_one(data, opts)?;
    if opts.reject_trailing && consumed < data.len() {
        return Err(CborError::TrailingData { offset: consumed });
    }
    Ok(val)
}

pub fn is_valid(data: &[u8]) -> bool {
    decode(data, &DecodeOptions::default()).is_ok()
}

fn check_len(n: usize, max: usize) -> CborResult<()> {
    if n > max {
        return Err(CborError::TooLarge(n));
    }
    Ok(())
}

fn decode_one(data: &[u8], opts: &DecodeOptions) -> CborResult<(CborValue, usize)> {
    let mut dec = Decoder::new(data);
    let before = dec.position();
    let val = decode_value(&mut dec, opts, 0)?;
    let consumed = dec.position() - before;
    Ok((val, consumed))
}

fn decode_value(
    dec: &mut Decoder<'_>,
    opts: &DecodeOptions,
    depth: usize,
) -> CborResult<CborValue> {
    if depth > opts.max_depth {
        return Err(CborError::DepthExceeded {
            max: opts.max_depth,
        });
    }
    let ty = dec.datatype().map_err(map_decode_err)?;
    let val = match ty {
        Type::Null => {
            dec.null().map_err(map_decode_err)?;
            CborValue::Null
        }
        Type::Undefined => {
            dec.undefined().map_err(map_decode_err)?;
            CborValue::Undefined
        }
        Type::Bool => CborValue::Bool(dec.bool().map_err(map_decode_err)?),
        Type::U8 => CborValue::Int(dec.u8().map_err(map_decode_err)? as i128),
        Type::U16 => CborValue::Int(dec.u16().map_err(map_decode_err)? as i128),
        Type::U32 => CborValue::Int(dec.u32().map_err(map_decode_err)? as i128),
        Type::U64 => CborValue::Int(dec.u64().map_err(map_decode_err)? as i128),
        Type::I8 => CborValue::Int(dec.i8().map_err(map_decode_err)? as i128),
        Type::I16 => CborValue::Int(dec.i16().map_err(map_decode_err)? as i128),
        Type::I32 => CborValue::Int(dec.i32().map_err(map_decode_err)? as i128),
        Type::I64 => CborValue::Int(dec.i64().map_err(map_decode_err)? as i128),
        Type::Int => {
            let i: i128 = dec.int().map_err(map_decode_err)?.into();
            CborValue::Int(i)
        }
        Type::F16 => CborValue::Float(dec.f16().map_err(map_decode_err)? as f64),
        Type::F32 => CborValue::Float(dec.f32().map_err(map_decode_err)? as f64),
        Type::F64 => CborValue::Float(dec.f64().map_err(map_decode_err)?),
        Type::Bytes => {
            let b = dec.bytes().map_err(map_decode_err)?;
            CborValue::Bytes(b.to_vec())
        }
        Type::BytesIndef => CborValue::Bytes(decode_bytes_indef(dec)?),
        Type::String => {
            let s = dec.str().map_err(map_decode_err)?;
            CborValue::String(s.to_string())
        }
        Type::StringIndef => CborValue::String(decode_string_indef(dec)?),
        Type::Array | Type::ArrayIndef => decode_array(dec, opts, depth)?,
        Type::Map | Type::MapIndef => decode_map(dec, opts, depth)?,
        Type::Tag => {
            let tag = dec.tag().map_err(map_decode_err)?.as_u64();
            let inner = decode_value(dec, opts, depth + 1)?;
            if opts.tag_hook {
                apply_tag_hook(tag, inner)?
            } else {
                CborValue::Tag(tag, Box::new(inner))
            }
        }
        Type::Simple => {
            let s = dec.simple().map_err(map_decode_err)?;
            CborValue::Simple(s)
        }
        Type::Break | Type::Unknown(_) => {
            return Err(CborError::Decode(format!("unexpected CBOR type {ty:?}")));
        }
    };
    Ok(val)
}

fn decode_array(
    dec: &mut Decoder<'_>,
    opts: &DecodeOptions,
    depth: usize,
) -> CborResult<CborValue> {
    let len = dec.array().map_err(map_decode_err)?;
    let mut items = Vec::new();
    match len {
        Some(n) => {
            if n as usize > opts.max_items {
                return Err(CborError::Decode(format!(
                    "array length {n} exceeds max_items {}",
                    opts.max_items
                )));
            }
            for _ in 0..n {
                items.push(decode_value(dec, opts, depth + 1)?);
            }
        }
        None => {
            if !opts.allow_indefinite {
                return Err(CborError::Decode(
                    "indefinite-length array not allowed".into(),
                ));
            }
            loop {
                if dec.datatype().map_err(map_decode_err)? == Type::Break {
                    dec.skip().map_err(map_decode_err)?;
                    break;
                }
                if items.len() >= opts.max_items {
                    return Err(CborError::Decode(format!(
                        "array exceeds max_items {}",
                        opts.max_items
                    )));
                }
                items.push(decode_value(dec, opts, depth + 1)?);
            }
        }
    }
    Ok(CborValue::Array(items))
}

fn decode_map(dec: &mut Decoder<'_>, opts: &DecodeOptions, depth: usize) -> CborResult<CborValue> {
    let len = dec.map().map_err(map_decode_err)?;
    let mut pairs = Vec::new();
    let mut seen = std::collections::HashSet::new();
    match len {
        Some(n) => {
            if n as usize > opts.max_items {
                return Err(CborError::Decode(format!(
                    "map length {n} exceeds max_items {}",
                    opts.max_items
                )));
            }
            for _ in 0..n {
                let k = decode_value(dec, opts, depth + 1)?;
                let v = decode_value(dec, opts, depth + 1)?;
                if opts.reject_duplicate_keys {
                    let key_sig = canonical_key_sig(&k);
                    if !seen.insert(key_sig.clone()) {
                        return Err(CborError::DuplicateKey(key_sig));
                    }
                }
                pairs.push((k, v));
            }
        }
        None => {
            if !opts.allow_indefinite {
                return Err(CborError::Decode(
                    "indefinite-length map not allowed".into(),
                ));
            }
            loop {
                if dec.datatype().map_err(map_decode_err)? == Type::Break {
                    dec.skip().map_err(map_decode_err)?;
                    break;
                }
                if pairs.len() >= opts.max_items {
                    return Err(CborError::Decode(format!(
                        "map exceeds max_items {}",
                        opts.max_items
                    )));
                }
                let k = decode_value(dec, opts, depth + 1)?;
                let v = decode_value(dec, opts, depth + 1)?;
                if opts.reject_duplicate_keys {
                    let key_sig = canonical_key_sig(&k);
                    if !seen.insert(key_sig.clone()) {
                        return Err(CborError::DuplicateKey(key_sig));
                    }
                }
                pairs.push((k, v));
            }
        }
    }
    Ok(CborValue::Map(pairs))
}

fn decode_bytes_indef(dec: &mut Decoder<'_>) -> CborResult<Vec<u8>> {
    let mut out = Vec::new();
    let mut iter = dec.bytes_iter().map_err(map_decode_err)?;
    while let Some(chunk) = iter.next().transpose().map_err(map_decode_err)? {
        out.extend_from_slice(chunk);
    }
    Ok(out)
}

fn decode_string_indef(dec: &mut Decoder<'_>) -> CborResult<String> {
    let mut out = String::new();
    let mut iter = dec.str_iter().map_err(map_decode_err)?;
    while let Some(chunk) = iter.next().transpose().map_err(map_decode_err)? {
        out.push_str(chunk);
    }
    Ok(out)
}

fn canonical_key_sig(k: &CborValue) -> String {
    match k {
        CborValue::String(s) => format!("s:{s}"),
        CborValue::Int(n) => format!("i:{n}"),
        CborValue::Bool(b) => format!("b:{b}"),
        CborValue::Bytes(b) => format!("y:{}", hex::encode(b)),
        other => format!("o:{other:?}"),
    }
}

fn apply_tag_hook(tag: u64, inner: CborValue) -> CborResult<CborValue> {
    match tag {
        tags::DATETIME_STRING => match inner {
            CborValue::String(s) => {
                if !looks_like_datetime(&s) {
                    return Err(CborError::InvalidDatetime(s));
                }
                Ok(CborValue::String(s))
            }
            other => Err(CborError::InvalidTag {
                tag,
                reason: format!("expected text, got {other:?}"),
            }),
        },
        tags::DATETIME_EPOCH => match inner {
            CborValue::Int(n) => Ok(CborValue::Float(n as f64)),
            CborValue::Float(f) => Ok(CborValue::Float(f)),
            other => Err(CborError::InvalidTag {
                tag,
                reason: format!("expected number, got {other:?}"),
            }),
        },
        tags::BIGNUM_POS | tags::BIGNUM_NEG => match inner {
            CborValue::Bytes(ref b) => bigint_from_tagged_bytes(tag, b)
                .map(|n| {
                    if let Some(i) = n.to_i64() {
                        CborValue::Int(i as i128)
                    } else {
                        CborValue::BigInt(n)
                    }
                })
                .ok_or_else(|| CborError::InvalidTag {
                    tag,
                    reason: "empty bignum".into(),
                }),
            other => Err(CborError::InvalidTag {
                tag,
                reason: format!("expected bytes, got {other:?}"),
            }),
        },
        tags::DECIMAL_FRACTION => match inner {
            CborValue::Array(items) if items.len() == 2 => {
                let exp = items[0]
                    .as_i64()
                    .ok_or_else(|| CborError::InvalidDecimal("exponent must be integer".into()))?;
                let mant = items[1]
                    .as_i64()
                    .ok_or_else(|| CborError::InvalidDecimal("mantissa must be integer".into()))?;
                let f = mant as f64 * 10f64.powi(exp as i32);
                Ok(CborValue::Float(f))
            }
            other => Err(CborError::InvalidDecimal(format!(
                "expected [exp, mant], got {other:?}"
            ))),
        },
        tags::UUID => match inner {
            CborValue::Bytes(ref b) if b.len() == 16 => Ok(CborValue::String(format_uuid(b))),
            CborValue::Bytes(ref b) => Err(CborError::InvalidUuid(format!(
                "expected 16 bytes, got {}",
                b.len()
            ))),
            other => Err(CborError::InvalidTag {
                tag,
                reason: format!("expected bytes, got {other:?}"),
            }),
        },
        tags::SELF_DESCRIBE => Ok(inner),
        _ => Ok(CborValue::Tag(tag, Box::new(inner))),
    }
}

fn looks_like_datetime(s: &str) -> bool {
    s.len() >= 10 && s.as_bytes().get(4) == Some(&b'-') && s.as_bytes().get(7) == Some(&b'-')
}

fn format_uuid(b: &[u8]) -> String {
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13],
        b[14], b[15]
    )
}

fn map_decode_err(e: DecodeError) -> CborError {
    CborError::Decode(e.to_string())
}

impl CborValue {
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Int(n) => i64::try_from(*n).ok(),
            _ => None,
        }
    }
}

mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut s = String::with_capacity(bytes.len() * 2);
        for &b in bytes {
            s.push(HEX[(b >> 4) as usize] as char);
            s.push(HEX[(b & 0xf) as usize] as char);
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode::encode;

    #[test]
    fn roundtrip_map() {
        let val = CborValue::Map(vec![
            (CborValue::String("a".into()), CborValue::Int(1)),
            (CborValue::String("b".into()), CborValue::Int(2)),
        ]);
        let bytes = encode(&val, &crate::EncodeOptions::default()).unwrap();
        let back = decode(&bytes, &DecodeOptions::default()).unwrap();
        assert_eq!(val, back);
    }

    #[test]
    fn datetime_tag0() {
        let tagged = CborValue::Tag(
            tags::DATETIME_STRING,
            Box::new(CborValue::String("2020-01-02T03:04:05Z".into())),
        );
        let bytes = encode(&tagged, &crate::EncodeOptions::default()).unwrap();
        let back = decode(&bytes, &DecodeOptions::default()).unwrap();
        assert!(matches!(back, CborValue::String(_)));
    }
}
