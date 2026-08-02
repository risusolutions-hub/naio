//! Unicode correctness for Niao — normalization, grapheme clusters, UCD
//! properties, display width, and casefold (~Python `unicodedata` + `grapheme`).

mod grapheme;
mod normalize;
mod properties;
mod width;

pub use grapheme::{
    casefold, char_len, chars, grapheme_at, grapheme_byte_offsets, grapheme_len, grapheme_slice,
    graphemes,
};
pub use normalize::{is_normalized, nfc, nfd, nfkc, nfkd, normalize, NormalizationForm};
pub use properties::{
    bidi, categories, category, combining, decimal, decomposition, digit, east_asian_width,
    is_alphabetic, is_control, is_numeric, is_whitespace, lookup, mirrored, name, numeric, script,
};
pub use width::{display_width, truncate_width};

use niao_parallel::map as par_map;

/// Parallel NFC (or other form) over a slice of strings.
pub fn parallel_normalize(
    items: &[String],
    form: NormalizationForm,
    threads: usize,
) -> Vec<String> {
    par_map(items, threads, |s| normalize(s, form))
}

/// Parallel display-width measurement.
pub fn parallel_display_width(items: &[String], threads: usize) -> Vec<usize> {
    par_map(items, threads, |s| display_width(s))
}

/// Parallel casefold.
pub fn parallel_casefold(items: &[String], threads: usize) -> Vec<String> {
    par_map(items, threads, |s| casefold(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nfc_e_acute() {
        let composed = "é";
        let decomposed = "e\u{0301}";
        assert_eq!(nfc(decomposed), composed);
        assert_eq!(nfd(composed), decomposed);
    }

    #[test]
    fn grapheme_flag() {
        assert_eq!(grapheme_len("🇺🇸"), 1);
        assert_eq!(graphemes("🇺🇸"), vec!["🇺🇸".to_string()]);
    }

    #[test]
    fn display_width_cjk() {
        assert_eq!(display_width("你好"), 4);
        assert_eq!(display_width("ab"), 2);
    }

    #[test]
    fn category_and_name() {
        assert_eq!(category('A').unwrap(), "Lu");
        assert_eq!(name('A').unwrap(), "LATIN CAPITAL LETTER A");
        assert_eq!(lookup("LATIN CAPITAL LETTER A").unwrap(), 'A');
    }

    #[test]
    fn parallel_normalize_batch() {
        let items = vec!["e\u{0301}".to_string(); 100];
        let out = parallel_normalize(&items, NormalizationForm::Nfc, 4);
        assert!(out.iter().all(|s| s == "é"));
    }
}
