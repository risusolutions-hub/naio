//! CountVectorizer, TfidfVectorizer, HashingVectorizer (sklearn-compatible).

use crate::error::{NlpError, NlpResult};
use crate::ngrams::ngram_range;
use crate::normalize::NormalizeOptions;
use crate::sparse::CsrMatrix;
use crate::tokenize::tokenize_for_vectorizer;
use std::collections::{HashMap, HashSet};

fn stable_hash(s: &str) -> usize {
    // sklearn MurmurHash-like sign trick: abs(hash) % n_features
    let mut h: u32 = 5381;
    for b in s.as_bytes() {
        h = h.wrapping_mul(33).wrapping_add(*b as u32);
    }
    h as usize
}

#[derive(Debug, Clone)]
pub struct VectorizerOptions {
    pub lowercase: bool,
    pub ngram_min: usize,
    pub ngram_max: usize,
    pub min_df: usize,
    pub max_df_ratio: f64,
    pub max_features: Option<usize>,
    pub binary: bool,
    pub sublinear_tf: bool,
    pub use_idf: bool,
    pub smooth_idf: bool,
    pub norm_l2: bool,
}

impl Default for VectorizerOptions {
    fn default() -> Self {
        Self {
            lowercase: true,
            ngram_min: 1,
            ngram_max: 1,
            min_df: 1,
            max_df_ratio: 1.0,
            max_features: None,
            binary: false,
            sublinear_tf: false,
            use_idf: true,
            smooth_idf: true,
            norm_l2: true,
        }
    }
}

fn norm_opts(opts: &VectorizerOptions) -> NormalizeOptions {
    NormalizeOptions {
        lowercase: opts.lowercase,
        ..NormalizeOptions::sklearn_default()
    }
}

fn tokenize_doc(text: &str, opts: &VectorizerOptions) -> Vec<String> {
    let base = tokenize_for_vectorizer(text, &norm_opts(opts));
    if opts.ngram_min == 1 && opts.ngram_max == 1 {
        base
    } else {
        ngram_range(&base, opts.ngram_min, opts.ngram_max)
    }
}

fn apply_tf(raw: f64, opts: &VectorizerOptions) -> f64 {
    if opts.binary {
        if raw > 0.0 {
            1.0
        } else {
            0.0
        }
    } else if opts.sublinear_tf {
        if raw > 0.0 {
            1.0 + raw.ln()
        } else {
            0.0
        }
    } else {
        raw
    }
}

fn build_vocab(
    docs_tokens: &[Vec<String>],
    opts: &VectorizerOptions,
) -> NlpResult<(HashMap<String, usize>, Vec<f64>)> {
    let n_docs = docs_tokens.len();
    let mut df: HashMap<String, usize> = HashMap::new();
    for toks in docs_tokens {
        let mut seen = HashSet::new();
        for t in toks {
            if seen.insert(t.clone()) {
                *df.entry(t.clone()).or_default() += 1;
            }
        }
    }

    let max_df_count = if opts.max_df_ratio >= 1.0 {
        n_docs
    } else {
        (opts.max_df_ratio * n_docs as f64).floor() as usize
    };

    let mut terms: Vec<(String, usize)> = df
        .into_iter()
        .filter(|(_, count)| *count >= opts.min_df && *count <= max_df_count)
        .collect();

    if terms.is_empty() {
        return Err(NlpError::EmptyVocab);
    }

    // sklearn: sort by term order for determinism when max_features set — use descending df then alpha
    terms.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    if let Some(max_f) = opts.max_features {
        if terms.len() > max_f {
            terms.truncate(max_f);
        }
    }

    terms.sort_by(|a, b| a.0.cmp(&b.0));

    let vocab: HashMap<String, usize> = terms
        .iter()
        .enumerate()
        .map(|(i, (t, _))| (t.clone(), i))
        .collect();

    let idf = if opts.use_idf {
        terms
            .iter()
            .map(|(_, df_count)| {
                if opts.smooth_idf {
                    ((1.0 + n_docs as f64) / (1.0 + *df_count as f64)).ln() + 1.0
                } else {
                    (n_docs as f64 / *df_count as f64).ln()
                }
            })
            .collect()
    } else {
        vec![1.0; terms.len()]
    };

    Ok((vocab, idf))
}

