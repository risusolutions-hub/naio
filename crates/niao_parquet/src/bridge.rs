//! DataFrame ↔ Arrow RecordBatch bridge.

use crate::error::{ParquetError, ParquetResult};
use arrow_array::{
    Array, ArrayRef, BooleanArray, Date32Array, Float64Array, Int64Array, RecordBatch, StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use niao_frame::{ColumnData, DataFrame, Dtype, Series, StringColumn, Validity};
use std::sync::Arc;

pub fn dataframe_to_record_batch(df: &DataFrame) -> ParquetResult<RecordBatch> {
    if df.ncols() == 0 {
        return RecordBatch::try_new(Arc::new(Schema::empty()), vec![])
            .map_err(|e| ParquetError::Arrow(e.to_string()));
    }
    let mut fields = Vec::with_capacity(df.ncols());
    let mut arrays = Vec::with_capacity(df.ncols());
    for col in &df.columns {
        let (field, array) = series_to_arrow(col)?;
        fields.push(field);
        arrays.push(array);
    }
    let schema = Arc::new(Schema::new(fields));
    RecordBatch::try_new(schema, arrays).map_err(|e| ParquetError::Arrow(e.to_string()))
}

pub fn record_batches_to_dataframe(
    batches: &[RecordBatch],
    opts: &crate::options::ReadOptions,
) -> ParquetResult<DataFrame> {
    if batches.is_empty() {
        return Ok(DataFrame::empty());
    }
    let schema = batches[0].schema();
    let col_names: Vec<String> = match &opts.columns {
        Some(cols) => cols.clone(),
        None => schema.fields().iter().map(|f| f.name().clone()).collect(),
    };
    let mut series_map: Vec<(String, Series)> = Vec::with_capacity(col_names.len());
    for name in &col_names {
        let idx = schema
            .fields()
            .iter()
            .position(|f| f.name() == name)
            .ok_or_else(|| {
                ParquetError::Schema(format!("column '{name}' not in parquet schema"))
            })?;
        let field = schema.field(idx);
        let mut parts: Vec<Series> = Vec::with_capacity(batches.len());
        for batch in batches {
            let array = batch.column(idx);
            parts.push(arrow_array_to_series(name, array, field.data_type())?);
        }
        let merged = concat_series(&parts)?;
        series_map.push((name.clone(), merged));
    }
    let columns: Vec<Series> = series_map.into_iter().map(|(_, s)| s).collect();
    let mut df = DataFrame::new(columns).map_err(|e| ParquetError::Shape(e.to_string()))?;
    if let Some(limit) = opts.rows {
        if df.nrows() > limit {
            df = df
                .slice(0, limit)
                .map_err(|e| ParquetError::Shape(e.to_string()))?;
        }
    }
    Ok(df)
}

fn concat_series(parts: &[Series]) -> ParquetResult<Series> {
    if parts.is_empty() {
        return Err(ParquetError::Shape("empty series parts".into()));
    }
    if parts.len() == 1 {
        return Ok(parts[0].clone());
    }
    let name = parts[0].name.clone();
    let dtype = parts[0].dtype();
    let total: usize = parts.iter().map(|s| s.len()).sum();
    let mut validity = Validity::all_valid(total);
    let data = match dtype {
        Dtype::I64 => {
            let mut v = Vec::with_capacity(total);
            let mut off = 0usize;
            for s in parts {
                if let ColumnData::I64(slice) = &s.data {
                    v.extend_from_slice(slice);
                }
                for i in 0..s.len() {
                    if s.validity.is_null(i) {
                        validity.set_null(off + i);
                    }
                }
                off += s.len();
            }
            ColumnData::I64(v)
        }
        Dtype::F64 => {
            let mut v = Vec::with_capacity(total);
            let mut off = 0usize;
            for s in parts {
                if let ColumnData::F64(slice) = &s.data {
                    v.extend_from_slice(slice);
                }
                for i in 0..s.len() {
                    if s.validity.is_null(i) {
                        validity.set_null(off + i);
                    }
                }
                off += s.len();
            }
            ColumnData::F64(v)
        }
        Dtype::Bool => {
            let mut v = Vec::with_capacity(total);
            let mut off = 0usize;
            for s in parts {
                if let ColumnData::Bool(slice) = &s.data {
                    v.extend_from_slice(slice);
                }
                for i in 0..s.len() {
                    if s.validity.is_null(i) {
                        validity.set_null(off + i);
                    }
                }
                off += s.len();
            }
            ColumnData::Bool(v)
        }
        Dtype::Str => {
            let mut col = StringColumn::new();
            let mut off = 0usize;
            for s in parts {
                if let ColumnData::Str(sc) = &s.data {
                    for i in 0..sc.len() {
                        col.push(sc.get(i));
                    }
                }
                for i in 0..s.len() {
                    if s.validity.is_null(i) {
                        validity.set_null(off + i);
                    }
                }
                off += s.len();
            }
            ColumnData::Str(col)
        }
        Dtype::Date => {
            let mut v = Vec::with_capacity(total);
            let mut off = 0usize;
            for s in parts {
                if let ColumnData::Date(slice) = &s.data {
                    v.extend_from_slice(slice);
                }
                for i in 0..s.len() {
                    if s.validity.is_null(i) {
                        validity.set_null(off + i);
                    }
                }
                off += s.len();
            }
            ColumnData::Date(v)
        }
    };
    Series::new(name, data)
        .with_validity(validity)
        .map_err(|e| ParquetError::Shape(e.to_string()))
}

fn series_to_arrow(series: &Series) -> ParquetResult<(Field, ArrayRef)> {
    let nullable = series.null_count() > 0;
    let name = series.name.clone();
    match &series.data {
        ColumnData::I64(v) => {
            let array = build_int64_array(v, &series.validity);
            let field = Field::new(name, DataType::Int64, nullable);
            Ok((field, Arc::new(array)))
        }
        ColumnData::F64(v) => {
            let array = build_float64_array(v, &series.validity);
            let field = Field::new(name, DataType::Float64, nullable);
            Ok((field, Arc::new(array)))
        }
        ColumnData::Bool(v) => {
            let array = build_bool_array(v, &series.validity);
            let field = Field::new(name, DataType::Boolean, nullable);
            Ok((field, Arc::new(array)))
        }
        ColumnData::Str(sc) => {
            let array = build_string_array(sc, &series.validity);
            let field = Field::new(name, DataType::Utf8, nullable);
            Ok((field, Arc::new(array)))
        }
        ColumnData::Date(v) => {
            let array = build_date32_array(v, &series.validity);
            let field = Field::new(name, DataType::Date32, nullable);
            Ok((field, Arc::new(array)))
        }
    }
}

fn build_int64_array(values: &[i64], validity: &Validity) -> Int64Array {
    if validity.null_count() == 0 {
        return Int64Array::from(values.to_vec());
    }
    let mut builder = arrow_array::builder::Int64Builder::with_capacity(values.len());
    for (i, &v) in values.iter().enumerate() {
        if validity.is_null(i) {
            builder.append_null();
        } else {
            builder.append_value(v);
        }
    }
    builder.finish()
}

fn build_float64_array(values: &[f64], validity: &Validity) -> Float64Array {
    if validity.null_count() == 0 {
        return Float64Array::from(values.to_vec());
    }
    let mut builder = arrow_array::builder::Float64Builder::with_capacity(values.len());
    for (i, &v) in values.iter().enumerate() {
        if validity.is_null(i) {
            builder.append_null();
        } else {
            builder.append_value(v);
        }
    }
    builder.finish()
}

fn build_bool_array(values: &[bool], validity: &Validity) -> BooleanArray {
    if validity.null_count() == 0 {
        return BooleanArray::from(values.to_vec());
    }
    let mut builder = arrow_array::builder::BooleanBuilder::with_capacity(values.len());
    for (i, &v) in values.iter().enumerate() {
        if validity.is_null(i) {
            builder.append_null();
        } else {
            builder.append_value(v);
        }
    }
    builder.finish()
}

fn build_string_array(col: &StringColumn, validity: &Validity) -> StringArray {
    if validity.null_count() == 0 {
        let vals: Vec<&str> = (0..col.len()).map(|i| col.get(i)).collect();
        return StringArray::from(vals);
    }
    let mut builder = arrow_array::builder::StringBuilder::with_capacity(col.len(), col.data.len());
    for i in 0..col.len() {
        if validity.is_null(i) {
            builder.append_null();
        } else {
            builder.append_value(col.get(i));
        }
    }
    builder.finish()
}

fn build_date32_array(values: &[i64], validity: &Validity) -> Date32Array {
    let days: Vec<i32> = values.iter().map(|&d| d as i32).collect();
    if validity.null_count() == 0 {
        return Date32Array::from(days);
    }
    let mut builder = arrow_array::builder::Date32Builder::with_capacity(values.len());
    for (i, &d) in days.iter().enumerate() {
        if validity.is_null(i) {
            builder.append_null();
        } else {
            builder.append_value(d);
        }
    }
    builder.finish()
}

fn arrow_array_to_series(name: &str, array: &ArrayRef, dtype: &DataType) -> ParquetResult<Series> {
    let len = array.len();
    let validity = arrow_nulls_to_validity(array);
    let data = match dtype {
        DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 => {
            let arr = coerce_int64_array(array, name)?;
            let v: Vec<i64> = (0..arr.len())
                .map(|i| if arr.is_null(i) { 0 } else { arr.value(i) })
                .collect();
            ColumnData::I64(v)
        }
        DataType::UInt8 | DataType::UInt16 | DataType::UInt32 | DataType::UInt64 => {
            let v: Vec<i64> = (0..len)
                .map(|i| {
                    if array.is_null(i) {
                        0
                    } else {
                        array
                            .as_any()
                            .downcast_ref::<Int64Array>()
                            .map(|a| a.value(i))
                            .unwrap_or(0)
                    }
                })
                .collect();
            ColumnData::I64(v)
        }
        DataType::Float32 | DataType::Float64 => {
            let arr = coerce_float64_array(array, name)?;
            let v: Vec<f64> = (0..arr.len())
                .map(|i| {
                    if arr.is_null(i) {
                        f64::NAN
                    } else {
                        arr.value(i)
                    }
                })
                .collect();
            ColumnData::F64(v)
        }
        DataType::Boolean => {
            let arr = array
                .as_any()
                .downcast_ref::<BooleanArray>()
                .ok_or_else(|| ParquetError::Type(format!("expected bool column '{name}'")))?;
            let v: Vec<bool> = (0..arr.len())
                .map(|i| !arr.is_null(i) && arr.value(i))
                .collect();
            ColumnData::Bool(v)
        }
        DataType::Utf8 | DataType::LargeUtf8 => {
            let arr = coerce_string_array(array, name)?;
            let mut col = StringColumn::new();
            for i in 0..arr.len() {
                if arr.is_null(i) {
                    col.push("");
                } else {
                    col.push(arr.value(i));
                }
            }
            ColumnData::Str(col)
        }
        DataType::Date32 | DataType::Date64 => {
            let arr = array
                .as_any()
                .downcast_ref::<Date32Array>()
                .ok_or_else(|| ParquetError::Type(format!("expected date column '{name}'")))?;
            let v: Vec<i64> = (0..arr.len())
                .map(|i| {
                    if arr.is_null(i) {
                        0
                    } else {
                        arr.value(i) as i64
                    }
                })
                .collect();
            ColumnData::Date(v)
        }
        DataType::Timestamp(_, _) => {
            let arr = coerce_timestamp_ms_array(array, name)?;
            let v: Vec<i64> = (0..arr.len())
                .map(|i| if arr.is_null(i) { 0 } else { arr.value(i) })
                .collect();
            ColumnData::I64(v)
        }
        other => {
            return Err(ParquetError::Type(format!(
                "unsupported arrow type {:?} for column '{name}'",
                other
            )));
        }
    };
    Series::new(name.to_string(), data)
        .with_validity(validity)
        .map_err(|e| ParquetError::Shape(e.to_string()))
}

fn coerce_int64_array(array: &ArrayRef, name: &str) -> ParquetResult<Int64Array> {
    if let Some(a) = array.as_any().downcast_ref::<Int64Array>() {
        return Ok(a.clone());
    }
    if let Some(a) = array.as_any().downcast_ref::<arrow_array::Int32Array>() {
        let v: Vec<i64> = (0..a.len())
            .map(|i| if a.is_null(i) { 0 } else { a.value(i) as i64 })
            .collect();
        return Ok(Int64Array::from(v));
    }
    Err(ParquetError::Type(format!("expected int column '{name}'")))
}

fn coerce_float64_array(array: &ArrayRef, name: &str) -> ParquetResult<Float64Array> {
    if let Some(a) = array.as_any().downcast_ref::<Float64Array>() {
        return Ok(a.clone());
    }
    if let Some(a) = array.as_any().downcast_ref::<arrow_array::Float32Array>() {
        let v: Vec<f64> = (0..a.len())
            .map(|i| {
                if a.is_null(i) {
                    f64::NAN
                } else {
                    a.value(i) as f64
                }
            })
            .collect();
        return Ok(Float64Array::from(v));
    }
    Err(ParquetError::Type(format!(
        "expected float column '{name}'"
    )))
}

fn coerce_string_array(array: &ArrayRef, name: &str) -> ParquetResult<StringArray> {
    if let Some(a) = array.as_any().downcast_ref::<StringArray>() {
        return Ok(a.clone());
    }
    if let Some(a) = array
        .as_any()
        .downcast_ref::<arrow_array::LargeStringArray>()
    {
        let v: Vec<Option<String>> = (0..a.len())
            .map(|i| {
                if a.is_null(i) {
                    None
                } else {
                    Some(a.value(i).to_string())
                }
            })
            .collect();
        return Ok(StringArray::from(v));
    }
    Err(ParquetError::Type(format!(
        "expected string column '{name}'"
    )))
}

fn coerce_timestamp_ms_array(
    array: &ArrayRef,
    name: &str,
) -> ParquetResult<arrow_array::TimestampMillisecondArray> {
    use arrow_array::TimestampMillisecondArray;
    if let Some(a) = array
        .as_any()
        .downcast_ref::<arrow_array::TimestampMillisecondArray>()
    {
        return Ok(a.clone());
    }
    if let Some(a) = array
        .as_any()
        .downcast_ref::<arrow_array::TimestampMicrosecondArray>()
    {
        let v: Vec<i64> = (0..a.len())
            .map(|i| if a.is_null(i) { 0 } else { a.value(i) / 1000 })
            .collect();
        return Ok(TimestampMillisecondArray::from(v));
    }
    Err(ParquetError::Type(format!(
        "unsupported timestamp column '{name}'"
    )))
}

fn arrow_nulls_to_validity(array: &ArrayRef) -> Validity {
    let len = array.len();
    let mut v = Validity::all_valid(len);
    for i in 0..len {
        if array.is_null(i) {
            v.set_null(i);
        }
    }
    v
}

pub fn arrow_dtype_name(dt: &DataType) -> String {
    match dt {
        DataType::Int64 => "int".into(),
        DataType::Int32 => "int".into(),
        DataType::Float64 => "float".into(),
        DataType::Float32 => "float".into(),
        DataType::Boolean => "bool".into(),
        DataType::Utf8 | DataType::LargeUtf8 => "string".into(),
        DataType::Date32 | DataType::Date64 => "date".into(),
        DataType::Timestamp(_, _) => "timestamp".into(),
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_frame::DataFrame;

    #[test]
    fn dataframe_roundtrip() {
        let df = DataFrame::new(vec![
            Series::from_i64("id", vec![1, 2, 3]),
            Series::from_f64("score", vec![1.5, 2.5, 3.5]),
            Series::from_str("name", &["a", "b", "c"]),
        ])
        .unwrap();
        let batch = dataframe_to_record_batch(&df).unwrap();
        let back = record_batches_to_dataframe(&[batch], &Default::default()).unwrap();
        assert_eq!(back.nrows(), 3);
        assert_eq!(back.ncols(), 3);
        assert_eq!(back.get("id").unwrap().as_i64_slice().unwrap(), &[1, 2, 3]);
    }
}
