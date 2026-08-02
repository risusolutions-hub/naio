//! XLSX write path (rust_xlsxwriter).

use crate::cell::{column_letter, CellValue};
use crate::error::{XlsxError, XlsxResult};
use crate::options::WriteOptions;
use crate::style::CellStyle;
use crate::workbook::{SheetData, WorkbookData};
use rust_xlsxwriter::{Format, Workbook, Worksheet};
use std::path::Path;

pub fn write_file(path: &Path, data: &WorkbookData, opts: &WriteOptions) -> XlsxResult<()> {
    let bytes = write_bytes(data, opts)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

pub fn write_bytes(data: &WorkbookData, opts: &WriteOptions) -> XlsxResult<Vec<u8>> {
    let mut workbook = Workbook::new();
    for sheet in &data.sheets {
        write_sheet(&mut workbook, sheet, opts)?;
    }
    workbook
        .save_to_buffer()
        .map_err(|e| XlsxError::Io(e.to_string()))
}

fn write_sheet(workbook: &mut Workbook, sheet: &SheetData, opts: &WriteOptions) -> XlsxResult<()> {
    let ws = if opts.constant_memory {
        workbook.add_worksheet_with_constant_memory()
    } else {
        workbook.add_worksheet()
    };
    ws.set_name(&sheet.name)
        .map_err(|e| XlsxError::Sheet(e.to_string()))?;

    for (&(row, col), style) in &sheet.styles {
        if let Ok(fmt) = style.to_format() {
            let r = row - 1;
            let c = (col - 1) as u16;
            let _ = ws.set_row_format(r, &fmt);
            let _ = ws.write_with_format(r, c, "", &fmt);
        }
    }

    for (&(row, col), formula) in &sheet.formulas {
        ws.write_formula(row - 1, (col - 1) as u16, formula.as_str())
            .map_err(|e| XlsxError::Cell(e.to_string()))?;
    }

    for (r_idx, row) in sheet.rows.iter().enumerate() {
        let row_num = (r_idx + 1) as u32;
        for (c_idx, cell) in row.iter().enumerate() {
            let col_num = (c_idx + 1) as u32;
            if sheet.formulas.contains_key(&(row_num, col_num)) {
                continue;
            }
            let style = sheet.styles.get(&(row_num, col_num));
            write_cell(ws, row_num - 1, (col_num - 1) as u16, cell, style)?;
        }
    }

    let blank = Format::new();
    for merge in &sheet.merges {
        if merge.start_row != merge.end_row || merge.start_col != merge.end_col {
            ws.merge_range(
                merge.start_row - 1,
                (merge.start_col - 1) as u16,
                merge.end_row - 1,
                (merge.end_col - 1) as u16,
                "",
                &blank,
            )
            .map_err(|e| XlsxError::Cell(e.to_string()))?;
        }
    }

    for (&col, &width) in &sheet.col_widths {
        ws.set_column_width((col - 1) as u16, width)
            .map_err(|e| XlsxError::Style(e.to_string()))?;
    }

    for (&row, &height) in &sheet.row_heights {
        ws.set_row_height(row - 1, height)
            .map_err(|e| XlsxError::Style(e.to_string()))?;
    }

    if let Some(fr) = sheet.freeze_row.or(opts.freeze_row) {
        let fc = sheet.freeze_col.or(opts.freeze_col).unwrap_or(0);
        ws.set_freeze_panes(fr, fc as u16)
            .map_err(|e| XlsxError::Style(e.to_string()))?;
    }

    if opts.autofit {
        let cols = sheet.ncols().max(1) as u16;
        for c in 0..cols {
            ws.set_column_width(c, 12.0)
                .map_err(|e| XlsxError::Style(e.to_string()))?;
        }
    }

    Ok(())
}

fn write_cell(
    ws: &mut Worksheet,
    row: u32,
    col: u16,
    value: &CellValue,
    style: Option<&CellStyle>,
) -> XlsxResult<()> {
    let fmt = style.and_then(|s| s.to_format().ok());
    match value {
        CellValue::Empty => Ok(()),
        CellValue::Int(n) => {
            if let Some(ref f) = fmt {
                ws.write_with_format(row, col, *n, f)
            } else {
                ws.write(row, col, *n)
            }
            .map_err(|e| XlsxError::Cell(e.to_string()))?;
            Ok(())
        }
        CellValue::Float(fv) => {
            if let Some(ref f) = fmt {
                ws.write_with_format(row, col, *fv, f)
            } else {
                ws.write(row, col, *fv)
            }
            .map_err(|e| XlsxError::Cell(e.to_string()))?;
            Ok(())
        }
        CellValue::Bool(b) => {
            if let Some(ref f) = fmt {
                ws.write_with_format(row, col, *b, f)
            } else {
                ws.write(row, col, *b)
            }
            .map_err(|e| XlsxError::Cell(e.to_string()))?;
            Ok(())
        }
        CellValue::String(s) => {
            if let Some(ref f) = fmt {
                ws.write_with_format(row, col, s.as_str(), f)
            } else {
                ws.write(row, col, s.as_str())
            }
            .map_err(|e| XlsxError::Cell(e.to_string()))?;
            Ok(())
        }
        CellValue::Formula(_) => Ok(()),
        CellValue::Date(serial) => {
            if let Some(ref f) = fmt {
                ws.write_with_format(row, col, *serial, f)
            } else {
                ws.write(row, col, *serial)
            }
            .map_err(|e| XlsxError::Cell(e.to_string()))?;
            Ok(())
        }
        CellValue::Error(e) => {
            if let Some(ref f) = fmt {
                ws.write_with_format(row, col, e.as_str(), f)
            } else {
                ws.write(row, col, e.as_str())
            }
            .map_err(|e| XlsxError::Cell(e.to_string()))?;
            Ok(())
        }
    }
}

pub fn column_letters(col: u32) -> XlsxResult<String> {
    column_letter(col)
}
