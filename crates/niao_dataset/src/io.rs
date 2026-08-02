//! CSV / JSON / JSONL loaders.

use crate::dataset::Dataset;
use crate::error::{DatasetError, DatasetResult};
use niao_frame::{parse_json_records, read_csv, read_json, CsvOptions};
use std::fs;
use std::path::Path;

/// Load CSV with optional header row.
///
/// // >>> use niao_dataset::load_csv;
/// // (see integration tests for file IO)
pub fn load_csv(path: impl AsRef<Path>, header: bool, delimiter: char) -> DatasetResult<Dataset> {
    let opts = CsvOptions { header, delimiter };
    let frame = read_csv(path, opts).map_err(DatasetError::from)?;
    Ok(Dataset::new(frame))
}

/// Load JSON array-of-objects file.
pub fn load_json(path: impl AsRef<Path>) -> DatasetResult<Dataset> {
    let frame = read_json(path).map_err(DatasetError::from)?;
    Ok(Dataset::new(frame))
}

/// Load newline-delimited JSON (one object per line).
pub fn load_jsonl(path: impl AsRef<Path>) -> DatasetResult<Dataset> {
    let text = fs::read_to_string(path.as_ref())
        .map_err(|e| DatasetError::Error(format!("read jsonl: {e}")))?;
    parse_jsonl_text(&text)
}

/// Parse JSONL from in-memory text.
pub fn parse_jsonl_text(text: &str) -> DatasetResult<Dataset> {
    let mut objects: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if !line.starts_with('{') {
            return Err(DatasetError::Error(format!(
                "jsonl line must be a JSON object, got: {line}"
            )));
        }
        objects.push(line.to_string());
    }
    if objects.is_empty() {
        return Ok(Dataset::new(niao_frame::DataFrame::empty()));
    }
    let wrapped = format!("[{}]", objects.join(","));
    let frame = parse_json_records(&wrapped).map_err(DatasetError::from)?;
    Ok(Dataset::new(frame))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_jsonl_inline() {
        let text = r#"{"x":1,"y":"a"}
{"x":2,"y":"b"}"#;
        let ds = parse_jsonl_text(text).unwrap();
        assert_eq!(ds.len(), 2);
        assert!(ds.columns().len() >= 2);
    }

    #[test]
    fn parse_jsonl_skips_blank_and_comments() {
        let text = "# comment\n\n{\"a\":1}\n";
        let ds = parse_jsonl_text(text).unwrap();
        assert_eq!(ds.len(), 1);
    }
}
