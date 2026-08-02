//! Dataset create, read, write, and reshape.

use crate::data::{read_dataset_values, write_dataset_values, DynData, SliceSpec};
use crate::dtype::DType;
use crate::error::{Hdf5Error, Hdf5Result};
use crate::location::{open_dataset, resolve_group};
use hdf5_metno::file::File;
use hdf5_metno::types::VarLenUnicode;
use hdf5_metno::Dataset;

/// Options for dataset creation.
#[derive(Debug, Clone, Default)]
pub struct CreateOpts {
    pub dtype: Option<String>,
    pub chunk: Option<Vec<usize>>,
    pub deflate: Option<u8>,
    pub shuffle: bool,
    pub fill_value: Option<f64>,
}

/// Create a new dataset at `path` with `shape`.
pub fn create_dataset(
    file: &File,
    path: &str,
    shape: &[usize],
    opts: &CreateOpts,
) -> Hdf5Result<Dataset> {
    if path.is_empty() {
        return Err(Hdf5Error::InvalidShape(
            "dataset path cannot be empty".into(),
        ));
    }
    let dtype = DType::parse(opts.dtype.as_deref().unwrap_or("f64"))?;
    let parent_path = path.rfind('/').map(|i| &path[..i]).unwrap_or("");
    let name = path.rfind('/').map(|i| &path[i + 1..]).unwrap_or(path);
    let loc = resolve_group(file, parent_path)?;

    macro_rules! create_typed {
        ($t:ty, $dt:expr) => {{
            let mut b = loc.new_dataset::<$t>().shape(shape);
            if let Some(ch) = &opts.chunk {
                b = b.chunk(ch.as_slice());
            }
            if opts.shuffle {
                b = b.shuffle();
            }
            if let Some(level) = opts.deflate {
                b = b.deflate(level);
            }
            if let Some(fv) = opts.fill_value {
                b = b.fill_value(fv);
            }
            b.create(name).map_err(Hdf5Error::from)
        }};
    }

    match dtype {
        DType::I8 => create_typed!(i8, dtype),
        DType::I16 => create_typed!(i16, dtype),
        DType::I32 => create_typed!(i32, dtype),
        DType::I64 => create_typed!(i64, dtype),
        DType::U8 => create_typed!(u8, dtype),
        DType::U16 => create_typed!(u16, dtype),
        DType::U32 => create_typed!(u32, dtype),
        DType::U64 => create_typed!(u64, dtype),
        DType::F32 => create_typed!(f32, dtype),
        DType::F64 => create_typed!(f64, dtype),
        DType::Bool => create_typed!(bool, dtype),
        DType::String => {
            let mut b = loc.new_dataset::<VarLenUnicode>().shape(shape);
            if let Some(ch) = &opts.chunk {
                b = b.chunk(ch.as_slice());
            }
            if opts.shuffle {
                b = b.shuffle();
            }
            if let Some(level) = opts.deflate {
                b = b.deflate(level);
            }
            b.create(name).map_err(Hdf5Error::from)
        }
    }
}

/// Read full dataset or hyperslab.
pub fn read_dataset(ds: &Dataset, slice: Option<&SliceSpec>) -> Hdf5Result<DynData> {
    read_dataset_values(ds, slice)
}

/// Write full dataset or hyperslab.
pub fn write_dataset(ds: &Dataset, data: &DynData, slice: Option<&SliceSpec>) -> Hdf5Result<()> {
    if ds.file()?.is_read_only() {
        return Err(Hdf5Error::ReadOnly("file opened read-only".into()));
    }
    write_dataset_values(ds, data, slice)
}

/// Dataset shape as usize vector.
pub fn dataset_shape(ds: &Dataset) -> Vec<i64> {
    ds.shape().into_iter().map(|d| d as i64).collect()
}

/// Dataset dtype name.
pub fn dataset_dtype(ds: &Dataset) -> Hdf5Result<String> {
    crate::dtype::dtype_name(&ds.dtype()?)
}

/// Resize extensible dataset.
pub fn resize_dataset(ds: &Dataset, shape: &[usize]) -> Hdf5Result<()> {
    if ds.file()?.is_read_only() {
        return Err(Hdf5Error::ReadOnly("file opened read-only".into()));
    }
    ds.resize(shape)?;
    Ok(())
}

/// Open dataset by file + path.
pub fn dataset(file: &File, path: &str) -> Hdf5Result<Dataset> {
    open_dataset(file, path)
}

/// Infer element count from shape.
pub fn num_elements(shape: &[usize]) -> usize {
    shape.iter().product()
}

/// Check stored dtype matches DynData kind.
pub fn validate_write_dtype(ds: &Dataset, data: &DynData) -> Hdf5Result<()> {
    use hdf5_metno::types::TypeDescriptor;
    let desc = ds.dtype()?.to_descriptor()?;
    let ok = match (data, desc) {
        (DynData::I64(_), TypeDescriptor::Integer(_) | TypeDescriptor::Unsigned(_)) => true,
        (DynData::F64(_), TypeDescriptor::Float(_)) => true,
        (DynData::Bool(_), TypeDescriptor::Boolean) => true,
        (DynData::String(_), TypeDescriptor::VarLenUnicode | TypeDescriptor::VarLenAscii) => true,
        (DynData::Nested(_, _), _) => true,
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err(Hdf5Error::TypeMismatch(
            "data type does not match dataset dtype".into(),
        ))
    }
}
