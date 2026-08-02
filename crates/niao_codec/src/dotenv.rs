//! `.env` file parser and process loader.

use std::env;
use std::io::{BufRead, Read};
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub enum DotenvError {
    Io(String),
    InvalidLine(String),
}

impl std::fmt::Display for DotenvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::InvalidLine(l) => write!(f, "invalid dotenv line: {l}"),
        }
    }
}

impl std::error::Error for DotenvError {}

/// Parse dotenv text into key/value pairs (does not modify process env).
pub fn parse_dotenv(text: &str) -> Result<Vec<(String, String)>, DotenvError> {
    parse_dotenv_reader(text.as_bytes())
}

/// Parse dotenv from any byte reader.
pub fn parse_dotenv_reader<R: Read>(reader: R) -> Result<Vec<(String, String)>, DotenvError> {
    let mut pairs = Vec::new();
    let mut buf = std::io::BufReader::new(reader);
    let mut line = String::new();
    loop {
        line.clear();
        let n = buf
            .read_line(&mut line)
            .map_err(|e| DotenvError::Io(e.to_string()))?;
        if n == 0 {
            break;
        }
        if line.ends_with('\n') {
            line.pop();
            if line.ends_with('\r') {
                line.pop();
            }
        }
        if let Some((k, v)) = parse_line(&line)? {
            pairs.push((k, v));
        }
    }
    Ok(pairs)
}

/// Load `.env` from the current directory or ancestors (first found), applying to process env.
pub fn load_dotenv() -> Result<Vec<(String, String)>, DotenvError> {
    let path = find_dotenv(".env").ok_or_else(|| DotenvError::Io(".env not found".into()))?;
    load_dotenv_file(&path)
}

/// Load a specific `.env` file into process env (does not override existing vars).
pub fn load_dotenv_file(path: &Path) -> Result<Vec<(String, String)>, DotenvError> {
    let pairs = parse_dotenv_file(path)?;
    apply_pairs(&pairs, false);
    Ok(pairs)
}

/// Parse a `.env` file without applying.
pub fn parse_dotenv_file(path: &Path) -> Result<Vec<(String, String)>, DotenvError> {
    let file = std::fs::File::open(path).map_err(|e| DotenvError::Io(e.to_string()))?;
    parse_dotenv_reader(file)
}

/// Apply pairs to process environment.
pub fn apply_pairs(pairs: &[(String, String)], override_existing: bool) -> usize {
    let mut count = 0;
    for (k, v) in pairs {
        if override_existing || env::var(k).is_err() {
            env::set_var(k, v);
            count += 1;
        }
    }
    count
}

fn find_dotenv(name: &str) -> Option<std::path::PathBuf> {
    let mut dir = env::current_dir().ok()?;
    loop {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

fn parse_line(line: &str) -> Result<Option<(String, String)>, DotenvError> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Ok(None);
    }
    let mut rest = trimmed;
    if rest.starts_with("export ") {
        rest = rest["export ".len()..].trim_start();
    }
    let Some((key, raw_value)) = rest.split_once('=') else {
        return Err(DotenvError::InvalidLine(line.to_string()));
    };
    let key = key.trim();
    if key.is_empty() || !valid_key(key) {
        return Err(DotenvError::InvalidLine(line.to_string()));
    }
    let value = parse_value(raw_value.trim_start())?;
    Ok(Some((key.to_string(), value)))
}

fn valid_key(key: &str) -> bool {
    key.as_bytes()
        .first()
        .map(|b| b.is_ascii_alphabetic() || *b == b'_')
        .unwrap_or(false)
        && key
            .as_bytes()
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

fn parse_value(raw: &str) -> Result<String, DotenvError> {
    if raw.is_empty() {
        return Ok(String::new());
    }
    let bytes = raw.as_bytes();
    match bytes[0] {
        b'"' => parse_quoted(raw, b'"'),
        b'\'' => parse_quoted(raw, b'\''),
        _ => Ok(raw.to_string()),
    }
}

fn parse_quoted(raw: &str, quote: u8) -> Result<String, DotenvError> {
    if raw.as_bytes().last().copied() != Some(quote) || raw.len() < 2 {
        return Err(DotenvError::InvalidLine(raw.to_string()));
    }
    let inner = &raw[1..raw.len() - 1];
    let mut out = String::with_capacity(inner.len());
    let bytes = inner.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && quote == b'"' {
            if i + 1 >= bytes.len() {
                return Err(DotenvError::InvalidLine(raw.to_string()));
            }
            let esc = bytes[i + 1];
            out.push(match esc {
                b'n' => '\n',
                b'r' => '\r',
                b't' => '\t',
                b'\\' => '\\',
                b'"' => '"',
                other => other as char,
            });
            i += 2;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_and_export() {
        let text = "FOO=bar\n# comment\nexport BAZ=qux\n";
        let pairs = parse_dotenv(text).unwrap();
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0], ("FOO".to_string(), "bar".to_string()));
        assert_eq!(pairs[1], ("BAZ".to_string(), "qux".to_string()));
    }

    #[test]
    fn quoted_and_escapes() {
        let text = "A=\"hello\\nworld\"\nB='single'\nC=\"\"\n";
        let pairs = parse_dotenv(text).unwrap();
        assert_eq!(pairs[0].1, "hello\nworld");
        assert_eq!(pairs[1].1, "single");
        assert_eq!(pairs[2].1, "");
    }

    #[test]
    fn crlf() {
        let text = "X=1\r\nY=two\r\n";
        let pairs = parse_dotenv(text).unwrap();
        assert_eq!(
            pairs,
            vec![
                ("X".to_string(), "1".to_string()),
                ("Y".to_string(), "two".to_string())
            ]
        );
    }

    #[test]
    fn empty_value() {
        let pairs = parse_dotenv("EMPTY=\nKEY=val\n").unwrap();
        assert_eq!(pairs[0].1, "");
    }
}
