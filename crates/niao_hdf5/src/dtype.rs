//! HDF5 dtype parsing and introspection.

use crate::error::{Hdf5Error, Hdf5Result};
use hdf5_metno::types::{FloatSize, IntSize, TypeDescriptor};
use hdf5_metno::Datatype;

/// Supported HDF5 dtype names for create/write operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DType {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    Bool,
    String,
}

impl DType {
    pub fn parse(name: &str) -> Hdf5Result<Self> {
        match name.to_ascii_lowercase().as_str() {
            "i8" | "int8" => Ok(DType::I8),
            "i16" | "int16" | "short" => Ok(DType::I16),
            "i32" | "int32" | "int" => Ok(DType::I32),
            "i64" | "int64" | "long" => Ok(DType::I64),
            "u8" | "uint8" | "byte" => Ok(DType::U8),
            "u16" | "uint16" => Ok(DType::U16),
            "u32" | "uint32" => Ok(DType::U32),
            "u64" | "uint64" => Ok(DType::U64),
            "f32" | "float32" | "float" => Ok(DType::F32),
            "f64" | "float64" | "double" => Ok(DType::F64),
            "bool" | "boolean" => Ok(DType::Bool),
            "string" | "str" | "utf8" => Ok(DType::String),
            other => Err(Hdf5Error::InvalidDtype(format!(
                "unsupported dtype '{other}'; use i8..u64, f32, f64, bool, string"
            ))),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            DType::I8 => "i8",
            DType::I16 => "i16",
            DType::I32 => "i32",
            DType::I64 => "i64",
            DType::U8 => "u8",
            DType::U16 => "u16",
            DType::U32 => "u32",
            DType::U64 => "u64",
            DType::F32 => "f32",
            DType::F64 => "f64",
            DType::Bool => "bool",
            DType::String => "string",
        }
    }
}

pub fn dtype_name(dt: &Datatype) -> Hdf5Result<String> {
    let desc = dt.to_descriptor()?;
    Ok(match desc {
        TypeDescriptor::Integer(IntSize::U1) => "i8".into(),
        TypeDescriptor::Integer(IntSize::U2) => "i16".into(),
        TypeDescriptor::Integer(IntSize::U4) => "i32".into(),
        TypeDescriptor::Integer(IntSize::U8) => "i64".into(),
        TypeDescriptor::Unsigned(IntSize::U1) => "u8".into(),
        TypeDescriptor::Unsigned(IntSize::U2) => "u16".into(),
        TypeDescriptor::Unsigned(IntSize::U4) => "u32".into(),
        TypeDescriptor::Unsigned(IntSize::U8) => "u64".into(),
        TypeDescriptor::Float(FloatSize::U4) => "f32".into(),
        TypeDescriptor::Float(FloatSize::U8) => "f64".into(),
        TypeDescriptor::Boolean => "bool".into(),
        TypeDescriptor::VarLenUnicode
        | TypeDescriptor::VarLenAscii
        | TypeDescriptor::FixedUnicode(_)
        | TypeDescriptor::FixedAscii(_) => "string".into(),
        other => format!("{other}"),
    })
}
