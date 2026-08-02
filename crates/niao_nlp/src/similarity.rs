//! Similarity metrics and BM25 ranking.

use std::collections::HashMap;

/// Cosine similarity between two dense vectors.
#[inline]
pub fn cosine(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0;
    let mut na = 0.0;
    let mut nb = 0.0;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na.sqrt() * nb.sqrt())
    }
}

/// Jaccard similarity between two token sets.
pub fn jaccard(a: &[&str], b: &[&str]) -> f64 {
    let sa: std::collections::HashSet<&str> = a.iter().copied().collect();
    let sb: std::collections::HashSet<&str> = b.iter().copied().collect();
    if sa.is_empty() && sb.is_empty() {
        return 1.0;
    }
    let inter = sa.intersection(&sb).count();
    let union = sa.union(&sb).count();
    if union == 0 {
        0.0
    } else {
        inter as f64 / union as f64
    }
}

/// Levenshtein edit distance (allocation-free row reuse).
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let n = a.len();
    let m = b.len();
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut cur = vec![0usize; m + 1];
    for i in 1..=n {
        cur[0] = i;
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[m]
}

/// Jaro similarity.
pub fn jaro(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let a_len = a_chars.len();
    let b_len = b_chars.len();
    if a_len == 0 || b_len == 0 {
        return 0.0;
    }
    let match_dist = (a_len.max(b_len) / 2).saturating_sub(1);
    let mut a_match = vec![false; a_len];
    let mut b_match = vec![false; b_len];
    let mut matches = 0usize;
    for i in 0..a_len {
        let start = i.saturating_sub(match_dist);
        let end = (i + match_dist + 1).min(b_len);
        for j in start..end {
            if b_match[j] || a_chars[i] != b_chars[j] {
                continue;
            }
            a_match[i] = true;
            b_match[j] = true;
            matches += 1;
            break;
        }
    }
    if matches == 0 {
        return 0.0;
    }
    let mut t = 0usize;
    let mut k = 0usize;
    for i in 0..a_len {
        if !a_match[i] {
            continue;
        }
        while !b_match[k] {
            k += 1;
        }
        if a_chars[i] != b_chars[k] {
            t += 1;
        }
        k += 1;
    }
    let m = matches as f64;
    (m / a_len as f64 + m / b_len as f64 + (m - t as f64 / 2.0) / m) / 3.0
}

/// Jaro–Winkler similarity with standard p=0.1 prefix scale.
pub fn jaro_winkler(a: &str, b: &str) -> f64 {
    let j = jaro(a, b);
    if j < 0.7 {
        return j;
    }
    let mut prefix = 0usize;
    for (ca, cb) in a.chars().zip(b.chars()) {
        if ca == cb {
            prefix += 1;
            if prefix == 4 {
                break;
            }
        } else {
            break;
        }
    }
    j + 0.1 * prefix as f64 * (1.0 - j)
}

/// BM25 ranking over documents tokenized as term→count maps.
#[derive(Debug, Clone)]
pub struct Bm25 {
    pub k1: f64,
    pub b: f64,
    pub avgdl: f64,
    pub idf: HashMap<String, f64>,
    pub doc_lens: Vec<usize>,
    pub docs: Vec<HashMap<String, usize>>,
}

impl Bm25 {
    pub fn fit(tokenized_docs: &[Vec<String>], k1: f64, b: f64) -> Self {
        let n = tokenized_docs.len();
        let mut docs = Vec::with_capacity(n);
        let mut doc_lens = Vec::with_capacity(n);
        let mut df: HashMap<String, usize> = HashMap::new();
        for toks in tokenized_docs {
            let mut counts: HashMap<String, usize> = HashMap::new();
            let mut seen = std::collections::HashSet::new();
            for t in toks {
                *counts.entry(t.clone()).or_default() += 1;
                if seen.insert(t.clone()) {
                    *df.entry(t.clone()).or_default() += 1;
                }
            }
            doc_lens.push(toks.len());
            docs.push(counts);
        }
        let avgdl = if n > 0 {
            doc_lens.iter().sum::<usize>() as f64 / n as f64
        } else {
            0.0
        };
        let idf: HashMap<String, f64> = df
            .into_iter()
            .map(|(term, dfi)| {
                let idf = ((n as f64 - dfi as f64 + 0.5) / (dfi as f64 + 0.5) + 1.0).ln();
                (term, idf)
            })
            .collect();
        Self {
            k1,
            b,
            avgdl,
            idf,
            doc_lens,
            docs,
        }
    }

    pub fn score(&self, query: &[String], doc_idx: usize) -> f64 {
        let doc = &self.docs[doc_idx];
        let dl = self.doc_lens[doc_idx] as f64;
        let mut total = 0.0;
        for q in query {
            let Some(&tf) = doc.get(q) else {
                continue;
            };
            let idf = self.idf.get(q).copied().unwrap_or(0.0);
            let num = tf as f64 * (self.k1 + 1.0);
            let den = tf as f64 + self.k1 * (1.0 - self.b + self.b * dl / self.avgdl);
            total += idf * num / den;
        }
        total
    }

    pub fn rank(&self, query: &[String]) -> Vec<(usize, f64)> {
        let mut scores: Vec<(usize, f64)> = (0..self.docs.len())
            .map(|i| (i, self.score(query, i)))
            .collect();
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levenshtein_known() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("", "abc"), 3);
    }

    #[test]
    fn jaro_winkler_known() {
        let jw = jaro_winkler("martha", "marhta");
        assert!((jw - 0.961).abs() < 0.01);
    }

    #[test]
    fn cosine_jaccard() {
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-12);
        assert!((jaccard(&["a", "b"], &["b", "c"]) - 1.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn bm25_ranks_relevant_doc_first() {
        let docs = vec![
            vec!["the".into(), "cat".into(), "sat".into()],
            vec!["the".into(), "dog".into(), "ran".into()],
            vec![
                "a".into(),
                "cat".into(),
                "chased".into(),
                "the".into(),
                "dog".into(),
            ],
        ];
        let bm = Bm25::fit(&docs, 1.5, 0.75);
        let q = vec!["cat".into()];
        let ranked = bm.rank(&q);
        assert_eq!(ranked[0].0, 0);
    }
}
