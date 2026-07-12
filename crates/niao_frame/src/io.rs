//! CSV / JSON IO with dtype inference (stdlib only; mirrors ncsv/njson usage).

use crate::dataframe::DataFrame;
use crate::error::{FrameError, FrameResult};
use crate::series::{ColumnData, Series, StringColumn};
use crate::validity::Validity;
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, Default)]
pub struct CsvOptions {
    pub header: bool,
    pub delimiter: char,
}

impl CsvOptions {
    pub fn with_header() -> Self {
        Self {
            header: true,
            delimiter: ',',
        }
    }
}

pub fn read_csv(path: impl AsRef<Path>, opts: CsvOptions) -> FrameResult<DataFrame> {
    let text = fs::read_to_string(path.as_ref())
        .map_err(|e| FrameError::Error(format!("read_csv: {e}")))?;
    parse_csv(&text, opts)
}

pub fn parse_csv(text: &str, opts: CsvOptions) -> FrameResult<DataFrame> {
    let records = parse_records(text, opts.delimiter)?;
    if records.is_empty() {
        return Ok(DataFrame::empty());
    }
    let (names, data_rows) = if opts.header {
        let names = records[0].clone();
        (names, &records[1..])
    } else {
        let n = records[0].len();
        let names: Vec<String> = (0..n).map(|i| format!("column_{i}")).collect();
        (names, &records[..])
    };
    let ncols = names.len();
    let mut cols: Vec<Vec<String>> = vec![Vec::with_capacity(data_rows.len()); ncols];
    for row in data_rows {
        for (i, cell) in row.iter().enumerate() {
            if i < ncols {
                cols[i].push(cell.clone());
            }
        }
        // pad short rows
        for i in row.len()..ncols {
            cols[i].push(String::new());
        }
    }
    let mut series = Vec::with_capacity(ncols);
    for (name, raw) in names.into_iter().zip(cols) {
        series.push(infer_series(name, &raw)?);
    }
    DataFrame::new(series)
}

pub fn write_csv(path: impl AsRef<Path>, df: &DataFrame, opts: CsvOptions) -> FrameResult<()> {
    let text = to_csv(df, opts)?;
    fs::write(path.as_ref(), text).map_err(|e| FrameError::Error(format!("write_csv: {e}")))
}

pub fn to_csv(df: &DataFrame, opts: CsvOptions) -> FrameResult<String> {
    let delim = opts.delimiter;
    let mut out = String::new();
    if opts.header {
        let names = df.column_names();
        out.push_str(&names.iter().map(|n| escape_csv(n, delim)).collect::<Vec<_>>().join(&delim.to_string()));
        out.push('\n');
    }
    for r in 0..df.nrows() {
        let mut cells = Vec::with_capacity(df.ncols());
        for c in &df.columns {
            cells.push(escape_csv(&cell_to_string(c, r), delim));
        }
        out.push_str(&cells.join(&delim.to_string()));
        out.push('\n');
    }
    Ok(out)
}

fn escape_csv(s: &str, delim: char) -> String {
    if s.contains(delim) || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn cell_to_string(s: &Series, i: usize) -> String {
    if s.validity.is_null(i) {
        return String::new();
    }
    match &s.data {
        ColumnData::I64(v) | ColumnData::Date(v) => v[i].to_string(),
        ColumnData::F64(v) => {
            let x = v[i];
            if x.fract() == 0.0 && x.abs() < 1e15 {
                format!("{}", x as i64)
            } else {
                format!("{x}")
            }
        }
        ColumnData::Bool(v) => if v[i] { "true" } else { "false" }.to_string(),
        ColumnData::Str(v) => v.get(i).to_string(),
    }
}

fn parse_records(text: &str, delim: char) -> FrameResult<Vec<Vec<String>>> {
    let mut records = Vec::new();
    let mut field = String::new();
    let mut row: Vec<String> = Vec::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                if in_quotes {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        field.push('"');
                    } else {
                        in_quotes = false;
                    }
                } else {
                    in_quotes = true;
                }
            }
            ch if ch == delim && !in_quotes => {
                row.push(std::mem::take(&mut field));
            }
            '\n' if !in_quotes => {
                row.push(std::mem::take(&mut field));
                if !(row.len() == 1 && row[0].is_empty() && records.is_empty()) {
                    // skip trailing empty line
                    if !(row.len() == 1 && row[0].is_empty()) {
                        records.push(std::mem::take(&mut row));
                    } else {
                        row.clear();
                    }
                } else {
                    row.clear();
                }
            }
            '\r' if !in_quotes => {
                // ignore; handle \r\n via \n
            }
            _ => field.push(c),
        }
    }
    if in_quotes {
        return Err(FrameError::Error("unclosed quote in CSV".into()));
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        if !(row.len() == 1 && row[0].is_empty()) {
            records.push(row);
        }
    }
    Ok(records)
}

