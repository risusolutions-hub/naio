//! Read/write options for xlsx operations.

#[derive(Debug, Clone)]
pub struct ReadOptions {
    /// First row is a header row (default `true` for `to_table`, `false` for raw rows).
    pub header: bool,
    /// 1-based start row (inclusive).
    pub start_row: u32,
    /// Max rows to read (`None` = all).
    pub rows: Option<usize>,
    /// Sheet name or index (1-based). `None` = active/first sheet.
    pub sheet: Option<SheetSelector>,
    /// Column projection by 1-based index (1 = A).
    pub columns: Option<Vec<u16>>,
    /// Treat empty cells as `nil`/skip in row arrays.
    pub skip_empty: bool,
    /// Infer typed columns when building tables.
    pub infer_types: bool,
}

#[derive(Debug, Clone)]
pub enum SheetSelector {
    Name(String),
    Index(usize),
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            header: true,
            start_row: 1,
            rows: None,
            sheet: None,
            columns: None,
            skip_empty: false,
            infer_types: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WriteOptions {
    pub constant_memory: bool,
    pub default_sheet: Option<String>,
    pub header: bool,
    pub autofit: bool,
    pub freeze_row: Option<u32>,
    pub freeze_col: Option<u32>,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            constant_memory: false,
            default_sheet: None,
            header: true,
            autofit: false,
            freeze_row: None,
            freeze_col: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChunkReadOptions {
    pub start_row: u32,
    pub count: usize,
    pub sheet: Option<SheetSelector>,
}

impl Default for ChunkReadOptions {
    fn default() -> Self {
        Self {
            start_row: 1,
            count: 1000,
            sheet: None,
        }
    }
}
