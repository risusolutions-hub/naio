//! Streaming write for large workbooks (constant-memory mode).

use crate::cell::CellValue;
use crate::error::{XlsxError, XlsxResult};
use crate::options::WriteOptions;
use rust_xlsxwriter::Workbook;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct StreamWriter {
    pub path: PathBuf,
    pub sheet_name: String,
    pub row: u32,
    workbook: Workbook,
    headers_written: bool,
    headers: Vec<String>,
}

pub struct StreamStore {
    next_id: u64,
    streams: HashMap<u64, StreamWriter>,
}

impl StreamStore {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            streams: HashMap::new(),
        }
    }

    pub fn open(
        &mut self,
        path: &Path,
        sheet_name: &str,
        headers: Option<Vec<String>>,
        _opts: &WriteOptions,
    ) -> XlsxResult<u64> {
        let mut workbook = Workbook::new();
        workbook
            .add_worksheet_with_constant_memory()
            .set_name(sheet_name)
            .map_err(|e| XlsxError::Sheet(e.to_string()))?;
        let headers_written = headers.is_none();
        let id = self.next_id;
        self.next_id += 1;
        self.streams.insert(
            id,
            StreamWriter {
                path: path.to_path_buf(),
                sheet_name: sheet_name.to_string(),
                row: if headers.is_some() { 1 } else { 0 },
                workbook,
                headers_written,
                headers: headers.unwrap_or_default(),
            },
        );
        Ok(id)
    }

    pub fn write_row(&mut self, id: u64, values: &[CellValue]) -> XlsxResult<()> {
        let stream = self
            .streams
            .get_mut(&id)
            .ok_or_else(|| XlsxError::Handle(format!("invalid stream handle: {id}")))?;
        let ws = stream
            .workbook
            .worksheet_from_index(0)
            .map_err(|e| XlsxError::Sheet(e.to_string()))?;
        if !stream.headers_written {
            for (i, h) in stream.headers.iter().enumerate() {
                ws.write(0, i as u16, h.as_str())
                    .map_err(|e| XlsxError::Cell(e.to_string()))?;
            }
            stream.headers_written = true;
            stream.row = 1;
        }
        let r = stream.row;
        for (c, val) in values.iter().enumerate() {
            write_stream_cell(ws, r, c as u16, val)?;
        }
        stream.row += 1;
        Ok(())
    }

    pub fn close(&mut self, id: u64) -> XlsxResult<()> {
        let mut stream = self
            .streams
            .remove(&id)
            .ok_or_else(|| XlsxError::Handle(format!("invalid stream handle: {id}")))?;
        stream
            .workbook
            .save(&stream.path)
            .map_err(|e| XlsxError::Io(e.to_string()))?;
        Ok(())
    }
}

impl Default for StreamStore {
    fn default() -> Self {
        Self::new()
    }
}

fn write_stream_cell(
    ws: &mut rust_xlsxwriter::Worksheet,
    row: u32,
    col: u16,
    value: &CellValue,
) -> XlsxResult<()> {
    match value {
        CellValue::Empty => Ok(()),
        CellValue::Int(n) => {
            ws.write(row, col, *n)
                .map_err(|e| XlsxError::Cell(e.to_string()))?;
            Ok(())
        }
        CellValue::Float(f) => {
            ws.write(row, col, *f)
                .map_err(|e| XlsxError::Cell(e.to_string()))?;
            Ok(())
        }
        CellValue::Bool(b) => {
            ws.write(row, col, *b)
                .map_err(|e| XlsxError::Cell(e.to_string()))?;
            Ok(())
        }
        CellValue::String(s) => {
            ws.write(row, col, s.as_str())
                .map_err(|e| XlsxError::Cell(e.to_string()))?;
            Ok(())
        }
        CellValue::Formula(f) => {
            ws.write_formula(row, col, f.as_str())
                .map_err(|e| XlsxError::Cell(e.to_string()))?;
            Ok(())
        }
        CellValue::Date(d) => {
            ws.write(row, col, *d)
                .map_err(|e| XlsxError::Cell(e.to_string()))?;
            Ok(())
        }
        CellValue::Error(e) => {
            ws.write(row, col, e.as_str())
                .map_err(|e| XlsxError::Cell(e.to_string()))?;
            Ok(())
        }
    }
}
