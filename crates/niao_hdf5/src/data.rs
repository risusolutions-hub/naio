//! Zero-copy-friendly data transfer between HDF5 and host buffers.

use crate::error::{Hdf5Error, Hdf5Result};
use hdf5_metno::types::{TypeDescriptor, VarLenUnicode};
use hdf5_metno::{Attribute, Dataset, Group, Hyperslab, Selection};
use ndarray::Array2;
use std::str::FromStr;

/// Hyperslab selection for partial I/O.
#[derive(Debug, Clone, Default)]
pub struct SliceSpec {
    pub start: Vec<usize>,
    pub count: Vec<usize>,
    pub stride: Option<Vec<usize>>,
}

impl SliceSpec {
    pub fn from_parts(
        start: Vec<usize>,
        count: Vec<usize>,
        stride: Option<Vec<usize>>,
    ) -> Hdf5Result<Self> {
        if start.len() != count.len() {
            return Err(Hdf5Error::InvalidShape(
                "slice start and count must have same rank".into(),
            ));
        }
        if let Some(st) = &stride {
            if st.len() != start.len() {
                return Err(Hdf5Error::InvalidShape(
                    "slice stride rank must match start".into(),
                ));
            }
            if st.iter().any(|&x| x == 0) {
                return Err(Hdf5Error::InvalidShape("slice stride must be >= 1".into()));
            }
        }
        Ok(Self {
            start,
            count,
            stride,
        })
    }

    fn selection(&self, ndim: usize) -> Hdf5Result<Selection> {
        if self.start.len() != ndim {
            return Err(Hdf5Error::InvalidShape(format!(
                "slice rank {} != dataset ndim {ndim}",
                self.start.len()
            )));
        }
        if self
            .stride
            .as_ref()
            .is_some_and(|s| s.iter().any(|&x| x != 1))
        {
            return Err(Hdf5Error::InvalidShape(
                "stride != 1 not supported yet; use contiguous slices".into(),
            ));
        }
        match ndim {
            1 => {
                let end = self.start[0] + self.count[0];
                Ok(Hyperslab::from(self.start[0]..end).into())
            }
            2 => {
                let e0 = self.start[0] + self.count[0];
                let e1 = self.start[1] + self.count[1];
                Ok(Hyperslab::from((self.start[0]..e0, self.start[1]..e1)).into())
            }
            _ => Err(Hdf5Error::InvalidShape(
                "slice selection supports rank 1-2; use full read for higher rank".into(),
            )),
        }
    }
}

/// Dynamic host-side array payload.
#[derive(Debug, Clone)]
pub enum DynData {
    I64(Vec<i64>),
    F64(Vec<f64>),
    Bool(Vec<u8>),
    String(Vec<String>),
    Nested(Vec<DynData>, Vec<i64>),
}

