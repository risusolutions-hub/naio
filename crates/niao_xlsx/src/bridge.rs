//! Columnar table ↔ sheet bridge and nframe interop.

use crate::cell::{column_letter, CellValue};
use crate::error::{XlsxError, XlsxResult};
use crate::workbook::{SheetData, WorkbookData};
use niao_frame::{ColumnData, DataFrame, Dtype, Series, Validity};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Table {
    pub columns: HashMap<String, Vec<CellValue>>,
    pub nrows: usize,
}

impl Table {
    pub fn from_columns(mut columns: HashMap<String, Vec<CellValue>>) -> XlsxResult<Self> {
        let nrows = columns.values().map(|c| c.len()).max().unwrap_or(0);
        for (name, col) in &mut columns {
            if col.len() < nrows {
                col.resize(nrows, CellValue::Empty);
            }
            if col.len() != nrows {
                return Err(XlsxError::Shape(format!(
                    "column '{name}' length {} != {nrows}",
                    col.len()
                )));
            }
        }
        Ok(Self { columns, nrows })
    }

    pub fn column_names(&self) -> Vec<String> {
        let mut names: Vec<_> = self.columns.keys().cloned().collect();
        names.sort_unstable();
        names
    }
}

pub fn sheet_to_table(sheet: &SheetData, header: bool, infer_types: bool) -> XlsxResult<Table> {
    if sheet.rows.is_empty() {
        return Ok(Table {
            columns: HashMap::new(),
            nrows: 0,
        });
    }

    let ncol = sheet.ncols().max(1);
    let names: Vec<String> = if header {
        sheet.rows[0]
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let s = c.as_display_string();
                if s.is_empty() {
                    format!("col_{i}")
                } else {
                    s
                }
            })
            .collect()
    } else {
        (0..ncol)
            .map(|i| column_letter((i + 1) as u32).unwrap_or_else(|_| format!("col_{i}")))
            .collect()
    };

    let data_rows: &[Vec<CellValue>] = if header {
        &sheet.rows[1..]
    } else {
        &sheet.rows
    };

    let mut columns: HashMap<String, Vec<CellValue>> = HashMap::new();
    for name in &names {
        columns.insert(name.clone(), Vec::with_capacity(data_rows.len()));
    }

    for row in data_rows {
        for (i, name) in names.iter().enumerate() {
            let val = row.get(i).cloned().unwrap_or(CellValue::Empty);
            columns.get_mut(name).unwrap().push(val);
        }
    }

    let columns = if infer_types {
        infer_column_types(columns)?
    } else {
        columns
    };

    Table::from_columns(columns)
}

pub fn table_to_rows(table: &Table, header: bool) -> Vec<Vec<CellValue>> {
    let names = table.column_names();
    let mut rows = Vec::new();
    if header {
        rows.push(names.iter().map(|n| CellValue::String(n.clone())).collect());
    }
    for r in 0..table.nrows {
        let mut row = Vec::with_capacity(names.len());
        for name in &names {
            let col = table.columns.get(name).unwrap();
            row.push(col.get(r).cloned().unwrap_or(CellValue::Empty));
        }
        rows.push(row);
    }
    rows
}

pub fn write_table_to_sheet(
    wb: &mut WorkbookData,
    sheet_name: &str,
    table: &Table,
    header: bool,
) -> XlsxResult<()> {
    if wb.sheet_index(sheet_name).is_none() {
        wb.add_sheet(sheet_name)?;
    }
    let sheet = wb.sheet_mut(sheet_name)?;
    sheet.rows = table_to_rows(table, header);
    wb.dirty = true;
    Ok(())
}

pub fn table_to_dataframe(table: &Table) -> XlsxResult<DataFrame> {
    let names = table.column_names();
    let mut series = Vec::with_capacity(names.len());
    for name in &names {
        let col = table
            .columns
            .get(name)
            .ok_or_else(|| XlsxError::Shape(format!("missing column: {name}")))?;
        series.push(column_to_series(name, col)?);
    }
    DataFrame::new(series).map_err(|e| XlsxError::Shape(e.to_string()))
}

pub fn dataframe_to_table(df: &DataFrame) -> XlsxResult<Table> {
    let mut columns = HashMap::new();
    for col in &df.columns {
        columns.insert(col.name.clone(), series_to_column(col)?);
    }
    Table::from_columns(columns)
}

