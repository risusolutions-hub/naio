//! Prefix / range scan helpers.

/// One key/value pair returned from a scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanPair {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

/// Options controlling ordered iteration.
#[derive(Debug, Clone, Default)]
pub struct ScanOptions {
    /// Inclusive lower bound (lexicographic).
    pub start: Option<Vec<u8>>,
    /// Exclusive upper bound (lexicographic), unless `end_inclusive`.
    pub end: Option<Vec<u8>>,
    /// When true, `end` is included in the result.
    pub end_inclusive: bool,
    /// Restrict to keys with this byte prefix (combined with start/end).
    pub prefix: Option<Vec<u8>>,
    /// Maximum number of pairs to return (`None` = unbounded).
    pub limit: Option<usize>,
    /// Iterate largest-to-smallest.
    pub reverse: bool,
}

/// Successor of `prefix` for half-open range scans: `[prefix, prefix_end)`.
///
/// Returns `None` when every byte is `0xFF` (unbounded upper end).
pub fn prefix_end(prefix: &[u8]) -> Option<Vec<u8>> {
    let mut end = prefix.to_vec();
    while let Some(b) = end.last_mut() {
        if *b < 0xFF {
            *b += 1;
            return Some(end);
        }
        end.pop();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_end_basic() {
        assert_eq!(prefix_end(b"abc"), Some(b"abd".to_vec()));
        assert_eq!(prefix_end(b"ab\xff"), Some(b"ac".to_vec()));
        assert_eq!(prefix_end(b"\xff\xff"), None);
        assert_eq!(prefix_end(b""), None);
    }
}
