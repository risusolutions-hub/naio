//! BM25 scoring helpers.

/// Classic BM25 term weight.
///
/// `tf` = term frequency in document, `df` = document frequency,
/// `n_docs` = collection size, `dl` = document length (tokens in field),
/// `avg_dl` = average document length for the field.
#[inline]
pub fn bm25(tf: f64, df: u64, n_docs: u64, dl: f64, avg_dl: f64, k1: f64, b: f64) -> f64 {
    if n_docs == 0 || df == 0 || avg_dl <= 0.0 {
        return 0.0;
    }
    let idf = ((n_docs as f64 - df as f64 + 0.5) / (df as f64 + 0.5) + 1.0).ln();
    let norm = 1.0 - b + b * (dl / avg_dl);
    let tf_norm = (tf * (k1 + 1.0)) / (tf + k1 * norm);
    idf * tf_norm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bm25_higher_tf_scores_higher() {
        let a = bm25(1.0, 10, 1000, 100.0, 100.0, 1.2, 0.75);
        let b = bm25(5.0, 10, 1000, 100.0, 100.0, 1.2, 0.75);
        assert!(b > a);
    }

    #[test]
    fn bm25_rare_term_scores_higher() {
        let common = bm25(1.0, 500, 1000, 100.0, 100.0, 1.2, 0.75);
        let rare = bm25(1.0, 2, 1000, 100.0, 100.0, 1.2, 0.75);
        assert!(rare > common);
    }
}
