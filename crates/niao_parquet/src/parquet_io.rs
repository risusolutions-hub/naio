//! Parquet read/write.

use crate::bridge::{dataframe_to_record_batch, record_batches_to_dataframe};
use crate::error::{ParquetError, ParquetResult};
use crate::options::{ReadOptions, WriteOptions};
use arrow_array::RecordBatch;
use bytes::Bytes;
use memmap2::Mmap;
use niao_frame::DataFrame;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::arrow::ArrowWriter;
use parquet::file::metadata::ParquetMetaData;
use parquet::file::properties::WriterProperties;
use std::fs::File;
use std::io::{Seek, Write};
use std::path::Path;
use std::sync::Arc;

pub fn read_parquet_bytes(data: &[u8], opts: &ReadOptions) -> ParquetResult<DataFrame> {
    let batches = read_batches_from_bytes(data, opts)?;
    record_batches_to_dataframe(&batches, opts)
}

pub fn read_parquet_file(path: &Path, opts: &ReadOptions) -> ParquetResult<DataFrame> {
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file).map_err(|e| ParquetError::Io(e.to_string()))? };
    read_parquet_bytes(&mmap, opts)
}

pub fn write_parquet_bytes(df: &DataFrame, opts: &WriteOptions) -> ParquetResult<Vec<u8>> {
    let mut cursor = std::io::Cursor::new(Vec::new());
    write_parquet_to_writer(&mut cursor, df, opts)?;
    Ok(cursor.into_inner())
}

pub fn write_parquet_file(path: &Path, df: &DataFrame, opts: &WriteOptions) -> ParquetResult<()> {
    let mut file = File::create(path)?;
    write_parquet_to_writer(&mut file, df, opts)
}

fn write_parquet_to_writer<W: Write + Seek + Send>(
    writer: &mut W,
    df: &DataFrame,
    opts: &WriteOptions,
) -> ParquetResult<()> {
    let batch = dataframe_to_record_batch(df)?;
    let schema = batch.schema();
    let props = WriterProperties::builder()
        .set_compression(opts.compression)
        .set_max_row_group_size(opts.row_group_size)
        .build();
    let mut arrow_writer = ArrowWriter::try_new(writer, schema, Some(props))
        .map_err(|e| ParquetError::Parquet(e.to_string()))?;
    arrow_writer
        .write(&batch)
        .map_err(|e| ParquetError::Parquet(e.to_string()))?;
    arrow_writer
        .close()
        .map_err(|e| ParquetError::Parquet(e.to_string()))?;
    Ok(())
}

fn read_batches_from_bytes(data: &[u8], opts: &ReadOptions) -> ParquetResult<Vec<RecordBatch>> {
    let owned = Bytes::copy_from_slice(data);
    let builder = ParquetRecordBatchReaderBuilder::try_new(owned)
        .map_err(|e| ParquetError::Parquet(e.to_string()))?;
    let reader = builder
        .build()
        .map_err(|e| ParquetError::Parquet(e.to_string()))?;
    let mut batches = Vec::new();
    let mut rows_left = opts.rows;
    for batch in reader {
        let batch = batch.map_err(|e| ParquetError::Arrow(e.to_string()))?;
        if let Some(limit) = rows_left {
            let n = batch.num_rows();
            if n <= limit {
                batches.push(batch);
                rows_left = Some(limit - n);
            } else {
                batches.push(batch.slice(0, limit));
                break;
            }
        } else {
            batches.push(batch);
        }
    }
    Ok(batches)
}

pub fn parquet_info_bytes(data: &[u8]) -> ParquetResult<ParquetInfo> {
    let owned = Bytes::copy_from_slice(data);
    let builder = ParquetRecordBatchReaderBuilder::try_new(owned)
        .map_err(|e| ParquetError::Parquet(e.to_string()))?;
    let meta = builder.metadata().clone();
    let schema = builder.schema().clone();
    let fields: Vec<(String, String)> = schema
        .fields()
        .iter()
        .map(|f| {
            (
                f.name().clone(),
                crate::bridge::arrow_dtype_name(f.data_type()),
            )
        })
        .collect();
    Ok(info_from_metadata(&meta, fields))
}

pub fn parquet_info_file(path: &Path) -> ParquetResult<ParquetInfo> {
    let data = std::fs::read(path)?;
    parquet_info_bytes(&data)
}

pub fn parquet_schema_bytes(data: &[u8]) -> ParquetResult<Vec<(String, String)>> {
    let owned = Bytes::copy_from_slice(data);
    let builder = ParquetRecordBatchReaderBuilder::try_new(owned)
        .map_err(|e| ParquetError::Parquet(e.to_string()))?;
    let schema = builder.schema().clone();
    Ok(schema
        .fields()
        .iter()
        .map(|f| {
            (
                f.name().clone(),
                crate::bridge::arrow_dtype_name(f.data_type()),
            )
        })
        .collect())
}

pub fn validate_parquet_bytes(data: &[u8]) -> bool {
    let owned = Bytes::copy_from_slice(data);
    ParquetRecordBatchReaderBuilder::try_new(owned).is_ok()
}

#[derive(Clone, Debug)]
pub struct ParquetInfo {
    pub rows: usize,
    pub cols: usize,
    pub columns: Vec<String>,
    pub types: Vec<String>,
    pub row_groups: usize,
    pub compressed_size: usize,
    pub uncompressed_size: usize,
    pub format: String,
}

fn info_from_metadata(meta: &Arc<ParquetMetaData>, fields: Vec<(String, String)>) -> ParquetInfo {
    let file_meta = meta.file_metadata();
    let rows = file_meta.num_rows() as usize;
    let columns: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
    let types: Vec<String> = fields.iter().map(|(_, t)| t.clone()).collect();
    let mut compressed = 0usize;
    let mut uncompressed = 0usize;
    for rg in meta.row_groups() {
        for col in rg.columns() {
            compressed += col.compressed_size() as usize;
            uncompressed += col.uncompressed_size() as usize;
        }
    }
    ParquetInfo {
        rows,
        cols: columns.len(),
        columns,
        types,
        row_groups: meta.num_row_groups(),
        compressed_size: compressed,
        uncompressed_size: uncompressed,
        format: "parquet".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::WriteOptions;
    use niao_frame::{DataFrame, Series};

    #[test]
    fn parquet_roundtrip_file() {
        let df = DataFrame::new(vec![
            Series::from_i64("id", (0..1000).collect()),
            Series::from_f64("v", (0..1000).map(|i| i as f64 * 0.1).collect()),
            Series::from_str("label", &vec!["x"; 1000]),
        ])
        .unwrap();
        let dir = std::env::temp_dir().join("niao_parquet_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("roundtrip.parquet");
        write_parquet_file(&path, &df, &WriteOptions::default()).unwrap();
        let back = read_parquet_file(&path, &ReadOptions::default()).unwrap();
        assert_eq!(back.nrows(), 1000);
        assert_eq!(back.ncols(), 3);
        let info = parquet_info_file(&path).unwrap();
        assert_eq!(info.rows, 1000);
        let _ = std::fs::remove_file(path);
    }
}
