//! Reshape: concat, melt, pivot, explode.

use crate::dataframe::DataFrame;
use crate::error::{FrameError, FrameResult};
use crate::series::{ColumnData, Series, StringColumn};
use crate::validity::Validity;
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub fn concat(frames: &[&DataFrame], axis: usize) -> FrameResult<DataFrame> {
    if frames.is_empty() {
        return Ok(DataFrame::empty());
    }
    if axis == 0 {
        concat_vertical(frames)
    } else if axis == 1 {
        concat_horizontal(frames)
    } else {
        Err(FrameError::Error("concat axis must be 0 or 1".into()))
    }
}

fn concat_vertical(frames: &[&DataFrame]) -> FrameResult<DataFrame> {
    let names = frames[0].column_names();
    for f in &frames[1..] {
        if f.column_names() != names {
            return Err(FrameError::Error(
                "concat axis=0 requires matching column names".into(),
            ));
        }
    }
    let mut cols = Vec::new();
    for name in &names {
        let mut series = frames[0].get(name)?.clone();
        for f in &frames[1..] {
            series = append_series(&series, f.get(name)?)?;
        }
        cols.push(series);
    }
    DataFrame::new(cols)
}

fn concat_horizontal(frames: &[&DataFrame]) -> FrameResult<DataFrame> {
    let nrows = frames[0].nrows();
    for f in &frames[1..] {
        if f.nrows() != nrows {
            return Err(FrameError::LengthMismatch(
                "concat axis=1 requires equal row counts".into(),
            ));
        }
    }
    let mut cols = Vec::new();
    let mut seen = HashMap::new();
    for f in frames {
        for c in &f.columns {
            let mut name = c.name.clone();
            if seen.contains_key(&name) {
                name = format!("{}_1", name);
            }
            seen.insert(name.clone(), ());
            let mut s = c.clone();
            s.name = name;
            cols.push(s);
        }
    }
    DataFrame::new(cols)
}

fn append_series(a: &Series, b: &Series) -> FrameResult<Series> {
    if a.dtype() != b.dtype() {
        return Err(FrameError::Dtype(format!(
            "concat dtype mismatch on '{}'",
            a.name
        )));
    }
    let mut validity = Validity::all_valid(a.len() + b.len());
    for i in 0..a.len() {
        if a.validity.is_null(i) {
            validity.set_null(i);
        }
    }
    for i in 0..b.len() {
        if b.validity.is_null(i) {
            validity.set_null(a.len() + i);
        }
    }
    let data = match (&a.data, &b.data) {
        (ColumnData::I64(x), ColumnData::I64(y)) => {
            ColumnData::I64([x.as_slice(), y.as_slice()].concat())
        }
        (ColumnData::F64(x), ColumnData::F64(y)) => {
            ColumnData::F64([x.as_slice(), y.as_slice()].concat())
        }
        (ColumnData::Bool(x), ColumnData::Bool(y)) => {
            ColumnData::Bool([x.as_slice(), y.as_slice()].concat())
        }
        (ColumnData::Date(x), ColumnData::Date(y)) => {
            ColumnData::Date([x.as_slice(), y.as_slice()].concat())
        }
        (ColumnData::Str(x), ColumnData::Str(y)) => ColumnData::Str(x.concat(y)),
        _ => return Err(FrameError::Dtype("concat incompatible".into())),
    };
    Ok(Series {
        name: a.name.clone(),
        data,
        validity,
    })
}

/// Wide → long: id_vars kept, value_vars melted into variable/value columns.
pub fn melt(df: &DataFrame, id_vars: &[&str], value_vars: Option<&[&str]>) -> FrameResult<DataFrame> {
    for id in id_vars {
        let _ = df.get(id)?;
    }
    let value_names: Vec<String> = if let Some(vv) = value_vars {
        for v in vv {
            let _ = df.get(v)?;
        }
        vv.iter().map(|s| (*s).to_string()).collect()
    } else {
        let id_set: BTreeSet<&str> = id_vars.iter().copied().collect();
        df.column_names()
            .into_iter()
            .filter(|n| !id_set.contains(n.as_str()))
            .collect()
    };
    if value_names.is_empty() {
        return Err(FrameError::Error("melt: no value columns".into()));
    }

    let n = df.nrows();
    let m = value_names.len();
    let out_n = n * m;

    let mut id_cols: Vec<Series> = Vec::new();
    for id in id_vars {
        let src = df.get(id)?;
        let mut indices = Vec::with_capacity(out_n);
        for _ in 0..m {
            for i in 0..n {
                indices.push(i);
            }
        }
        let mut s = src.take(&indices);
        s.name = (*id).to_string();
        id_cols.push(s);
    }

    let mut var_col = StringColumn::new();
    for name in &value_names {
        for _ in 0..n {
            var_col.push(name);
        }
    }
    id_cols.push(Series::new("variable", ColumnData::Str(var_col)));

    // Promote all value cols to f64 when possible, else string
    let all_numeric = value_names.iter().all(|n| {
        matches!(
            df.get(n).map(|s| s.dtype()),
            Ok(crate::series::Dtype::I64)
                | Ok(crate::series::Dtype::F64)
                | Ok(crate::series::Dtype::Date)
                | Ok(crate::series::Dtype::Bool)
        )
    });

    if all_numeric {
        let mut vals = Vec::with_capacity(out_n);
        let mut validity = Validity::all_valid(out_n);
        let mut j = 0;
        for name in &value_names {
            let s = df.get(name)?;
            let f = s.to_f64_vec()?;
            for i in 0..n {
                if s.validity.is_null(i) {
                    vals.push(f64::NAN);
                    validity.set_null(j);
                } else {
                    vals.push(f[i]);
                }
                j += 1;
            }
        }
        id_cols.push(Series::from_f64("value", vals).with_validity(validity)?);
    } else {
        let mut sc = StringColumn::new();
        let mut validity = Validity::all_valid(out_n);
        let mut j = 0;
        for name in &value_names {
            let s = df.get(name)?;
            for i in 0..n {
                if s.validity.is_null(i) {
                    sc.push("");
                    validity.set_null(j);
                } else {
                    match &s.data {
                        ColumnData::Str(v) => sc.push(v.get(i)),
                        ColumnData::I64(v) | ColumnData::Date(v) => sc.push(&v[i].to_string()),
                        ColumnData::F64(v) => sc.push(&v[i].to_string()),
                        ColumnData::Bool(v) => sc.push(if v[i] { "true" } else { "false" }),
                    }
                }
                j += 1;
            }
        }
        id_cols.push(
            Series::new("value", ColumnData::Str(sc)).with_validity(validity)?,
        );
    }

    DataFrame::new(id_cols)
}