impl DynData {
    pub fn len(&self) -> usize {
        match self {
            DynData::I64(v) => v.len(),
            DynData::F64(v) => v.len(),
            DynData::Bool(v) => v.len(),
            DynData::String(v) => v.len(),
            DynData::Nested(v, _) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub fn read_dataset_values(ds: &Dataset, slice: Option<&SliceSpec>) -> Hdf5Result<DynData> {
    let desc = ds.dtype()?.to_descriptor()?;
    match desc {
        TypeDescriptor::Integer(_) => read_int_dataset(ds, slice),
        TypeDescriptor::Unsigned(_) => read_uint_dataset(ds, slice),
        TypeDescriptor::Float(_) => read_f64_dataset(ds, slice),
        TypeDescriptor::Boolean => read_bool_dataset(ds, slice),
        TypeDescriptor::VarLenUnicode
        | TypeDescriptor::VarLenAscii
        | TypeDescriptor::FixedUnicode(_)
        | TypeDescriptor::FixedAscii(_) => read_string_dataset(ds, slice),
        _ => Err(Hdf5Error::TypeMismatch(format!(
            "unsupported dataset dtype: {desc}"
        ))),
    }
}

pub fn write_dataset_values(
    ds: &Dataset,
    data: &DynData,
    slice: Option<&SliceSpec>,
) -> Hdf5Result<()> {
    match data {
        DynData::I64(v) => write_i64_dataset(ds, v, slice),
        DynData::F64(v) => write_f64_dataset(ds, v, slice),
        DynData::Bool(v) => {
            let bools: Vec<bool> = v.iter().map(|b| *b != 0).collect();
            write_bool_dataset(ds, &bools, slice)
        }
        DynData::String(v) => write_string_dataset(ds, v, slice),
        DynData::Nested(_, _) => Err(Hdf5Error::TypeMismatch(
            "nested array write not supported; pass flat IntArray/FloatArray".into(),
        )),
    }
}

pub fn read_attr_values(attr: &Attribute) -> Hdf5Result<DynData> {
    let desc = attr.dtype()?.to_descriptor()?;
    match desc {
        TypeDescriptor::Integer(_) => {
            if attr.is_scalar() {
                Ok(DynData::I64(vec![attr.read_scalar::<i64>()?]))
            } else {
                Ok(DynData::I64(attr.read_raw()?))
            }
        }
        TypeDescriptor::Float(_) => {
            if attr.is_scalar() {
                Ok(DynData::F64(vec![attr.read_scalar::<f64>()?]))
            } else {
                Ok(DynData::F64(attr.read_raw()?))
            }
        }
        TypeDescriptor::Boolean => {
            if attr.is_scalar() {
                Ok(DynData::Bool(vec![u8::from(attr.read_scalar::<bool>()?)]))
            } else {
                let raw: Vec<bool> = attr.read_raw()?;
                Ok(DynData::Bool(raw.iter().map(|&b| u8::from(b)).collect()))
            }
        }
        TypeDescriptor::VarLenUnicode | TypeDescriptor::VarLenAscii => {
            if attr.is_scalar() {
                let v: VarLenUnicode = attr.read_scalar()?;
                Ok(DynData::String(vec![v.to_string()]))
            } else {
                let raw: Vec<VarLenUnicode> = attr.read_raw()?;
                Ok(DynData::String(raw.iter().map(|s| s.to_string()).collect()))
            }
        }
        _ => Err(Hdf5Error::TypeMismatch(format!(
            "unsupported attribute dtype: {desc}"
        ))),
    }
}

pub fn write_attr_scalar(loc: &Group, name: &str, value: &DynData) -> Hdf5Result<()> {
    match value {
        DynData::I64(v) if v.len() == 1 => {
            loc.new_attr::<i64>()
                .shape(())
                .create(name)?
                .write_scalar(&v[0])?;
        }
        DynData::F64(v) if v.len() == 1 => {
            loc.new_attr::<f64>()
                .shape(())
                .create(name)?
                .write_scalar(&v[0])?;
        }
        DynData::Bool(v) if v.len() == 1 => {
            let b = v[0] != 0;
            loc.new_attr::<bool>()
                .shape(())
                .create(name)?
                .write_scalar(&b)?;
        }
        DynData::String(v) if v.len() == 1 => {
            let s = VarLenUnicode::from_str(&v[0]).map_err(|e| Hdf5Error::H5(e.to_string()))?;
            loc.new_attr::<VarLenUnicode>()
                .shape(())
                .create(name)?
                .write_scalar(&s)?;
        }
        DynData::I64(v) => {
            loc.new_attr::<i64>()
                .shape([v.len()])
                .create(name)?
                .write(v.as_slice())?;
        }
        DynData::F64(v) => {
            loc.new_attr::<f64>()
                .shape([v.len()])
                .create(name)?
                .write(v.as_slice())?;
        }
        DynData::String(v) => {
            let items: Vec<VarLenUnicode> = v
                .iter()
                .map(|s| VarLenUnicode::from_str(s).map_err(|e| Hdf5Error::H5(e.to_string())))
                .collect::<Result<_, _>>()?;
            loc.new_attr::<VarLenUnicode>()
                .shape([items.len()])
                .create(name)?
                .write(items.as_slice())?;
        }
        _ => {
            return Err(Hdf5Error::TypeMismatch(
                "attribute value must be scalar or 1d array".into(),
            ));
        }
    }
    Ok(())
}

fn read_int_dataset(ds: &Dataset, slice: Option<&SliceSpec>) -> Hdf5Result<DynData> {
    if ds.is_scalar() {
        return Ok(DynData::I64(vec![ds.read_scalar::<i64>()?]));
    }
    if let Some(sl) = slice {
        let sel = sl.selection(ds.ndim())?;
        return match ds.ndim() {
            1 => {
                let arr = ds.read_slice_1d::<i64, _>(sel)?;
                Ok(DynData::I64(arr.into_raw_vec_and_offset().0))
            }
            2 => {
                let arr = ds.read_slice_2d::<i64, _>(sel)?;
                Ok(DynData::I64(arr.into_raw_vec_and_offset().0))
            }
            _ => Err(Hdf5Error::InvalidShape("unsupported slice rank".into())),
        };
    }
    Ok(DynData::I64(ds.read_raw()?))
}

fn read_uint_dataset(ds: &Dataset, slice: Option<&SliceSpec>) -> Hdf5Result<DynData> {
    if ds.is_scalar() {
        let v = ds.read_scalar::<u64>()?;
        return Ok(DynData::I64(vec![v as i64]));
    }
    if let Some(sl) = slice {
        let sel = sl.selection(ds.ndim())?;
        return match ds.ndim() {
            1 => {
                let arr = ds.read_slice_1d::<u64, _>(sel)?;
                Ok(DynData::I64(arr.iter().map(|&x| x as i64).collect()))
            }
            2 => {
                let arr = ds.read_slice_2d::<u64, _>(sel)?;
                Ok(DynData::I64(arr.iter().map(|&x| x as i64).collect()))
            }
            _ => Err(Hdf5Error::InvalidShape("unsupported slice rank".into())),
        };
    }
    let raw: Vec<u64> = ds.read_raw()?;
    Ok(DynData::I64(raw.iter().map(|&x| x as i64).collect()))
}

fn read_f64_dataset(ds: &Dataset, slice: Option<&SliceSpec>) -> Hdf5Result<DynData> {
    if ds.is_scalar() {
        return Ok(DynData::F64(vec![ds.read_scalar::<f64>()?]));
    }
    if let Some(sl) = slice {
        let sel = sl.selection(ds.ndim())?;
        return match ds.ndim() {
            1 => {
                let arr = ds.read_slice_1d::<f64, _>(sel)?;
                Ok(DynData::F64(arr.into_raw_vec_and_offset().0))
            }
            2 => {
                let arr = ds.read_slice_2d::<f64, _>(sel)?;
                Ok(DynData::F64(arr.into_raw_vec_and_offset().0))
            }
            _ => Err(Hdf5Error::InvalidShape("unsupported slice rank".into())),
        };
    }
    Ok(DynData::F64(ds.read_raw()?))
}

fn read_bool_dataset(ds: &Dataset, slice: Option<&SliceSpec>) -> Hdf5Result<DynData> {
    if ds.is_scalar() {
        return Ok(DynData::Bool(vec![u8::from(ds.read_scalar::<bool>()?)]));
    }
    if let Some(sl) = slice {
        let sel = sl.selection(ds.ndim())?;
        return match ds.ndim() {
            1 => {
                let arr = ds.read_slice_1d::<bool, _>(sel)?;
                Ok(DynData::Bool(arr.iter().map(|&b| u8::from(b)).collect()))
            }
            2 => {
                let arr = ds.read_slice_2d::<bool, _>(sel)?;
                Ok(DynData::Bool(arr.iter().map(|&b| u8::from(b)).collect()))
            }
            _ => Err(Hdf5Error::InvalidShape("unsupported slice rank".into())),
        };
    }
    let raw: Vec<bool> = ds.read_raw()?;
    Ok(DynData::Bool(raw.iter().map(|&b| u8::from(b)).collect()))
}

fn read_string_dataset(ds: &Dataset, slice: Option<&SliceSpec>) -> Hdf5Result<DynData> {
    if ds.is_scalar() {
        let v: VarLenUnicode = ds.read_scalar()?;
        return Ok(DynData::String(vec![v.to_string()]));
    }
    if slice.is_some() {
        return Err(Hdf5Error::InvalidShape(
            "string slice read: use full read".into(),
        ));
    }
    let raw: Vec<VarLenUnicode> = ds.read_raw()?;
    Ok(DynData::String(raw.iter().map(|s| s.to_string()).collect()))
}

fn write_i64_dataset(ds: &Dataset, data: &[i64], slice: Option<&SliceSpec>) -> Hdf5Result<()> {
    if ds.is_scalar() {
        ds.write_scalar(&data[0])?;
        return Ok(());
    }
    if let Some(sl) = slice {
        let sel = sl.selection(ds.ndim())?;
        if ds.ndim() == 1 {
            ds.write_slice(data, sel)?;
        } else {
            let view = Array2::from_shape_vec((sl.count[0], sl.count[1]), data.to_vec())
                .map_err(|e| Hdf5Error::InvalidShape(e.to_string()))?;
            ds.write_slice(view.view(), sel)?;
        }
    } else if ds.ndim() == 1 {
        ds.write(data)?;
    } else {
        let shape = ds.shape();
        let view = Array2::from_shape_vec((shape[0], shape[1]), data.to_vec())
            .map_err(|e| Hdf5Error::InvalidShape(e.to_string()))?;
        ds.write(view.view())?;
    }
    Ok(())
}

fn write_f64_dataset(ds: &Dataset, data: &[f64], slice: Option<&SliceSpec>) -> Hdf5Result<()> {
    if ds.is_scalar() {
        ds.write_scalar(&data[0])?;
        return Ok(());
    }
    if let Some(sl) = slice {
        let sel = sl.selection(ds.ndim())?;
        if ds.ndim() == 1 {
            ds.write_slice(data, sel)?;
        } else {
            let view = Array2::from_shape_vec((sl.count[0], sl.count[1]), data.to_vec())
                .map_err(|e| Hdf5Error::InvalidShape(e.to_string()))?;
            ds.write_slice(view.view(), sel)?;
        }
    } else if ds.ndim() == 1 {
        ds.write(data)?;
    } else {
        let shape = ds.shape();
        if shape.len() != 2 {
            ds.write_raw(data)?;
        } else {
            let view = Array2::from_shape_vec((shape[0], shape[1]), data.to_vec())
                .map_err(|e| Hdf5Error::InvalidShape(e.to_string()))?;
            ds.write(view.view())?;
        }
    }
    Ok(())
}

fn write_bool_dataset(ds: &Dataset, data: &[bool], slice: Option<&SliceSpec>) -> Hdf5Result<()> {
    if ds.is_scalar() {
        ds.write_scalar(&data[0])?;
        return Ok(());
    }
    if slice.is_some() {
        return Err(Hdf5Error::InvalidShape(
            "bool slice write: use full write".into(),
        ));
    }
    if ds.ndim() == 1 {
        ds.write(data)?;
    } else {
        let shape = ds.shape();
        let view = Array2::from_shape_vec((shape[0], shape[1]), data.to_vec())
            .map_err(|e| Hdf5Error::InvalidShape(e.to_string()))?;
        ds.write(view.view())?;
    }
    Ok(())
}

fn write_string_dataset(
    ds: &Dataset,
    data: &[String],
    slice: Option<&SliceSpec>,
) -> Hdf5Result<()> {
    let items: Vec<VarLenUnicode> = data
        .iter()
        .map(|s| VarLenUnicode::from_str(s).map_err(|e| Hdf5Error::H5(e.to_string())))
        .collect::<Result<_, _>>()?;
    if ds.is_scalar() {
        ds.write_scalar(&items[0])?;
        return Ok(());
    }
    if slice.is_some() {
        return Err(Hdf5Error::InvalidShape(
            "string slice write: use full write".into(),
        ));
    }
    if ds.ndim() == 1 {
        ds.write(items.as_slice())?;
    } else {
        let shape = ds.shape();
        let view = Array2::from_shape_vec((shape[0], shape[1]), items)
            .map_err(|e| Hdf5Error::InvalidShape(e.to_string()))?;
        ds.write(view.view())?;
    }
    Ok(())
}

/// Reshape flat data to nested Niao-style arrays given HDF5 shape.
pub fn nest_data(data: DynData, shape: &[i64]) -> DynData {
    if shape.len() <= 1 {
        return data;
    }
    let size: usize = shape.iter().map(|&d| d as usize).product();
    match data {
        DynData::I64(v) if v.len() == size => nest_flat_i64(v, shape),
        DynData::F64(v) if v.len() == size => nest_flat_f64(v, shape),
        other => other,
    }
}

fn nest_flat_i64(flat: Vec<i64>, shape: &[i64]) -> DynData {
    fn rec(flat: &[i64], shape: &[i64]) -> Vec<DynData> {
        if shape.len() == 1 {
            return flat
                .iter()
                .take(shape[0] as usize)
                .map(|&x| DynData::I64(vec![x]))
                .collect();
        }
        let stride: usize = shape[1..].iter().map(|&d| d as usize).product();
        (0..shape[0] as usize)
            .map(|i| {
                let sub = rec(&flat[i * stride..(i + 1) * stride], &shape[1..]);
                DynData::Nested(sub, shape[1..].to_vec())
            })
            .collect()
    }
    DynData::Nested(rec(&flat, shape), shape.to_vec())
}

fn nest_flat_f64(flat: Vec<f64>, shape: &[i64]) -> DynData {
    fn rec(flat: &[f64], shape: &[i64]) -> Vec<DynData> {
        if shape.len() == 1 {
            return flat
                .iter()
                .take(shape[0] as usize)
                .map(|&x| DynData::F64(vec![x]))
                .collect();
        }
        let stride: usize = shape[1..].iter().map(|&d| d as usize).product();
        (0..shape[0] as usize)
            .map(|i| {
                let sub = rec(&flat[i * stride..(i + 1) * stride], &shape[1..]);
                DynData::Nested(sub, shape[1..].to_vec())
            })
            .collect()
    }
    DynData::Nested(rec(&flat, shape), shape.to_vec())
}

/// Flatten nested host data to a 1d buffer matching shape.
pub fn flatten_data(data: &DynData, shape: &[usize]) -> Hdf5Result<DynData> {
    let need: usize = shape.iter().product();
    let flat = match data {
        DynData::I64(v) => DynData::I64(v.clone()),
        DynData::F64(v) => DynData::F64(v.clone()),
        DynData::Bool(v) => DynData::Bool(v.clone()),
        DynData::String(v) => DynData::String(v.clone()),
        DynData::Nested(items, _) => flatten_nested(items, shape)?,
    };
    if flat.len() != need && !matches!(data, DynData::Nested(_, _)) {
        if shape.len() == 1 {
            return Ok(flat);
        }
        return Err(Hdf5Error::InvalidShape(format!(
            "data length {} != shape product {need}",
            flat.len()
        )));
    }
    Ok(flat)
}

fn flatten_nested(items: &[DynData], shape: &[usize]) -> Hdf5Result<DynData> {
    if shape.is_empty() {
        return Err(Hdf5Error::InvalidShape("empty shape".into()));
    }
    if shape.len() == 1 {
        let mut out_i64 = Vec::with_capacity(shape[0]);
        let mut out_f64 = Vec::with_capacity(shape[0]);
        let mut kind = 0u8;
        for item in items {
            match item {
                DynData::I64(v) if v.len() == 1 => {
                    kind = 1;
                    out_i64.push(v[0]);
                }
                DynData::F64(v) if v.len() == 1 => {
                    kind = 2;
                    out_f64.push(v[0]);
                }
                _ => return Err(Hdf5Error::TypeMismatch("invalid nested leaf".into())),
            }
        }
        return match kind {
            1 => Ok(DynData::I64(out_i64)),
            2 => Ok(DynData::F64(out_f64)),
            _ => Err(Hdf5Error::TypeMismatch("empty nested array".into())),
        };
    }
    let mut flat_i64 = Vec::new();
    let mut flat_f64 = Vec::new();
    let mut kind = 0u8;
    for item in items {
        let sub = flatten_nested(std::slice::from_ref(item), &shape[1..])?;
        match sub {
            DynData::I64(mut v) => {
                kind = 1;
                flat_i64.append(&mut v);
            }
            DynData::F64(mut v) => {
                kind = 2;
                flat_f64.append(&mut v);
            }
            _ => return Err(Hdf5Error::TypeMismatch("nested type mismatch".into())),
        }
    }
    match kind {
        1 => Ok(DynData::I64(flat_i64)),
        2 => Ok(DynData::F64(flat_f64)),
        _ => Err(Hdf5Error::TypeMismatch("empty data".into())),
    }
}
