//! Excel .xlsx read/write for Niao (`nxlsx`).
//!
//! Backed by calamine (read) and rust_xlsxwriter (write).

mod bridge;
mod cell;
mod error;
mod options;
mod read;
mod stream;
mod style;
mod workbook;
mod write;

pub use bridge::{
    dataframe_to_table, sheet_to_row_arrays, sheet_to_table, table_to_dataframe, table_to_rows,
    write_table_to_sheet, Table,
};
pub use cell::{column_index, column_letter, parse_range, CellRange, CellValue};
pub use error::{XlsxError, XlsxResult};
pub use options::{ChunkReadOptions, ReadOptions, SheetSelector, WriteOptions};
pub use read::{
    info_file, open_file, read_all_sheets, read_chunk_file, validate_bytes, validate_file,
    SheetInfo, WorkbookInfo,
};
pub use stream::StreamStore;
pub use style::CellStyle;
pub use workbook::{WorkbookData, WorkbookStore};
pub use write::{column_letters, write_bytes, write_file};

/// Maximum in-memory payload size (256 MiB guard).
pub const MAX_BYTES: usize = 256 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn write_read_roundtrip() {
        let path = std::env::temp_dir().join("niao_xlsx_lib_test.xlsx");
        let mut wb = WorkbookData::new();
        let mut cols = HashMap::new();
        cols.insert("x".to_string(), vec![CellValue::Int(1), CellValue::Int(2)]);
        let table = Table::from_columns(cols).unwrap();
        write_table_to_sheet(&mut wb, "Sheet1", &table, true).unwrap();
        write_file(&path, &wb, &WriteOptions::default()).unwrap();
        let loaded = open_file(&path, &ReadOptions::default()).unwrap();
        assert!(loaded.sheets[0].nrows() >= 2);
        let _ = std::fs::remove_file(path);
    }
}