fn column_to_series(name: &str, col: &[CellValue]) -> XlsxResult<Series> {
    if col.is_empty() {
        return Ok(Series::from_str(name, &[] as &[&str]));
    }
    let all_int = col
        .iter()
        .all(|c| matches!(c, CellValue::Int(_) | CellValue::Empty));
    let all_float = col.iter().all(|c| {
        matches!(
            c,
            CellValue::Float(_) | CellValue::Int(_) | CellValue::Empty
        )
    });
    let all_bool = col
        .iter()
        .all(|c| matches!(c, CellValue::Bool(_) | CellValue::Empty));

    if all_bool && col.iter().any(|c| matches!(c, CellValue::Bool(_))) {
        let mut data = Vec::with_capacity(col.len());
        let mut valid = Validity::all_valid(col.len());
        for (i, c) in col.iter().enumerate() {
            match c {
                CellValue::Bool(b) => data.push(*b),
                CellValue::Empty => {
                    data.push(false);
                    valid.set_null(i);
                }
                _ => {}
            }
        }
        return Series::from_bool(name, data)
            .with_validity(valid)
            .map_err(|e| XlsxError::Shape(e.to_string()));
    }
    if all_int && col.iter().any(|c| matches!(c, CellValue::Int(_))) {
        let mut data = Vec::with_capacity(col.len());
        let mut valid = Validity::all_valid(col.len());
        for (i, c) in col.iter().enumerate() {
            match c {
                CellValue::Int(n) => data.push(*n),
                CellValue::Empty => {
                    data.push(0);
                    valid.set_null(i);
                }
                _ => {}
            }
        }
        return Series::from_i64(name, data)
            .with_validity(valid)
            .map_err(|e| XlsxError::Shape(e.to_string()));
    }
    if all_float
        && col
            .iter()
            .any(|c| matches!(c, CellValue::Float(_) | CellValue::Int(_)))
    {
        let mut data = Vec::with_capacity(col.len());
        let mut valid = Validity::all_valid(col.len());
        for (i, c) in col.iter().enumerate() {
            match c {
                CellValue::Float(f) => data.push(*f),
                CellValue::Int(n) => data.push(*n as f64),
                CellValue::Empty => {
                    data.push(0.0);
                    valid.set_null(i);
                }
                _ => {}
            }
        }
        return Series::from_f64(name, data)
            .with_validity(valid)
            .map_err(|e| XlsxError::Shape(e.to_string()));
    }

    let mut strings = Vec::with_capacity(col.len());
    let mut valid = Validity::all_valid(col.len());
    for (i, c) in col.iter().enumerate() {
        match c {
            CellValue::Empty => {
                strings.push(String::new());
                valid.set_null(i);
            }
            other => strings.push(other.as_display_string()),
        }
    }
    let refs: Vec<&str> = strings.iter().map(|s| s.as_str()).collect();
    Series::from_str(name, &refs)
        .with_validity(valid)
        .map_err(|e| XlsxError::Shape(e.to_string()))
}

fn series_to_column(series: &Series) -> XlsxResult<Vec<CellValue>> {
    let n = series.len();
    let mut out = Vec::with_capacity(n);
    match series.dtype() {
        Dtype::I64 | Dtype::Date => {
            let data = match &series.data {
                ColumnData::I64(v) => v,
                ColumnData::Date(v) => v,
                _ => return Err(XlsxError::Shape("expected i64/date".into())),
            };
            for i in 0..n {
                if series.validity.is_valid(i) {
                    out.push(CellValue::Int(data[i]));
                } else {
                    out.push(CellValue::Empty);
                }
            }
        }
        Dtype::F64 => {
            let data = match &series.data {
                ColumnData::F64(v) => v,
                _ => return Err(XlsxError::Shape("expected f64".into())),
            };
            for i in 0..n {
                if series.validity.is_valid(i) {
                    out.push(CellValue::Float(data[i]));
                } else {
                    out.push(CellValue::Empty);
                }
            }
        }
        Dtype::Bool => {
            let data = match &series.data {
                ColumnData::Bool(v) => v,
                _ => return Err(XlsxError::Shape("expected bool".into())),
            };
            for i in 0..n {
                if series.validity.is_valid(i) {
                    out.push(CellValue::Bool(data[i]));
                } else {
                    out.push(CellValue::Empty);
                }
            }
        }
        Dtype::Str => {
            let data = match &series.data {
                ColumnData::Str(v) => v,
                _ => return Err(XlsxError::Shape("expected str".into())),
            };
            for i in 0..n {
                if series.validity.is_valid(i) {
                    out.push(CellValue::String(data.get(i).to_string()));
                } else {
                    out.push(CellValue::Empty);
                }
            }
        }
    }
    Ok(out)
}

fn infer_column_types(
    columns: HashMap<String, Vec<CellValue>>,
) -> XlsxResult<HashMap<String, Vec<CellValue>>> {
    let mut out = HashMap::new();
    for (name, col) in columns {
        let non_empty: Vec<_> = col.iter().filter(|c| !c.is_empty()).cloned().collect();
        if non_empty.is_empty() {
            out.insert(name, col);
            continue;
        }
        let all_int = non_empty.iter().all(|c| matches!(c, CellValue::Int(_)));
        let all_float = non_empty
            .iter()
            .all(|c| matches!(c, CellValue::Int(_) | CellValue::Float(_)));
        let all_bool = non_empty.iter().all(|c| matches!(c, CellValue::Bool(_)));
        if all_bool {
            out.insert(
                name,
                col.into_iter()
                    .map(|c| match c {
                        CellValue::String(s) if s == "true" || s == "false" => {
                            CellValue::Bool(s == "true")
                        }
                        other => other,
                    })
                    .collect(),
            );
        } else if all_int {
            out.insert(
                name,
                col.into_iter()
                    .map(|c| match c {
                        CellValue::Float(f) if (f.fract()).abs() < f64::EPSILON => {
                            CellValue::Int(f as i64)
                        }
                        other => other,
                    })
                    .collect(),
            );
        } else if all_float {
            out.insert(
                name,
                col.into_iter()
                    .map(|c| match c {
                        CellValue::Int(n) => CellValue::Float(n as f64),
                        other => other,
                    })
                    .collect(),
            );
        } else {
            out.insert(
                name,
                col.into_iter()
                    .map(|c| {
                        if c.is_empty() {
                            CellValue::Empty
                        } else {
                            CellValue::String(c.as_display_string())
                        }
                    })
                    .collect(),
            );
        }
    }
    Ok(out)
}

pub fn sheet_to_row_arrays(sheet: &SheetData) -> Vec<Vec<CellValue>> {
    sheet.rows.clone()
}
