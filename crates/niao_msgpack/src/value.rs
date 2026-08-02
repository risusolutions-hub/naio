use niao_bignum::BigInt;
use std::collections::HashMap;

/// In-memory MessagePack value used by the Niao bridge.
#[derive(Debug, Clone, PartialEq)]
pub enum MsgValue {
    Nil,
    Bool(bool),
    Int(i64),
    Uint(u64),
    BigInt(BigInt),
    Float(f64),
    String(String),
    Binary(Vec<u8>),
    Array(Vec<MsgValue>),
    Map(Vec<(MsgValue, MsgValue)>),
    Ext { code: i8, data: Vec<u8> },
    Timestamp { sec: i64, nsec: u32 },
}

impl MsgValue {
    pub fn ext(code: i8, data: Vec<u8>) -> Self {
        Self::Ext { code, data }
    }

    pub fn timestamp(sec: i64, nsec: u32) -> Self {
        Self::Timestamp { sec, nsec }
    }

    pub fn is_ext(&self) -> bool {
        matches!(self, Self::Ext { .. })
    }

    pub fn is_timestamp(&self) -> bool {
        matches!(self, Self::Timestamp { .. })
    }
}

/// Niao-facing extension wrapper `{code, data}`.
pub fn ext_object(code: i8, data: Vec<u8>) -> HashMap<String, MsgValue> {
    let mut map = HashMap::new();
    map.insert("code".into(), MsgValue::Int(code as i64));
    map.insert("data".into(), MsgValue::Binary(data));
    map
}

/// Niao-facing timestamp wrapper `{sec, nsec}`.
pub fn timestamp_object(sec: i64, nsec: u32) -> HashMap<String, MsgValue> {
    let mut map = HashMap::new();
    map.insert("sec".into(), MsgValue::Int(sec));
    map.insert("nsec".into(), MsgValue::Int(nsec as i64));
    map
}
