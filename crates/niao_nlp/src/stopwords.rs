//! English stopword list and removal.

use std::collections::HashSet;

static ENGLISH_STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "by", "for", "from", "has", "he", "in", "is", "it",
    "its", "of", "on", "that", "the", "to", "was", "were", "will", "with", "this", "but", "they",
    "have", "had", "what", "when", "where", "who", "which", "why", "how", "all", "each", "few",
    "more", "most", "other", "some", "such", "no", "nor", "not", "only", "own", "same", "so",
    "than", "too", "very", "can", "just", "don", "should", "now",
];

pub fn english_stopwords() -> HashSet<String> {
    ENGLISH_STOPWORDS.iter().map(|s| s.to_string()).collect()
}

/// Remove tokens present in `stopwords` (typically lowercase tokens).
pub fn remove_stopwords<'a, I>(tokens: I, stopwords: &HashSet<String>) -> Vec<String>
where
    I: IntoIterator<Item = &'a str>,
{
    tokens
        .into_iter()
        .filter(|t| !stopwords.contains(*t))
        .map(|t| t.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_the_and() {
        let sw = english_stopwords();
        let tokens: Vec<&str> = vec!["the", "cat", "and", "dog"];
        let out = remove_stopwords(tokens, &sw);
        assert_eq!(out, vec!["cat", "dog"]);
    }
}
