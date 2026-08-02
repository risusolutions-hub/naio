use crate::error::{ProtoError, ProtoResult};

/// A single field parsed from raw protobuf wire bytes.
#[derive(Debug, Clone)]
pub struct RawField {
    pub field_number: u32,
    pub wire_type: u8,
    pub wire_name: String,
    pub value: RawValue,
}

#[derive(Debug, Clone)]
pub enum RawValue {
    Varint(u64),
    Fixed32(u32),
    Fixed64(u64),
    LengthDelimited(Vec<u8>),
}

const WIRE_VARINT: u8 = 0;
const WIRE_FIXED64: u8 = 1;
const WIRE_LENGTH: u8 = 2;
const WIRE_FIXED32: u8 = 5;

/// Decode protobuf wire format without a schema (debug / introspection).
pub fn decode_raw(data: &[u8]) -> ProtoResult<Vec<RawField>> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < data.len() {
        let (tag, next) = read_varint(data, pos).map_err(ProtoError::Decode)?;
        pos = next;
        let field_number = (tag >> 3) as u32;
        let wire_type = (tag & 0x7) as u8;
        let (value, next) = read_value(data, pos, wire_type).map_err(ProtoError::Decode)?;
        pos = next;
        out.push(RawField {
            field_number,
            wire_type,
            wire_name: wire_type_name(wire_type).to_string(),
            value,
        });
    }
    Ok(out)
}

/// Encode a field tag (field number + wire type) as a varint.
pub fn encode_tag(field_number: u32, wire_type: u8) -> Vec<u8> {
    let tag = ((field_number as u64) << 3) | (wire_type as u64);
    encode_varint(tag)
}

pub fn encode_varint(mut n: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(10);
    while n >= 0x80 {
        out.push((n as u8) | 0x80);
        n >>= 7;
    }
    out.push(n as u8);
    out
}

pub fn decode_varint(data: &[u8], offset: usize) -> ProtoResult<(u64, usize)> {
    read_varint(data, offset).map_err(ProtoError::Decode)
}

pub fn wire_type_name(wire_type: u8) -> &'static str {
    match wire_type {
        WIRE_VARINT => "varint",
        WIRE_FIXED64 => "fixed64",
        WIRE_LENGTH => "length_delimited",
        WIRE_FIXED32 => "fixed32",
        _ => "unknown",
    }
}

fn read_varint(data: &[u8], mut pos: usize) -> Result<(u64, usize), String> {
    let mut result = 0u64;
    let mut shift = 0u32;
    while pos < data.len() {
        let byte = data[pos];
        pos += 1;
        result |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Ok((result, pos));
        }
        shift += 7;
        if shift >= 64 {
            return Err("varint overflow".into());
        }
    }
    Err("truncated varint".into())
}

fn read_value(data: &[u8], pos: usize, wire_type: u8) -> Result<(RawValue, usize), String> {
    match wire_type {
        WIRE_VARINT => {
            let (n, next) = read_varint(data, pos)?;
            Ok((RawValue::Varint(n), next))
        }
        WIRE_FIXED64 => {
            if pos + 8 > data.len() {
                return Err("truncated fixed64".into());
            }
            let n = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
            Ok((RawValue::Fixed64(n), pos + 8))
        }
        WIRE_LENGTH => {
            let (len, next) = read_varint(data, pos)?;
            let len = len as usize;
            if next + len > data.len() {
                return Err("truncated length-delimited field".into());
            }
            let slice = data[next..next + len].to_vec();
            Ok((RawValue::LengthDelimited(slice), next + len))
        }
        WIRE_FIXED32 => {
            if pos + 4 > data.len() {
                return Err("truncated fixed32".into());
            }
            let n = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap());
            Ok((RawValue::Fixed32(n), pos + 4))
        }
        other => Err(format!("unsupported wire type {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_roundtrip() {
        let tag = encode_tag(3, WIRE_VARINT);
        let (n, _) = decode_varint(&tag, 0).unwrap();
        assert_eq!(n, (3 << 3) | 0);
    }
}
