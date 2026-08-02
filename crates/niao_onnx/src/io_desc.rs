use tract_onnx::prelude::DatumType;
use tract_onnx::prelude::TDim;
use tract_onnx::tract_hir::internal::DimLike;

/// One ONNX graph input or output descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IoDesc {
    pub name: String,
    /// `None` marks a dynamic axis.
    pub shape: Vec<Option<usize>>,
    pub dtype: String,
}

impl IoDesc {
    pub fn element_count(&self) -> Option<usize> {
        let mut n = 1usize;
        for d in &self.shape {
            let d = (*d)?;
            n = n.checked_mul(d)?;
        }
        Some(n)
    }

    pub fn shape_display(&self) -> String {
        let parts: Vec<String> = self
            .shape
            .iter()
            .map(|d| d.map(|n| n.to_string()).unwrap_or_else(|| "?".into()))
            .collect();
        format!("[{}]", parts.join(", "))
    }
}

pub fn dtype_name(dt: DatumType) -> String {
    match dt {
        DatumType::F32 => "float32".into(),
        DatumType::F64 => "float64".into(),
        DatumType::I8 => "int8".into(),
        DatumType::I16 => "int16".into(),
        DatumType::I32 => "int32".into(),
        DatumType::I64 => "int64".into(),
        DatumType::U8 => "uint8".into(),
        DatumType::U16 => "uint16".into(),
        DatumType::U32 => "uint32".into(),
        DatumType::U64 => "uint64".into(),
        DatumType::Bool => "bool".into(),
        _ => format!("{dt:?}"),
    }
}

pub fn shape_from_fact(shape: &[TDim]) -> Vec<Option<usize>> {
    shape.iter().map(|d| d.to_usize().ok()).collect()
}

pub fn shape_from_concrete(shape: &[usize]) -> Vec<Option<usize>> {
    shape.iter().map(|d| Some(*d)).collect()
}