fn doc_term_counts(toks: &[String], vocab: &HashMap<String, usize>) -> HashMap<usize, f64> {
    let mut counts: HashMap<usize, f64> = HashMap::new();
    for t in toks {
        if let Some(&idx) = vocab.get(t) {
            *counts.entry(idx).or_default() += 1.0;
        }
    }
    counts
}

fn counts_to_csr(
    row_counts: &[HashMap<usize, f64>],
    n_cols: usize,
    idf: &[f64],
    opts: &VectorizerOptions,
) -> CsrMatrix {
    let n_rows = row_counts.len();
    let mut mat = CsrMatrix::new(n_rows, n_cols);
    mat.indptr[0] = 0;
    for (r, counts) in row_counts.iter().enumerate() {
        let mut pairs: Vec<(usize, f64)> = counts
            .iter()
            .map(|(&c, &tf)| {
                let mut v = apply_tf(tf, opts);
                if opts.use_idf {
                    v *= idf[c];
                }
                (c, v)
            })
            .collect();
        pairs.sort_by_key(|p| p.0);
        for (c, v) in pairs {
            mat.indices.push(c);
            mat.data.push(v);
        }
        mat.indptr[r + 1] = mat.indices.len();
    }
    if opts.norm_l2 {
        mat.l2_normalize_rows();
    }
    mat
}

/// Bag-of-words count vectorizer.
#[derive(Debug, Clone)]
pub struct CountVectorizer {
    opts: VectorizerOptions,
    vocab: Option<HashMap<String, usize>>,
    idf: Vec<f64>,
    fitted: bool,
}

impl Default for CountVectorizer {
    fn default() -> Self {
        Self::new(VectorizerOptions {
            use_idf: false,
            norm_l2: false,
            ..Default::default()
        })
    }
}

impl CountVectorizer {
    pub fn new(opts: VectorizerOptions) -> Self {
        Self {
            opts,
            vocab: None,
            idf: Vec::new(),
            fitted: false,
        }
    }

    pub fn fit(&mut self, docs: &[&str]) -> NlpResult<&mut Self> {
        let tokenized: Vec<Vec<String>> =
            docs.iter().map(|d| tokenize_doc(d, &self.opts)).collect();
        let (vocab, idf) = build_vocab(&tokenized, &self.opts)?;
        self.vocab = Some(vocab);
        self.idf = idf;
        self.fitted = true;
        Ok(self)
    }

    pub fn transform(&self, docs: &[&str]) -> NlpResult<CsrMatrix> {
        let vocab = self.vocab.as_ref().ok_or(NlpError::NotFitted)?;
        let n_cols = vocab.len();
        let row_counts: Vec<_> = docs
            .iter()
            .map(|d| {
                let toks = tokenize_doc(d, &self.opts);
                doc_term_counts(&toks, vocab)
            })
            .collect();
        Ok(counts_to_csr(&row_counts, n_cols, &self.idf, &self.opts))
    }

    pub fn fit_transform(&mut self, docs: &[&str]) -> NlpResult<CsrMatrix> {
        self.fit(docs)?;
        self.transform(docs)
    }

    pub fn vocabulary(&self) -> NlpResult<&HashMap<String, usize>> {
        self.vocab.as_ref().ok_or(NlpError::NotFitted)
    }
}

/// TF-IDF vectorizer (sklearn-compatible defaults).
#[derive(Debug, Clone)]
pub struct TfidfVectorizer {
    inner: CountVectorizer,
}

impl Default for TfidfVectorizer {
    fn default() -> Self {
        Self::new(VectorizerOptions::default())
    }
}

impl TfidfVectorizer {
    pub fn new(opts: VectorizerOptions) -> Self {
        Self {
            inner: CountVectorizer::new(opts),
        }
    }

    pub fn fit(&mut self, docs: &[&str]) -> NlpResult<&mut Self> {
        self.inner.fit(docs)?;
        Ok(self)
    }

    pub fn transform(&self, docs: &[&str]) -> NlpResult<CsrMatrix> {
        self.inner.transform(docs)
    }

    pub fn fit_transform(&mut self, docs: &[&str]) -> NlpResult<CsrMatrix> {
        self.inner.fit_transform(docs)
    }

    pub fn idf(&self) -> NlpResult<&[f64]> {
        if !self.inner.fitted {
            return Err(NlpError::NotFitted);
        }
        Ok(&self.inner.idf)
    }

