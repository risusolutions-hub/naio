//! nnlp — classical NLP for Niao (nltk/gensim/sklearn text subset).
//!
//! Normalization, tokenization, stemming, n-grams, TF-IDF vectorizers, word2vec,
//! similarity metrics, and baseline text tasks. Zero external deps.
//! Error block: 4080–4089.

pub mod baseline;
pub mod error;
pub mod ngrams;
pub mod normalize;
pub mod sparse;
pub mod stem;
pub mod stopwords;
pub mod similarity;
pub mod tokenize;
pub mod vectorizer;
pub mod word2vec;

pub use baseline::{detect_language, keywords, keywords_rake, keywords_tfidf, sentiment};
pub use error::{
    NlpError, NlpResult, E4080_NNLP_ARITY, E4081_NNLP_ERROR, E4082_NNLP_TYPE,
    E4083_NNLP_NOT_FITTED, E4084_NNLP_EMPTY_VOCAB, E4085_NNLP_SHAPE, E4086_NNLP_OOV,
};
pub use ngrams::{char_ngrams, ngram_range, ngrams, skip_grams};
pub use normalize::{normalize, NormalizeOptions};
pub use sparse::CsrMatrix;
pub use stem::{DictLemmatizer, PorterStemmer, SnowballEnglish};
pub use stopwords::{english_stopwords, remove_stopwords};
pub use similarity::{cosine, jaccard, jaro, jaro_winkler, levenshtein, Bm25};
pub use tokenize::{sent_tokenize, tokenize_for_vectorizer, whitespace_tokenize, word_tokenize};
pub use vectorizer::{CountVectorizer, HashingVectorizer, TfidfVectorizer, VectorizerOptions};
pub use word2vec::{Word2Vec, Word2VecOptions, W2vMode};

#[cfg(test)]
mod integration {
    use super::*;

    #[test]
    fn end_to_end_clean_vectorize() {
        let docs = &[
            "The quick brown fox jumps over the lazy dog.",
            "A fast brown fox leaps over a sleepy dog.",
        ];
        let norm_opts = NormalizeOptions::sklearn_default();
        let cleaned: Vec<String> = docs
            .iter()
            .map(|d| normalize(d, &norm_opts))
            .collect();
        let mut tv = TfidfVectorizer::default();
        let mat = tv
            .fit_transform(&cleaned.iter().map(|s| s.as_str()).collect::<Vec<_>>())
            .unwrap();
        assert!(mat.nnz() > 0);
        assert_eq!(mat.n_rows, 2);
    }
}
