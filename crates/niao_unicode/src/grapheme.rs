use unicode_segmentation::UnicodeSegmentation;

/// Split `s` into extended grapheme clusters (user-perceived characters).
pub fn graphemes(s: &str) -> Vec<String> {
    s.graphemes(true).map(str::to_string).collect()
}

/// Number of extended grapheme clusters in `s`.
#[inline]
pub fn grapheme_len(s: &str) -> usize {
    s.grapheme_indices(true).count()
}

/// Grapheme cluster at zero-based index, or `None` when out of range.
pub fn grapheme_at(s: &str, index: usize) -> Option<String> {
    s.grapheme_indices(true)
        .nth(index)
        .map(|(_, g)| g.to_string())
}

/// Inclusive-start, exclusive-end grapheme slice.
pub fn grapheme_slice(s: &str, start: usize, end: Option<usize>) -> Option<String> {
    let gs: Vec<&str> = s.graphemes(true).collect();
    if start > gs.len() {
        return None;
    }
    let end = end.unwrap_or(gs.len()).min(gs.len());
    if end < start {
        return None;
    }
    Some(gs[start..end].concat())
}

/// Unicode scalar values as single-character strings.
pub fn chars(s: &str) -> Vec<String> {
    s.chars().map(|c| c.to_string()).collect()
}

#[inline]
pub fn char_len(s: &str) -> usize {
    s.chars().count()
}

/// Casefold mapping (case-insensitive comparison / search).
pub fn casefold(s: &str) -> String {
    s.chars().flat_map(|c| c.to_lowercase()).collect()
}

/// Byte offset of each grapheme cluster start (for slicing / indexing).
pub fn grapheme_byte_offsets(s: &str) -> Vec<usize> {
    s.grapheme_indices(true).map(|(i, _)| i).collect()
}
