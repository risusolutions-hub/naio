//! Arrow IPC (Feather v2 / streaming) read/write.

use crate::bridge::{dataframe_to_record_batch, record_batches_to_dataframe};
use crate::error::{ParquetError, ParquetResult};
use crate::options::ReadOptions;
use arrow_ipc::reader::StreamReader;
use arrow_ipc::writer::StreamWriter;
use niao_frame::DataFrame;
use std::fs::File;
use std::io::{Cursor, Write};
use std::path::Path;

pub fn read_ipc_bytes(data: &[u8], opts: &ReadOptions) -> ParquetResult<DataFrame> {
    let cursor = Cursor::new(data);
    let reader =
        StreamReader::try_new(cursor, None).map_err(|e| ParquetError::Arrow(e.to_string()))?;
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
    record_batches_to_dataframe(&batches, opts)
}

pub fn read_ipc_file(path: &Path, opts: &ReadOptions) -> ParquetResult<DataFrame> {
    let data = std::fs::read(path)?;
    read_ipc_bytes(&data, opts)
}

pub fn write_ipc_bytes(df: &DataFrame) -> ParquetResult<Vec<u8>> {
    let mut buf = Vec::new();
    write_ipc_to_writer(&mut buf, df)?;
    Ok(buf)
}

pub fn write_ipc_file(path: &Path, df: &DataFrame) -> ParquetResult<()> {
    let mut file = File::create(path)?;
    write_ipc_to_writer(&mut file, df)
}

fn write_ipc_to_writer<W: Write>(writer: &mut W, df: &DataFrame) -> ParquetResult<()> {
    let batch = dataframe_to_record_batch(df)?;
    let schema = batch.schema();
    let mut ipc_writer =
        StreamWriter::try_new(writer, &schema).map_err(|e| ParquetError::Arrow(e.to_string()))?;
    ipc_writer
        .write(&batch)
        .map_err(|e| ParquetError::Arrow(e.to_string()))?;
    ipc_writer
        .finish()
        .map_err(|e| ParquetError::Arrow(e.to_string()))?;
    Ok(())
}

pub fn validate_ipc_bytes(data: &[u8]) -> bool {
    let cursor = Cursor::new(data);
    StreamReader::try_new(cursor, None).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_frame::{DataFrame, Series};

    #[test]
    fn ipc_roundtrip() {
        let df = DataFrame::new(vec![
            Series::from_i64("a", vec![1, 2, 3]),
            Series::from_str("b", &["x", "y", "z"]),
        ])
        .unwrap();
        let bytes = write_ipc_bytes(&df).unwrap();
        assert!(validate_ipc_bytes(&bytes));
        let back = read_ipc_bytes(&bytes, &ReadOptions::default()).unwrap();
        assert_eq!(back.nrows(), 3);
    }
}
