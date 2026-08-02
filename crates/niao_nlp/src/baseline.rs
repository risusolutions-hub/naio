//! Baseline NLP tasks: language detection, lexicon sentiment, keyword extraction.

use crate::tokenize::word_tokenize;
use crate::vectorizer::{TfidfVectorizer, VectorizerOptions};
use std::collections::HashMap;

static LANG_PROFILES: &[(&str, &[&str])] = &[
    ("en", &["the", "and", "is", "in", "to", "of", "a"]),
    ("fr", &["le", "la", "de", "et", "est", "un", "une"]),
    ("de", &["der", "die", "und", "ist", "ein", "eine", "nicht"]),
    ("es", &["el", "la", "de", "y", "en", "un", "una"]),
];

/// N-gram profile language detection (short text baseline).
pub fn detect_language(text: &str) -> String {
    let toks = word_tokenize(text, true);
    if toks.is_empty() {
        return "unknown".into();
    }
    let mut best_lang = "unknown".to_string();
    let mut best_score = 0usize;
    for (lang, profile) in LANG_PROFILES {
        let score = profile
            .iter()
            .filter(|w| toks.contains(&w.to_string()))
            .count();
        if score > best_score {
            best_score = score;
            best_lang = lang.to_string();
        }
    }
    best_lang
}

static SENTIMENT_LEX: &[(&str, i8)] = &[
    ("good", 1),
    ("great", 2),
    ("excellent", 3),
    ("bad", -1),
    ("terrible", -3),
    ("awful", -2),
    ("love", 2),
    ("hate", -2),
    ("happy", 2),
    ("sad", -2),
];

/// Lexicon baseline sentiment: positive / negative / neutral.
pub fn sentiment(text: &str) -> &'static str {
    let toks = word_tokenize(text, true);
    let mut score = 0i32;
    for t in &toks {
        for (word, val) in SENTIMENT_LEX {
            if t == *word {
                score += *val as i32;
            }
        }
    }
    if score > 0 {
        "positive"
    } else if score < 0 {
        "negative"
    } else {
        "neutral"
    }
}

/// RAKE-style keyword extraction via degree scoring of word co-occurrence.
pub fn keywords_rake(text: &str, topn: usize) -> Vec<(String, f64)> {
    let sents: Vec<&str> = text.split(|c| c == '.' || c == '!' || c == '?').collect();
    let stop: std::collections::HashSet<String> = crate::stopwords::english_stopwords();
    let mut phrase_scores: HashMap<String, f64> = HashMap::new();
    let mut word_degree: HashMap<String, f64> = HashMap::new();
    let mut word_freq: HashMap<String, f64> = HashMap::new();

    for sent in sents {
        let toks: Vec<String> = word_tokenize(sent, true)
            .into_iter()
            .filter(|t| !stop.contains(t) && t.len() > 1)
            .collect();
        if toks.is_empty() {
            continue;
        }
        let phrase = toks.join(" ");
        *phrase_scores.entry(phrase).or_default() += 1.0;
        for w in &toks {
            *word_freq.entry(w.clone()).or_default() += 1.0;
            *word_degree.entry(w.clone()).or_default() += toks.len() as f64;
        }
    }

    let mut word_score: HashMap<String, f64> = HashMap::new();
    for (w, deg) in &word_degree {
        let freq = word_freq.get(w).copied().unwrap_or(1.0);
        word_score.insert(w.clone(), deg / freq);
    }

    let mut ranked: Vec<(String, f64)> = phrase_scores
        .keys()
        .map(|phrase| {
            let score: f64 = phrase
                .split_whitespace()
                .filter_map(|w| word_score.get(w))
                .sum();
            (phrase.clone(), score)
        })
        .collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(topn);
    ranked
}

/// TF-IDF keyword extraction from a single document against itself (top terms).
pub fn keywords_tfidf(text: &str, topn: usize) -> Vec<(String, f64)> {
    let mut tv = TfidfVectorizer::new(VectorizerOptions {
        norm_l2: false,
        ..Default::default()
    });
    let mat = tv.fit_transform(&[text]).unwrap();
    let vocab = tv.vocabulary().unwrap();
    let mut inv: HashMap<usize, String> = HashMap::new();
    for (term, idx) in vocab {
        inv.insert(*idx, term.clone());
    }
    let mut pairs: Vec<(String, f64)> = mat
        .row_values(0)
        .map(|(c, v)| (inv.get(&c).cloned().unwrap_or_default(), v))
        .collect();
    pairs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    pairs.truncate(topn);
    pairs
}

/// Convenience: RAKE first, TF-IDF fallback on empty.
pub fn keywords(text: &str, topn: usize) -> Vec<(String, f64)> {
    let rake = keywords_rake(text, topn);
    if rake.is_empty() {
        keywords_tfidf(text, topn)
    } else {
        rake
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_detect_en_fr() {
        assert_eq!(detect_language("the cat is in the garden"), "en");
        assert_eq!(detect_language("le chat est dans le jardin"), "fr");
    }

    #[test]
    fn sentiment_lexicon() {
        assert_eq!(sentiment("I love this excellent product"), "positive");
        assert_eq!(sentiment("this is terrible and awful"), "negative");
        assert_eq!(sentiment("the table is brown"), "neutral");
    }

    #[test]
    fn keywords_non_empty() {
        let kws = keywords(
            "machine learning algorithms for natural language processing",
            3,
        );
        assert!(!kws.is_empty());
    }
}