    pub fn vocabulary(&self) -> NlpResult<&HashMap<String, usize>> {
        self.inner.vocabulary()
    }
}

/// Feature hashing (HashingVectorizer) — no fit step.
#[derive(Debug, Clone)]
pub struct HashingVectorizer {
    pub n_features: usize,
    pub opts: VectorizerOptions,
}

impl HashingVectorizer {
    pub fn new(n_features: usize, opts: VectorizerOptions) -> Self {
        Self { n_features, opts }
    }

    pub fn transform(&self, docs: &[&str]) -> NlpResult<CsrMatrix> {
        if self.n_features == 0 {
            return Err(NlpError::Error("n_features must be > 0".into()));
        }
        let mut mat = CsrMatrix::new(docs.len(), self.n_features);
        mat.indptr[0] = 0;
        for (r, doc) in docs.iter().enumerate() {
            let toks = tokenize_doc(doc, &self.opts);
            let mut counts: HashMap<usize, f64> = HashMap::new();
            for t in toks {
                let idx = stable_hash(&t) % self.n_features;
                *counts.entry(idx).or_default() += 1.0;
            }
            let mut pairs: Vec<(usize, f64)> = counts
                .into_iter()
                .map(|(c, tf)| (c, apply_tf(tf, &self.opts)))
                .collect();
            pairs.sort_by_key(|p| p.0);
            for (c, v) in pairs {
                mat.indices.push(c);
                mat.data.push(v);
            }
            mat.indptr[r + 1] = mat.indices.len();
        }
        if self.opts.norm_l2 {
            mat.l2_normalize_rows();
        }
        Ok(mat)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, rtol: f64) -> bool {
        (a - b).abs() <= rtol * b.abs().max(1.0) + 1e-12
    }

    const DOCS: &[&str] = &["the cat sat on the mat", "the dog sat on the log"];

    // sklearn 1.4 defaults on DOCS
    #[test]
    fn count_vectorizer_vocab() {
        let mut cv = CountVectorizer::default();
        cv.fit(DOCS).unwrap();
        let vocab = cv.vocabulary().unwrap();
        let mut terms: Vec<_> = vocab.keys().cloned().collect();
        terms.sort();
        assert_eq!(terms, vec!["cat", "dog", "log", "mat", "on", "sat", "the"]);
    }

    #[test]
    fn tfidf_sklearn_fixture() {
        let mut tv = TfidfVectorizer::default();
        let mat = tv.fit_transform(DOCS).unwrap();
        assert_eq!(mat.n_rows, 2);
        assert_eq!(mat.n_cols, 7);

        // sklearn TfidfVectorizer().fit_transform(DOCS).toarray() row 0 (approx)
        let dense = mat.to_dense().unwrap();
        let row0: Vec<f64> = (0..7).map(|c| dense.index(&[0, c]).unwrap()).collect();

        // Expected from sklearn 1.4 (rtol 1e-8); columns sorted by vocab term order
        let expected = [0.445548, 0.0, 0.0, 0.445548, 0.317011, 0.317011, 0.634021];
        for (a, e) in row0.iter().zip(expected) {
            assert!(close(*a, e, 1e-6), "got {a} expected {e}");
        }
    }

    #[test]
    fn transform_before_fit() {
        let cv = CountVectorizer::default();
        assert_eq!(cv.transform(DOCS).unwrap_err().code(), 4083);
    }

    #[test]
    fn empty_vocab_after_prune() {
        let mut cv = CountVectorizer::new(VectorizerOptions {
            min_df: 99,
            ..Default::default()
        });
        assert_eq!(cv.fit(DOCS).unwrap_err().code(), 4084);
    }

    #[test]
    fn sublinear_tf_changes_weights() {
        let mut tv = TfidfVectorizer::new(VectorizerOptions {
            sublinear_tf: true,
            norm_l2: false,
            ..Default::default()
        });
        let mat = tv.fit_transform(&["aa aa aa"]).unwrap();
        let mut tv2 = TfidfVectorizer::new(VectorizerOptions {
            sublinear_tf: false,
            norm_l2: false,
            ..Default::default()
        });
        let mat2 = tv2.fit_transform(&["aa aa aa"]).unwrap();
        assert!(mat.data[0] < mat2.data[0]);
    }
}