fn infer_series(name: String, raw: &[String]) -> FrameResult<Series> {
    let mut nulls = Vec::new();
    let mut all_bool = true;
    let mut all_int = true;
    let mut all_float = true;
    let mut bools = Vec::with_capacity(raw.len());
    let mut ints = Vec::with_capacity(raw.len());
    let mut floats = Vec::with_capacity(raw.len());

    for (i, s) in raw.iter().enumerate() {
        if s.is_empty() || s.eq_ignore_ascii_case("null") || s.eq_ignore_ascii_case("na") {
            nulls.push(i);
            bools.push(false);
            ints.push(0);
            floats.push(f64::NAN);
            continue;
        }
        let lower = s.to_ascii_lowercase();
        if lower == "true" || lower == "false" {
            bools.push(lower == "true");
            all_int = false;
            all_float = false;
            ints.push(0);
            floats.push(f64::NAN);
        } else if let Ok(n) = s.parse::<i64>() {
            all_bool = false;
            bools.push(false);
            ints.push(n);
            floats.push(n as f64);
        } else if let Ok(f) = s.parse::<f64>() {
            all_bool = false;
            all_int = false;
            bools.push(false);
            ints.push(0);
            floats.push(f);
        } else {
            all_bool = false;
            all_int = false;
            all_float = false;
            bools.push(false);
            ints.push(0);
            floats.push(f64::NAN);
        }
    }

    let mut validity = Validity::all_valid(raw.len());
    for &i in &nulls {
        validity.set_null(i);
    }

    if all_bool && nulls.len() < raw.len() {
        // only bool if every non-null is bool
        let non_null_bool = raw.iter().enumerate().all(|(i, s)| {
            nulls.contains(&i)
                || s.eq_ignore_ascii_case("true")
                || s.eq_ignore_ascii_case("false")
        });
        if non_null_bool {
            return Series::from_bool(name, bools).with_validity(validity);
        }
    }
    if all_int {
        return Series::from_i64(name, ints).with_validity(validity);
    }
    if all_float {
        return Series::from_f64(name, floats).with_validity(validity);
    }
    let mut sc = StringColumn::new();
    for s in raw {
        sc.push(s);
    }
    Series::new(name, ColumnData::Str(sc)).with_validity(validity)
}

/// JSON array of objects → DataFrame. Minimal parser (no third-party).
pub fn read_json(path: impl AsRef<Path>) -> FrameResult<DataFrame> {
    let text = fs::read_to_string(path.as_ref())
        .map_err(|e| FrameError::Error(format!("read_json: {e}")))?;
    parse_json_records(&text)
}

pub fn write_json(path: impl AsRef<Path>, df: &DataFrame) -> FrameResult<()> {
    let text = to_json(df)?;
    fs::write(path.as_ref(), text).map_err(|e| FrameError::Error(format!("write_json: {e}")))
}

pub fn to_json(df: &DataFrame) -> FrameResult<String> {
    let mut out = String::from("[\n");
    for r in 0..df.nrows() {
        if r > 0 {
            out.push_str(",\n");
        }
        out.push('{');
        for (ci, c) in df.columns.iter().enumerate() {
            if ci > 0 {
                out.push(',');
            }
            out.push_str(&format!("\"{}\":", escape_json_str(&c.name)));
            if c.validity.is_null(r) {
                out.push_str("null");
            } else {
                match &c.data {
                    ColumnData::I64(v) | ColumnData::Date(v) => out.push_str(&v[r].to_string()),
                    ColumnData::F64(v) => {
                        if v[r].is_nan() {
                            out.push_str("null");
                        } else {
                            out.push_str(&format!("{}", v[r]));
                        }
                    }
                    ColumnData::Bool(v) => out.push_str(if v[r] { "true" } else { "false" }),
                    ColumnData::Str(v) => {
                        out.push('"');
                        out.push_str(&escape_json_str(v.get(r)));
                        out.push('"');
                    }
                }
            }
        }
        out.push('}');
    }
    out.push_str("\n]");
    Ok(out)
}

