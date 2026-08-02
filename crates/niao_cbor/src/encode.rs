//! CBOR encoder with canonical (COSE-friendly) mode.

use crate::error::{CborError, CborResult};
use crate::tags;
use crate::value::CborValue;
use crate::EncodeOptions;
use minicbor::data::Tag;
use minicbor::encode::{Encoder, Write};
use niao_bignum::BigInt;

pub fn encode(value: &CborValue, opts: &EncodeOptions) -> CborResult<Vec<u8>> {
    let mut buf = Vec::new();
    encode_into(value, opts, &mut buf)?;
    if buf.len() > opts.max_bytes {
        return Err(CborError::TooLarge(buf.len()));
    }
    Ok(buf)
}

pub fn encode_into<W: Write>(value: &CborValue, opts: &EncodeOptions, w: &mut W) -> CborResult<()> {
    let mut enc = Encoder::new(w);
    encode_value(&mut enc, value, opts, 0)?;
    Ok(())
}

fn step<W: Write>(r: Result<&mut Encoder<W>, minicbor::encode::Error<W::Error>>) -> CborResult<()> {
    r.map_err(|_| CborError::Encode("CBOR encode error".into()))?;
    Ok(())
}

fn encode_value<W: Write>(
    enc: &mut Encoder<W>,
    value: &CborValue,
    opts: &EncodeOptions,
    depth: usize,
) -> CborResult<()> {
    if depth > opts.max_depth {
        return Err(CborError::DepthExceeded {
            max: opts.max_depth,
        });
    }
    match value {
        CborValue::Null => step(enc.null()),
        CborValue::Undefined => step(enc.undefined()),
        CborValue::Bool(b) => step(enc.bool(*b)),
        CborValue::Int(n) => encode_int(enc, *n, opts),
        CborValue::BigInt(n) => encode_bigint(enc, n),
        CborValue::Float(f) => encode_float(enc, *f, opts),
        CborValue::Bytes(b) => encode_bytes(enc, b, opts),
        CborValue::String(s) => encode_text(enc, s, opts),
        CborValue::Array(items) => encode_array(enc, items, opts, depth),
        CborValue::Map(pairs) => encode_map(enc, pairs, opts, depth),
        CborValue::Tag(tag, inner) => {
            step(enc.tag(Tag::new(*tag)))?;
            encode_value(enc, inner, opts, depth + 1)
        }
        CborValue::Simple(n) => step(enc.simple(*n)),
    }
}

fn encode_int<W: Write>(enc: &mut Encoder<W>, n: i128, opts: &EncodeOptions) -> CborResult<()> {
    if opts.canonical && (n < -0x8000_0000_0000_0000 || n > 0xFFFF_FFFF_FFFF_FFFF) {
        return encode_bignum(enc, n);
    }
    if n >= 0 {
        if n <= u8::MAX as i128 {
            step(enc.u8(n as u8))
        } else if n <= u16::MAX as i128 {
            step(enc.u16(n as u16))
        } else if n <= u32::MAX as i128 {
            step(enc.u32(n as u32))
        } else if n <= u64::MAX as i128 {
            step(enc.u64(n as u64))
        } else {
            encode_bignum(enc, n)
        }
    } else if n >= i64::MIN as i128 {
        step(enc.i64(n as i64))
    } else {
        encode_bignum(enc, n)
    }
}

fn encode_bigint<W: Write>(enc: &mut Encoder<W>, n: &BigInt) -> CborResult<()> {
    if n.sign() == niao_bignum::Sign::Plus || n.is_zero() {
        let (_, bytes) = n.to_radix_be(256);
        step(enc.tag(Tag::new(tags::BIGNUM_POS)))?;
        step(enc.bytes(&bytes))
    } else {
        let mag_bi = -BigInt::from(1) - n.clone();
        let (_, mag) = mag_bi.to_radix_be(256);
        step(enc.tag(Tag::new(tags::BIGNUM_NEG)))?;
        step(enc.bytes(&mag))
    }
}

fn encode_bignum<W: Write>(enc: &mut Encoder<W>, n: i128) -> CborResult<()> {
    encode_bigint(enc, &BigInt::from_i128(n))
}

fn encode_float<W: Write>(enc: &mut Encoder<W>, f: f64, opts: &EncodeOptions) -> CborResult<()> {
    if opts.fractional_floats {
        let s = format!("{f:.15e}");
        let parts: Vec<&str> = s.split('e').collect();
        if parts.len() == 2 {
            let mant = parts[0].replace('.', "").parse::<i64>().unwrap_or(0);
            let exp = parts[1].parse::<i32>().unwrap_or(0) - (parts[0].len() as i32 - 1);
            step(enc.tag(Tag::new(tags::BIGFLOAT)))?;
            step(enc.array(2))?;
            step(enc.i32(exp))?;
            return step(enc.i64(mant));
        }
    }
    if opts.canonical {
        let f32v = f as f32;
        if f64::from(f32v) == f {
            return step(enc.f32(f32v));
        }
    }
    step(enc.f64(f))
}

