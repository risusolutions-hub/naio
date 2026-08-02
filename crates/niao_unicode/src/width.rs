use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// East-Asian-aware terminal display width (fullwidth = 2 columns).
#[inline]
pub fn display_width(s: &str) -> usize {
    s.width()
}

/// Truncate to at most `max_width` display columns, appending `suffix` when shortened.
pub fn truncate_width(s: &str, max_width: usize, suffix: &str) -> String {
    if max_width == 0 {
        return String::new();
    }
    let suffix_w = suffix.width();
    if suffix_w >= max_width {
        return suffix.chars().take(1).collect();
    }
    let budget = max_width.saturating_sub(suffix_w);
    let mut out = String::new();
    let mut used = 0usize;
    for g in s.graphemes(true) {
        let w = g.width();
        if used + w > budget {
            out.push_str(suffix);
            return out;
        }
        used += w;
        out.push_str(g);
    }
    out
}