fn escape_json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

/// Parse a JSON array of flat objects with string/number/bool/null values.
pub fn parse_json_records(text: &str) -> FrameResult<DataFrame> {
    let text = text.trim();
    if !text.starts_with('[') || !text.ends_with(']') {
        return Err(FrameError::Error(
            "JSON must be an array of objects".into(),
        ));
    }
    let inner = &text[1..text.len() - 1];
    let objects = split_top_level(inner, ',')?;
    if objects.is_empty() || (objects.len() == 1 && objects[0].trim().is_empty()) {
        return Ok(DataFrame::empty());
    }

    let mut rows: Vec<std::collections::HashMap<String, Option<JsonAtom>>> = Vec::new();
    let mut all_keys: Vec<String> = Vec::new();
    let mut key_set = std::collections::HashSet::new();

    for obj in objects {
        let obj = obj.trim();
        if obj.is_empty() {
            continue;
        }
        let map = parse_object(obj)?;
        for k in map.keys() {
            if key_set.insert(k.clone()) {
                all_keys.push(k.clone());
            }
        }
        rows.push(map);
    }

    let mut series_vec = Vec::new();
    for key in &all_keys {
        let raw: Vec<String> = rows
            .iter()
            .map(|m| match m.get(key) {
                Some(Some(JsonAtom::Null)) | Some(None) | None => String::new(),
                Some(Some(JsonAtom::Str(s))) => s.clone(),
                Some(Some(JsonAtom::Num(s))) => s.clone(),
                Some(Some(JsonAtom::Bool(b))) => if *b { "true" } else { "false" }.to_string(),
            })
            .collect();
        series_vec.push(infer_series(key.clone(), &raw)?);
    }
    DataFrame::new(series_vec)
}

#[derive(Clone, Debug)]
enum JsonAtom {
    Null,
    Str(String),
    Num(String),
    Bool(bool),
}

fn parse_object(text: &str) -> FrameResult<std::collections::HashMap<String, Option<JsonAtom>>> {
    let text = text.trim();
    if !text.starts_with('{') || !text.ends_with('}') {
        return Err(FrameError::Error("expected JSON object".into()));
    }
    let inner = &text[1..text.len() - 1];
    let parts = split_top_level(inner, ',')?;
    let mut map = std::collections::HashMap::new();
    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let kv = split_top_level(part, ':')?;
        if kv.len() != 2 {
            return Err(FrameError::Error(format!("bad JSON field: {part}")));
        }
        let key = parse_json_string(kv[0].trim())?;
        let val = parse_atom(kv[1].trim())?;
        map.insert(key, Some(val));
    }
    Ok(map)
}

fn parse_atom(s: &str) -> FrameResult<JsonAtom> {
    if s == "null" {
        return Ok(JsonAtom::Null);
    }
    if s == "true" {
        return Ok(JsonAtom::Bool(true));
    }
    if s == "false" {
        return Ok(JsonAtom::Bool(false));
    }
    if s.starts_with('"') {
        return Ok(JsonAtom::Str(parse_json_string(s)?));
    }
    // number
    if s.parse::<f64>().is_ok() {
        return Ok(JsonAtom::Num(s.to_string()));
    }
    Err(FrameError::Error(format!("unsupported JSON value: {s}")))
}

fn parse_json_string(s: &str) -> FrameResult<String> {
    let s = s.trim();
    if !s.starts_with('"') || !s.ends_with('"') || s.len() < 2 {
        return Err(FrameError::Error(format!("expected string, got {s}")));
    }
    let mut out = String::new();
    let mut chars = s[1..s.len() - 1].chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some(other) => out.push(other),
                None => break,
            }
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

fn split_top_level(s: &str, sep: char) -> FrameResult<Vec<String>> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;
    for c in s.chars() {
        if escape {
            cur.push(c);
            escape = false;
            continue;
        }
        if in_str {
            cur.push(c);
            if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                cur.push(c);
            }
            '{' | '[' => {
                depth += 1;
                cur.push(c);
            }
            '}' | ']' => {
                depth -= 1;
                cur.push(c);
            }
            ch if ch == sep && depth == 0 => {
                out.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    Ok(out)
}
