//! XLSX read path (calamine + mmap).

use crate::cell::CellValue;
use crate::error::{XlsxError, XlsxResult};
use crate::options::{ChunkReadOptions, ReadOptions, SheetSelector};
use crate::workbook::{SheetData, WorkbookData};
use calamine::{open_workbook_auto, Reader, Sheets};
use memmap2::Mmap;
use std::fs::File;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct WorkbookInfo {
    pub path: String,
    pub sheet_names: Vec<String>,
    pub sheets: Vec<SheetInfo>,
    pub file_size: u64,
}

#[derive(Debug, Clone)]
pub struct SheetInfo {
    pub name: String,
    pub rows: Option<usize>,
    pub cols: Option<usize>,
}

pub fn validate_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    let Ok(file) = File::open(path) else {
        return false;
    };
    let Ok(mmap) = (unsafe { Mmap::map(&file) }) else {
        return false;
    };
    validate_bytes(&mmap)
}

pub fn validate_bytes(data: &[u8]) -> bool {
    zip::ZipArchive::new(std::io::Cursor::new(data)).is_ok()
}

pub fn info_file(path: &Path) -> XlsxResult<WorkbookInfo> {
    let meta = std::fs::metadata(path)?;
    let mut workbook = open_workbook_auto(path).map_err(map_calamine)?;
    let names = workbook.sheet_names().to_vec();
    let mut sheets = Vec::with_capacity(names.len());
    for name in &names {
        let (rows, cols) = worksheet_dims(&mut workbook, name)?;
        sheets.push(SheetInfo {
            name: name.clone(),
            rows,
            cols,
        });
    }
    Ok(WorkbookInfo {
        path: path.display().to_string(),
        sheet_names: names,
        sheets,
        file_size: meta.len(),
    })
}

pub fn open_file(path: &Path, opts: &ReadOptions) -> XlsxResult<WorkbookData> {
    let mut workbook = open_workbook_auto(path).map_err(map_calamine)?;
    let names = workbook.sheet_names().to_vec();
    if names.is_empty() {
        return Err(XlsxError::Format("workbook has no sheets".into()));
    }

    let target_names: Vec<String> = match &opts.sheet {
        Some(SheetSelector::Name(n)) => vec![n.clone()],
        Some(SheetSelector::Index(i)) => {
            let idx = i.saturating_sub(1);
            vec![names
                .get(idx)
                .cloned()
                .ok_or_else(|| XlsxError::Sheet(format!("sheet index out of range: {i}")))?]
        }
        None => names,
    };

    let mut sheets = Vec::with_capacity(target_names.len());
    for name in &target_names {
        let rows = read_sheet_range(&mut workbook, name, opts)?;
        sheets.push(SheetData {
            name: name.clone(),
            rows,
            ..Default::default()
        });
    }

    Ok(WorkbookData {
        sheets,
        active: 0,
        source_path: Some(path.display().to_string()),
        dirty: false,
    })
}

pub fn read_chunk_file(path: &Path, opts: &ChunkReadOptions) -> XlsxResult<Vec<Vec<CellValue>>> {
    let mut workbook = open_workbook_auto(path).map_err(map_calamine)?;
    let names = workbook.sheet_names();
    let name = resolve_sheet_name(&names, opts.sheet.as_ref())?;
    let range = workbook.worksheet_range(&name).map_err(map_calamine)?;
    let start = opts.start_row.saturating_sub(1) as usize;
    let mut out = Vec::with_capacity(opts.count.min(10_000));
    for row in range.rows().into_iter().skip(start).take(opts.count) {
        out.push(row.iter().map(|c| CellValue::from(c.clone())).collect());
    }
    Ok(out)
}

pub fn read_all_sheets(path: &Path, opts: &ReadOptions) -> XlsxResult<WorkbookData> {
    open_file(path, opts)
}

fn resolve_sheet_name(names: &[String], sheet: Option<&SheetSelector>) -> XlsxResult<String> {
    match sheet {
        Some(SheetSelector::Name(n)) => {
            if names.iter().any(|s| s == n) {
                Ok(n.clone())
            } else {
                Err(XlsxError::Sheet(format!("sheet not found: {n}")))
            }
        }
        Some(SheetSelector::Index(i)) => names
            .get(i.saturating_sub(1))
            .cloned()
            .ok_or_else(|| XlsxError::Sheet(format!("sheet index out of range: {i}"))),
        None => names
            .first()
            .cloned()
            .ok_or_else(|| XlsxError::Format("workbook has no sheets".into())),
    }
}

fn worksheet_dims(
    workbook: &mut Sheets<impl std::io::Read + std::io::Seek>,
    name: &str,
) -> XlsxResult<(Option<usize>, Option<usize>)> {
    let range = workbook.worksheet_range(name).map_err(map_calamine)?;
    let rows = range.height();
    let cols = range.width();
    Ok((
        if rows > 0 { Some(rows) } else { None },
        if cols > 0 { Some(cols) } else { None },
    ))
}

fn read_sheet_range(
    workbook: &mut Sheets<impl std::io::Read + std::io::Seek>,
    name: &str,
    opts: &ReadOptions,
) -> XlsxResult<Vec<Vec<CellValue>>> {
    let range = workbook.worksheet_range(name).map_err(map_calamine)?;
    let start = opts.start_row.saturating_sub(1) as usize;
    let limit = opts.rows.unwrap_or(usize::MAX);
    let mut out = Vec::new();
    for row in range.rows().into_iter().skip(start).take(limit) {
        let mut cells: Vec<CellValue> = row.iter().map(|c| CellValue::from(c.clone())).collect();
        if let Some(cols) = &opts.columns {
            let mut projected = Vec::with_capacity(cols.len());
            for c in cols {
                let idx = (*c as usize).saturating_sub(1);
                projected.push(cells.get(idx).cloned().unwrap_or(CellValue::Empty));
            }
            cells = projected;
        }
        if opts.skip_empty && cells.iter().all(|c| c.is_empty()) {
            continue;
        }
        out.push(cells);
    }
    Ok(out)
}

fn map_calamine(e: calamine::Error) -> XlsxError {
    XlsxError::Format(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::Table;
    use crate::options::WriteOptions;
    use crate::write::write_file;
    use std::collections::HashMap;

    #[test]
    fn roundtrip_via_calamine() {
        let dir = std::env::temp_dir().join("niao_xlsx_test_roundtrip.xlsx");
        let mut cols = HashMap::new();
        cols.insert("id".to_string(), vec![CellValue::Int(1), CellValue::Int(2)]);
        cols.insert(
            "name".to_string(),
            vec![CellValue::String("a".into()), CellValue::String("b".into())],
        );
        let table = Table::from_columns(cols).unwrap();
        let mut wb = WorkbookData::new();
        wb.sheets[0].name = "Data".into();
        crate::bridge::write_table_to_sheet(&mut wb, "Data", &table, true).unwrap();
        write_file(&dir, &wb, &WriteOptions::default()).unwrap();
        let loaded = open_file(&dir, &ReadOptions::default()).unwrap();
        assert_eq!(loaded.sheets[0].nrows(), 3);
        let _ = std::fs::remove_file(dir);
    }
}
