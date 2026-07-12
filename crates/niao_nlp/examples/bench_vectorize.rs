//! Vectorization micro-benchmark (100k short docs).

use niao_nlp::{TfidfVectorizer, VectorizerOptions};
use std::time::Instant;

fn main() {
    let n_docs = 100_000usize;
    let docs: Vec<String> = (0..n_docs)
        .map(|i| format!("document number {i} about cats dogs and natural language processing"))
        .collect();
    let refs: Vec<&str> = docs.iter().map(|s| s.as_str()).collect();

    let mut tv = TfidfVectorizer::new(VectorizerOptions::default());
    let t0 = Instant::now();
    let _ = tv.fit_transform(&refs).unwrap();
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!("{ms:.1}");
}
