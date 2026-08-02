//! In-memory workbook model and handle store.

use crate::cell::{CellRange, CellValue};
use crate::error::{XlsxError, XlsxResult};
use crate::style::CellStyle;
use std::collections::HashMap;

/// Maximum in-memory workbook size guard (256 MiB of cell payload estimate).
pub const MAX_CELLS: usize = 5_000_000;

#[derive(Debug, Clone)]
pub struct MergeRange {
    pub start_row: u32,
    pub start_col: u32,
    pub end_row: u32,
    pub end_col: u32,
}

#[derive(Debug, Clone, Default)]
pub struct SheetData {
    pub name: String,
    pub rows: Vec<Vec<CellValue>>,
    pub formulas: HashMap<(u32, u32), String>,
    pub styles: HashMap<(u32, u32), CellStyle>,
    pub merges: Vec<MergeRange>,
    pub col_widths: HashMap<u32, f64>,
    pub row_heights: HashMap<u32, f64>,
    pub freeze_row: Option<u32>,
    pub freeze_col: Option<u32>,
}

impl SheetData {
    pub fn nrows(&self) -> usize {
        self.rows.len()
    }

    pub fn ncols(&self) -> usize {
        self.rows.iter().map(|r| r.len()).max().unwrap_or(0)
    }

    pub fn get_cell(&self, row: u32, col: u32) -> CellValue {
        let r = row.saturating_sub(1) as usize;
        let c = col.saturating_sub(1) as usize;
        self.rows
            .get(r)
            .and_then(|row| row.get(c))
            .cloned()
            .unwrap_or(CellValue::Empty)
    }

    pub fn set_cell(&mut self, row: u32, col: u32, value: CellValue) -> XlsxResult<()> {
        ensure_capacity(&mut self.rows, row as usize, col as usize);
        let total = self.rows.iter().map(|r| r.len()).sum::<usize>();
        if total > MAX_CELLS {
            return Err(XlsxError::Limit(format!(
                "workbook exceeds {MAX_CELLS} populated cells"
            )));
        }
        let r = (row - 1) as usize;
        let c = (col - 1) as usize;
        self.rows[r][c] = value;
        Ok(())
    }

    pub fn set_formula(&mut self, row: u32, col: u32, formula: String) -> XlsxResult<()> {
        self.formulas.insert((row, col), formula);
        self.set_cell(row, col, CellValue::Formula(String::new()))
    }
}

#[derive(Debug, Clone)]
pub struct WorkbookData {
    pub sheets: Vec<SheetData>,
    pub active: usize,
    pub source_path: Option<String>,
    pub dirty: bool,
}

impl WorkbookData {
    pub fn new() -> Self {
        Self {
            sheets: vec![SheetData {
                name: "Sheet1".to_string(),
                ..Default::default()
            }],
            active: 0,
            source_path: None,
            dirty: true,
        }
    }

    pub fn sheet_index(&self, name: &str) -> Option<usize> {
        self.sheets.iter().position(|s| s.name == name)
    }

    pub fn sheet_mut(&mut self, name: &str) -> XlsxResult<&mut SheetData> {
        let idx = self
            .sheet_index(name)
            .ok_or_else(|| XlsxError::Sheet(format!("sheet not found: {name}")))?;
        Ok(&mut self.sheets[idx])
    }

    pub fn sheet(&self, name: &str) -> XlsxResult<&SheetData> {
        let idx = self
            .sheet_index(name)
            .ok_or_else(|| XlsxError::Sheet(format!("sheet not found: {name}")))?;
        Ok(&self.sheets[idx])
    }

    pub fn add_sheet(&mut self, name: &str) -> XlsxResult<()> {
        if self.sheet_index(name).is_some() {
            return Err(XlsxError::Sheet(format!("sheet already exists: {name}")));
        }
        self.sheets.push(SheetData {
            name: name.to_string(),
            ..Default::default()
        });
        self.dirty = true;
        Ok(())
    }

    pub fn remove_sheet(&mut self, name: &str) -> XlsxResult<()> {
        if self.sheets.len() <= 1 {
            return Err(XlsxError::Sheet("cannot remove the only sheet".into()));
        }
        let idx = self
            .sheet_index(name)
            .ok_or_else(|| XlsxError::Sheet(format!("sheet not found: {name}")))?;
        self.sheets.remove(idx);
        if self.active >= self.sheets.len() {
            self.active = self.sheets.len() - 1;
        }
        self.dirty = true;
        Ok(())
    }

    pub fn rename_sheet(&mut self, old: &str, new: &str) -> XlsxResult<()> {
        if self.sheet_index(new).is_some() {
            return Err(XlsxError::Sheet(format!("sheet already exists: {new}")));
        }
        let sheet = self.sheet_mut(old)?;
        sheet.name = new.to_string();
        self.dirty = true;
        Ok(())
    }

    pub fn apply_merge(&mut self, sheet: &str, range: &CellRange) -> XlsxResult<()> {
        let s = self.sheet_mut(sheet)?;
        s.merges.push(MergeRange {
            start_row: range.start_row,
            start_col: range.start_col,
            end_row: range.end_row,
            end_col: range.end_col,
        });
        self.dirty = true;
        Ok(())
    }

    pub fn apply_style_range(
        &mut self,
        sheet: &str,
        range: &CellRange,
        style: CellStyle,
    ) -> XlsxResult<()> {
        let s = self.sheet_mut(sheet)?;
        for row in range.start_row..=range.end_row {
            for col in range.start_col..=range.end_col {
                s.styles.insert((row, col), style.clone());
            }
        }
        self.dirty = true;
        Ok(())
    }
}

impl Default for WorkbookData {
    fn default() -> Self {
        Self::new()
    }
}

pub struct WorkbookStore {
    next_id: u64,
    books: HashMap<u64, WorkbookData>,
}

impl WorkbookStore {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            books: HashMap::new(),
        }
    }

    pub fn alloc(&mut self, book: WorkbookData) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.books.insert(id, book);
        id
    }

    pub fn get(&self, id: u64) -> XlsxResult<&WorkbookData> {
        self.books
            .get(&id)
            .ok_or_else(|| XlsxError::Handle(format!("invalid workbook handle: {id}")))
    }

    pub fn get_mut(&mut self, id: u64) -> XlsxResult<&mut WorkbookData> {
        self.books
            .get_mut(&id)
            .ok_or_else(|| XlsxError::Handle(format!("invalid workbook handle: {id}")))
    }

    pub fn close(&mut self, id: u64) -> bool {
        self.books.remove(&id).is_some()
    }
}

impl Default for WorkbookStore {
    fn default() -> Self {
        Self::new()
    }
}

fn ensure_capacity(rows: &mut Vec<Vec<CellValue>>, row_1: usize, col_1: usize) {
    if rows.len() < row_1 {
        rows.resize_with(row_1, Vec::new);
    }
    let row = &mut rows[row_1 - 1];
    if row.len() < col_1 {
        row.resize(col_1, CellValue::Empty);
    }
}
