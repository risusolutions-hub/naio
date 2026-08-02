use crate::error::WhenError;
use crate::options::ParseOptions;
use crate::parser::parse;

/// A date substring found inside larger text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub unix_ms: i64,
    pub text: String,
    pub start: usize,
    pub end: usize,
}

/// Scan `text` for parseable date/time substrings (~dateparser.search_dates subset).
///
/// >>> use niao_when::{search, options::ParseOptions};
/// >>> let hits = search("meet next friday at 5pm please", &ParseOptions::default()).unwrap();
/// >>> !hits.is_empty()
/// true
pub fn search(text: &str, opts: &ParseOptions) -> Result<Vec<SearchHit>, WhenError> {
    if text.trim().is_empty() {
        return Err(WhenError::Empty);
    }
    let mut hits = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        while i < bytes.len() && !bytes[i].is_ascii_alphanumeric() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let start = i;
        let mut end = i;
        let mut words = 0usize;
        while end < bytes.len() && words < 8 {
            while end < bytes.len()
                && (bytes[end].is_ascii_alphanumeric()
                    || bytes[end] == b':'
                    || bytes[end] == b'/'
                    || bytes[end] == b'-'
                    || bytes[end] == b'.')
            {
                end += 1;
            }
            words += 1;
            if end >= bytes.len() {
                break;
            }
            if bytes[end].is_ascii_whitespace() {
                end += 1;
                while end < bytes.len() && bytes[end].is_ascii_whitespace() {
                    end += 1;
                }
            } else {
                break;
            }
        }
        let slice = &text[start..end.min(text.len())];
        if slice.len() >= 2 {
            if let Ok(parsed) = parse(slice.trim(), opts) {
                hits.push(SearchHit {
                    unix_ms: parsed.unix_ms,
                    text: slice.trim().to_string(),
                    start,
                    end: start + slice.trim().len(),
                });
                i = end;
                continue;
            }
        }
        i = start + 1;
    }
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::ParseOptions;

    #[test]
    fn finds_phrase() {
        let hits = search("see you tomorrow at 3pm", &ParseOptions::default()).unwrap();
        assert!(!hits.is_empty());
    }
}