fn encode_bytes<W: Write>(enc: &mut Encoder<W>, b: &[u8], opts: &EncodeOptions) -> CborResult<()> {
    if opts.indefinite_length && b.len() > 256 {
        step(enc.begin_bytes())?;
        for chunk in b.chunks(256) {
            step(enc.bytes(chunk))?;
        }
        step(enc.end())
    } else {
        step(enc.bytes(b))
    }
}

fn encode_text<W: Write>(enc: &mut Encoder<W>, s: &str, opts: &EncodeOptions) -> CborResult<()> {
    if opts.auto_datetime_tag && looks_like_datetime(s) && !opts.datetime_timestamp {
        step(enc.tag(Tag::new(tags::DATETIME_STRING)))?;
        return step(enc.str(s));
    }
    if opts.indefinite_length && s.len() > 256 {
        step(enc.begin_str())?;
        for chunk in s.as_bytes().chunks(256) {
            let piece = std::str::from_utf8(chunk).map_err(|e| CborError::Encode(e.to_string()))?;
            step(enc.str(piece))?;
        }
        step(enc.end())
    } else {
        step(enc.str(s))
    }
}

fn encode_array<W: Write>(
    enc: &mut Encoder<W>,
    items: &[CborValue],
    opts: &EncodeOptions,
    depth: usize,
) -> CborResult<()> {
    if opts.indefinite_length {
        step(enc.begin_array())?;
        for item in items {
            encode_value(enc, item, opts, depth + 1)?;
        }
        step(enc.end())
    } else {
        step(enc.array(items.len() as u64))?;
        for item in items {
            encode_value(enc, item, opts, depth + 1)?;
        }
        Ok(())
    }
}

fn encode_map<W: Write>(
    enc: &mut Encoder<W>,
    pairs: &[(CborValue, CborValue)],
    opts: &EncodeOptions,
    depth: usize,
) -> CborResult<()> {
    let mut ordered: Vec<(usize, &CborValue, &CborValue)> = pairs
        .iter()
        .enumerate()
        .map(|(i, (k, v))| (i, k, v))
        .collect();
    if opts.canonical || opts.sort_keys {
        ordered.sort_by(|a, b| canonical_key_cmp(a.1, b.1).then(a.0.cmp(&b.0)));
    }
    if opts.indefinite_length {
        step(enc.begin_map())?;
        for (_, k, v) in &ordered {
            encode_value(enc, k, opts, depth + 1)?;
            encode_value(enc, v, opts, depth + 1)?;
        }
        step(enc.end())
    } else {
        step(enc.map(ordered.len() as u64))?;
        for (_, k, v) in &ordered {
            encode_value(enc, k, opts, depth + 1)?;
            encode_value(enc, v, opts, depth + 1)?;
        }
        Ok(())
    }
}

fn canonical_key_cmp(a: &CborValue, b: &CborValue) -> std::cmp::Ordering {
    let ea = canonical_key_bytes(a);
    let eb = canonical_key_bytes(b);
    ea.cmp(&eb)
}

fn canonical_key_bytes(k: &CborValue) -> Vec<u8> {
    let mut buf = Vec::new();
    let _ = encode_into(
        k,
        &EncodeOptions {
            canonical: true,
            ..Default::default()
        },
        &mut buf,
    );
    buf
}

fn looks_like_datetime(s: &str) -> bool {
    s.len() >= 10
        && s.as_bytes().get(4) == Some(&b'-')
        && s.as_bytes().get(7) == Some(&b'-')
        && (s.contains('T') || s.contains('t'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::decode;

    #[test]
    fn canonical_sorts_keys() {
        let val = CborValue::Map(vec![
            (CborValue::String("z".into()), CborValue::Int(1)),
            (CborValue::String("a".into()), CborValue::Int(2)),
        ]);
        let bytes = encode(
            &val,
            &EncodeOptions {
                canonical: true,
                ..Default::default()
            },
        )
        .unwrap();
        let back = decode(&bytes, &crate::DecodeOptions::default()).unwrap();
        if let CborValue::Map(pairs) = back {
            assert_eq!(pairs[0].0, CborValue::String("a".into()));
        } else {
            panic!("expected map");
        }
    }
}
