//! Parquet + Arrow IPC read/write for Niao (`nparquet`).
//!
//! Backed by Apache Arrow / Parquet (arrow-rs). Converts to/from `niao_frame::DataFrame`
//! for high-performance columnar interchange beside `ncolumnar`'s NCOL1 format.

mod bridge;
mod error;
mod ipc;
mod options;
mod parquet_io;

pub use bridge::arrow_dtype_name;
pub use error::{ParquetError, ParquetResult};
pub use ipc::{read_ipc_bytes, read_ipc_file, validate_ipc_bytes, write_ipc_bytes, write_ipc_file};
pub use options::{ReadOptions, WriteOptions};
pub use parquet_io::{
    parquet_info_bytes, parquet_info_file, parquet_schema_bytes, read_parquet_bytes,
    read_parquet_file, validate_parquet_bytes, write_parquet_bytes, write_parquet_file,
    ParquetInfo,
};

/// Maximum in-memory payload size (256 MiB guard).
pub const MAX_BYTES: usize = 256 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;
    use niao_frame::{DataFrame, Series};

    #[test]
    fn encode_decode_parquet() {
        let df = DataFrame::new(vec![
            Series::from_i64("id", vec![10, 20]),
            Series::from_f64("score", vec![1.1, 2.2]),
        ])
        .unwrap();
        let bytes = write_parquet_bytes(&df, &WriteOptions::default()).unwrap();
        assert!(validate_parquet_bytes(&bytes));
        let back = read_parquet_bytes(&bytes, &ReadOptions::default()).unwrap();
        assert_eq!(back.nrows(), 2);
    }
}