/// Pivot: index × columns → values (mean aggregation for duplicates).
pub fn pivot(
    df: &DataFrame,
    index: &str,
    columns: &str,
    values: &str,
) -> FrameResult<DataFrame> {
    let _ = df.get(index)?;
    let col_s = df.get(columns)?;
    let val_s = df.get(values)?;

    let mut col_labels: BTreeSet<String> = BTreeSet::new();
    for i in 0..df.nrows() {
        if col_s.validity.is_valid(i) {
            col_labels.insert(cell_label(col_s, i));
        }
    }
    let col_labels: Vec<String> = col_labels.into_iter().collect();

    // index key → col_label → sum/count
    let mut map: BTreeMap<String, HashMap<String, (f64, usize)>> = BTreeMap::new();
    let val_f = val_s.to_f64_vec().unwrap_or_else(|_| vec![0.0; df.nrows()]);

    for i in 0..df.nrows() {
        if df.get(index)?.validity.is_null(i) || col_s.validity.is_null(i) || val_s.validity.is_null(i)
        {
            continue;
        }
        let ik = cell_label(df.get(index)?, i);
        let ck = cell_label(col_s, i);
        let e = map.entry(ik).or_default().entry(ck).or_insert((0.0, 0));
        e.0 += val_f[i];
        e.1 += 1;
    }

    let mut idx_col = StringColumn::new();
    let mut out_cols: Vec<Vec<f64>> = col_labels.iter().map(|_| Vec::new()).collect();
    let mut valids: Vec<Validity> = col_labels
        .iter()
        .map(|_| Validity::all_valid(0))
        .collect();

    let n_rows = map.len();
    for v in &mut valids {
        *v = Validity::all_valid(n_rows);
    }
    for c in &mut out_cols {
        c.reserve(n_rows);
    }

    let mut row = 0;
    for (ik, cmap) in &map {
        idx_col.push(ik);
        for (ci, label) in col_labels.iter().enumerate() {
            if let Some(&(sum, cnt)) = cmap.get(label) {
                out_cols[ci].push(sum / cnt as f64);
            } else {
                out_cols[ci].push(f64::NAN);
                valids[ci].set_null(row);
            }
        }
        row += 1;
    }

    let mut series_vec = vec![Series::new(index, ColumnData::Str(idx_col))];
    for (i, label) in col_labels.iter().enumerate() {
        series_vec.push(
            Series::from_f64(label.clone(), out_cols[i].clone()).with_validity(valids[i].clone())?,
        );
    }
    DataFrame::new(series_vec)
}

fn cell_label(s: &Series, i: usize) -> String {
    match &s.data {
        ColumnData::Str(v) => v.get(i).to_string(),
        ColumnData::I64(v) | ColumnData::Date(v) => v[i].to_string(),
        ColumnData::F64(v) => format!("{}", v[i]),
        ColumnData::Bool(v) => if v[i] { "true" } else { "false" }.to_string(),
    }
}

/// Explode list-like string column split by delimiter into rows.
pub fn explode(df: &DataFrame, col: &str, sep: &str) -> FrameResult<DataFrame> {
    let s = df.get(col)?;
    let strs = match &s.data {
        ColumnData::Str(v) => v,
        _ => {
            return Err(FrameError::Dtype(
                "explode requires string column".into(),
            ))
        }
    };
    let mut indices = Vec::new();
    let mut parts: Vec<String> = Vec::new();
    for i in 0..s.len() {
        if s.validity.is_null(i) {
            indices.push(i);
            parts.push(String::new());
            continue;
        }
        let text = strs.get(i);
        if text.is_empty() {
            indices.push(i);
            parts.push(String::new());
        } else {
            for p in text.split(sep) {
                indices.push(i);
                parts.push(p.to_string());
            }
        }
    }
    let mut out = df.take_rows(&indices)?;
    let exploded = Series::from_str(col, &parts);
    // Fix nulls for empty originals that were null
    let mut validity = Validity::all_valid(parts.len());
    for (j, &i) in indices.iter().enumerate() {
        if s.validity.is_null(i) {
            validity.set_null(j);
        }
    }
    let exploded = exploded.with_validity(validity)?;
    out = out.with_column(exploded)?;
    Ok(out)
}
