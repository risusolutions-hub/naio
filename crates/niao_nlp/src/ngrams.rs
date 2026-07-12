//! Word, character, and skip n-grams.

/// Word or token n-grams as joined strings (sklearn-style for vectorizers).
pub fn ngrams(tokens: &[String], n: usize) -> Vec<String> {
    if n == 0 || tokens.len() < n {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(tokens.len() + 1 - n);
    for i in 0..=tokens.len() - n {
        out.push(tokens[i..i + n].join(" "));
    }
    out
}

/// Character n-grams over a string (spaces preserved unless stripped).
pub fn char_ngrams(text: &str, n: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if n == 0 || chars.len() < n {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(chars.len() + 1 - n);
    for i in 0..=chars.len() - n {
        out.push(chars[i..i + n].iter().collect());
    }
    out
}

/// Skip-grams: tokens within window with `skip` gaps.
pub fn skip_grams(tokens: &[String], n: usize, skip: usize) -> Vec<String> {
    if n == 0 || tokens.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for i in 0..tokens.len() {
        let mut gram = vec![tokens[i].clone()];
        let mut pos = i;
        for _ in 1..n {
            pos += skip + 1;
            if pos >= tokens.len() {
                break;
            }
            gram.push(tokens[pos].clone());
        }
        if gram.len() == n {
            out.push(gram.join(" "));
        }
    }
    out
}

/// Generate n-grams for a range `[min_n, max_n]` inclusive.
pub fn ngram_range(tokens: &[String], min_n: usize, max_n: usize) -> Vec<String> {
    let mut out = Vec::new();
    for n in min_n..=max_n {
        out.extend(ngrams(tokens, n));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_bigrams() {
        let toks = vec!["a".into(), "b".into(), "c".into()];
        assert_eq!(ngrams(&toks, 2), vec!["a b", "b c"]);
    }

    #[test]
    fn char_trigrams() {
        assert_eq!(char_ngrams("abc", 2), vec!["ab", "bc"]);
    }

    #[test]
    fn skip_gram_basic() {
        let toks = vec!["a".into(), "b".into(), "c".into(), "d".into()];
        assert_eq!(skip_grams(&toks, 2, 1), vec!["a c", "b d"]);
    }
}
