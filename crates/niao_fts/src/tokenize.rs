//! Unicode-aware tokenizer: lowercase, split on non-alphanumeric runs.

/// Tokenize `text` into lowercase alphanumeric tokens (Unicode letters + digits).
///
/// Empty input yields an empty vec. Positions are 0-based within the token stream.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            for c in ch.to_lowercase() {
                cur.push(c);
            }
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Tokenize and also return each token's position in the stream.
pub fn tokenize_with_positions(text: &str) -> Vec<(String, u32)> {
    tokenize(text)
        .into_iter()
        .enumerate()
        .map(|(i, t)| (t, i as u32))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_whitespace() {
        assert!(tokenize("").is_empty());
        assert!(tokenize("   \t\n").is_empty());
    }

    #[test]
    fn basic_ascii() {
        assert_eq!(
            tokenize("Hello, World!"),
            vec!["hello".to_string(), "world".to_string()]
        );
    }

    #[test]
    fn unicode_letters() {
        assert_eq!(
            tokenize("café naïve"),
            vec!["café".to_string(), "naïve".to_string()]
        );
    }

    #[test]
    fn digits_kept() {
        assert_eq!(tokenize("rfc822"), vec!["rfc822".to_string()]);
    }
}
