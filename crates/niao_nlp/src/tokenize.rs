//! Word and sentence tokenization (regex + rules). Subword/BPE via `ntok` is runtime-only.

use crate::normalize::{lowercase, NormalizeOptions};

static ABBREV: &[&str] = &[
    "mr.", "mrs.", "ms.", "dr.", "prof.", "sr.", "jr.", "vs.", "etc.", "e.g.", "i.e.",
];

/// sklearn default token pattern: words with at least 2 word chars.
pub fn word_tokenize(text: &str, lowercase_input: bool) -> Vec<String> {
    let s = if lowercase_input {
        lowercase(text)
    } else {
        text.to_string()
    };
    let bytes = s.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && !is_word_char(bytes[i]) {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let start = i;
        while i < bytes.len() && is_word_char(bytes[i]) {
            i += 1;
        }
        if i - start >= 2 {
            tokens.push(s[start..i].to_string());
        }
    }
    tokens
}

#[inline]
fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Whitespace tokenizer (no length filter).
pub fn whitespace_tokenize(text: &str) -> Vec<&str> {
    text.split_whitespace().collect()
}

/// Sentence splitter with abbreviation handling.
pub fn sent_tokenize(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let mut sentences = Vec::new();
    let mut start = 0;
    let chars: Vec<char> = text.chars().collect();
    let lower_chars: Vec<char> = lower.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '.' || c == '!' || c == '?' {
            let end = i + 1;
            let slice_lower: String = lower_chars[start..end].iter().collect();
            let is_abbrev = ABBREV.iter().any(|a| slice_lower.ends_with(a));
            let next_is_lower = chars
                .get(end)
                .map(|nc| nc.is_lowercase())
                .unwrap_or(false);
            if !is_abbrev && (!next_is_lower || c != '.') {
                let sent: String = chars[start..end].iter().collect();
                let trimmed = sent.trim();
                if !trimmed.is_empty() {
                    sentences.push(trimmed.to_string());
                }
                start = end;
                while start < chars.len() && chars[start].is_whitespace() {
                    start += 1;
                }
                i = start;
                continue;
            }
        }
        i += 1;
    }
    if start < chars.len() {
        let sent: String = chars[start..].iter().collect();
        let trimmed = sent.trim();
        if !trimmed.is_empty() {
            sentences.push(trimmed.to_string());
        }
    }
    if sentences.is_empty() && !text.trim().is_empty() {
        sentences.push(text.trim().to_string());
    }
    sentences
}

/// Tokenize with optional normalization (vectorizer path).
pub fn tokenize_for_vectorizer(text: &str, opts: &NormalizeOptions) -> Vec<String> {
    let norm = crate::normalize::normalize(text, opts);
    word_tokenize(&norm, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_tokens_sklearn_pattern() {
        let toks = word_tokenize("The cat sat on the mat.", true);
        assert_eq!(toks, vec!["the", "cat", "sat", "on", "the", "mat"]);
    }

    #[test]
    fn sentence_split_abbrev() {
        let sents = sent_tokenize("Dr. Smith went home. He was tired.");
        assert_eq!(sents.len(), 2);
        assert!(sents[0].starts_with("Dr."));
    }
}
