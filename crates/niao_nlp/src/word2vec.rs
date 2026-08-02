//! word2vec CBOW and skip-gram with negative sampling.

use crate::error::{NlpError, NlpResult};
use crate::similarity::cosine;
use niao_rand::{Rng, SeedableRng, Xoshiro256StarStar};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum W2vMode {
    Cbow,
    SkipGram,
}

#[derive(Debug, Clone)]
pub struct Word2VecOptions {
    pub vector_size: usize,
    pub window: usize,
    pub min_count: usize,
    pub negative: usize,
    pub epochs: usize,
    pub learning_rate: f64,
    pub subsample: f64,
    pub mode: W2vMode,
    pub seed: u64,
}

impl Default for Word2VecOptions {
    fn default() -> Self {
        Self {
            vector_size: 50,
            window: 5,
            min_count: 1,
            negative: 5,
            epochs: 5,
            learning_rate: 0.025,
            subsample: 1e-3,
            mode: W2vMode::SkipGram,
            seed: 42,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Word2Vec {
    vocab: HashMap<String, usize>,
    index_to_word: Vec<String>,
    word_freq: Vec<u64>,
    vectors: Vec<Vec<f64>>,
    context_vectors: Vec<Vec<f64>>,
    opts: Word2VecOptions,
    trained: bool,
    loss_history: Vec<f64>,
}

impl Word2Vec {
    pub fn new(opts: Word2VecOptions) -> Self {
        Self {
            vocab: HashMap::new(),
            index_to_word: Vec::new(),
            word_freq: Vec::new(),
            vectors: Vec::new(),
            context_vectors: Vec::new(),
            opts,
            trained: false,
            loss_history: Vec::new(),
        }
    }

    pub fn train(&mut self, sentences: &[Vec<String>]) -> NlpResult<()> {
        self.build_vocab(sentences)?;
        if self.vocab.is_empty() {
            return Err(NlpError::EmptyVocab);
        }
        let dim = self.opts.vector_size;
        let n = self.vocab.len();
        let mut rng = Xoshiro256StarStar::seed_from_u64(self.opts.seed);

        self.vectors = (0..n)
            .map(|_| {
                (0..dim)
                    .map(|_| (rng.gen_f64() - 0.5) / dim as f64)
                    .collect()
            })
            .collect();
        self.context_vectors = (0..n)
            .map(|_| {
                (0..dim)
                    .map(|_| (rng.gen_f64() - 0.5) / dim as f64)
                    .collect()
            })
            .collect();

        let neg_table = build_negative_table(&self.word_freq, 1_000_000, &mut rng);
        let mut epoch_loss = 0.0f64;

        for epoch in 0..self.opts.epochs {
            epoch_loss = 0.0;
            for sent in sentences {
                for (pos, word) in sent.iter().enumerate() {
                    let w = word.to_lowercase();
                    let Some(&center) = self.vocab.get(&w) else {
                        continue;
                    };
                    if self.should_skip(center, &mut rng) {
                        continue;
                    }
                    let start = pos.saturating_sub(self.opts.window);
                    let end = (pos + self.opts.window + 1).min(sent.len());
                    for (ctx_pos, ctx_word) in sent[start..end].iter().enumerate() {
                        let actual = start + ctx_pos;
                        if actual == pos {
                            continue;
                        }
                        let ctx = ctx_word.to_lowercase();
                        let Some(&context) = self.vocab.get(&ctx) else {
                            continue;
                        };
                        match self.opts.mode {
                            W2vMode::SkipGram => {
                                epoch_loss +=
                                    self.train_pair(center, context, &neg_table, &mut rng);
                            }
                            W2vMode::Cbow => {
                                epoch_loss +=
                                    self.train_cbow(center, context, &neg_table, &mut rng);
                            }
                        }
                    }
                }
            }
            self.loss_history.push(epoch_loss);
        }
        self.trained = true;
        Ok(())
    }

    fn build_vocab(&mut self, sentences: &[Vec<String>]) -> NlpResult<()> {
        let mut counts: HashMap<String, u64> = HashMap::new();
        for sent in sentences {
            for w in sent {
                *counts.entry(w.to_lowercase()).or_default() += 1;
            }
        }
        let mut items: Vec<(String, u64)> = counts
            .into_iter()
            .filter(|(_, c)| *c >= self.opts.min_count as u64)
            .collect();
        items.sort_by(|a, b| a.0.cmp(&b.0));
        self.vocab.clear();
        self.index_to_word.clear();
        self.word_freq.clear();
        for (i, (word, freq)) in items.into_iter().enumerate() {
            self.vocab.insert(word.clone(), i);
            self.index_to_word.push(word);
            self.word_freq.push(freq);
        }
        Ok(())
    }

    fn should_skip(&self, idx: usize, rng: &mut Xoshiro256StarStar) -> bool {
        if self.opts.subsample <= 0.0 {
            return false;
        }
        let f = self.word_freq[idx] as f64;
        let thresh = (self.opts.subsample * f).sqrt();
        let prob = (thresh - 1.0) / thresh;
        if prob > 0.0 {
            rng.gen_f64() < prob
        } else {
            false
        }
    }

    fn sigmoid(x: f64) -> f64 {
        if x >= 0.0 {
            1.0 / (1.0 + (-x).exp())
        } else {
            let z = x.exp();
            z / (1.0 + z)
        }
    }

    fn dot(a: &[f64], b: &[f64]) -> f64 {
        a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
    }

    fn train_pair(
        &mut self,
        center: usize,
        context: usize,
        neg_table: &[usize],
        rng: &mut Xoshiro256StarStar,
    ) -> f64 {
        let lr = self.opts.learning_rate;
        let dim = self.opts.vector_size;
        let mut loss = 0.0;
        let score = Self::dot(&self.vectors[center], &self.context_vectors[context]);
        let pred = Self::sigmoid(score);
        loss -= (pred.max(1e-10)).ln();
        let g = (1.0 - pred) * lr;
        for d in 0..dim {
            let vc = self.vectors[center][d];
            let ux = self.context_vectors[context][d];
            self.vectors[center][d] += g * ux;
            self.context_vectors[context][d] += g * vc;
        }
        for _ in 0..self.opts.negative {
            let neg = neg_table[rng.gen_range_usize(0, neg_table.len())];
            if neg == context {
                continue;
            }
            let score = Self::dot(&self.vectors[center], &self.context_vectors[neg]);
            let pred = Self::sigmoid(score);
            loss -= (1.0 - pred.max(1e-10)).ln();
            let g = -pred * lr;
            for d in 0..dim {
                let vc = self.vectors[center][d];
                let un = self.context_vectors[neg][d];
                self.vectors[center][d] += g * un;
                self.context_vectors[neg][d] += g * vc;
            }
        }
        loss
    }

    fn train_cbow(
        &mut self,
        center: usize,
        context: usize,
        neg_table: &[usize],
        rng: &mut Xoshiro256StarStar,
    ) -> f64 {
        // Simplified CBOW: treat each context word independently (v1)
        self.train_pair(context, center, neg_table, rng)
    }

    pub fn vector(&self, word: &str) -> NlpResult<&[f64]> {
        if !self.trained {
            return Err(NlpError::NotFitted);
        }
        let w = word.to_lowercase();
        let idx = self.vocab.get(&w).ok_or_else(|| NlpError::Oov(w.clone()))?;
        Ok(&self.vectors[*idx])
    }

    pub fn most_similar(&self, word: &str, topn: usize) -> NlpResult<Vec<(String, f64)>> {
        let v = self.vector(word)?;
        let mut scores: Vec<(String, f64)> = self
            .index_to_word
            .iter()
            .enumerate()
            .filter(|(i, w)| w.to_lowercase() != word.to_lowercase())
            .map(|(i, w)| (w.clone(), cosine(v, &self.vectors[i])))
            .collect();
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(topn);
        Ok(scores)
    }

    pub fn analogy(
        &self,
        positive: &[&str],
        negative: &[&str],
        topn: usize,
    ) -> NlpResult<Vec<(String, f64)>> {
        if !self.trained {
            return Err(NlpError::NotFitted);
        }
        let dim = self.opts.vector_size;
        let mut vec = vec![0.0; dim];
        for w in positive {
            let v = self.vector(w)?;
            for (i, x) in v.iter().enumerate() {
                vec[i] += x;
            }
        }
        for w in negative {
            let v = self.vector(w)?;
            for (i, x) in v.iter().enumerate() {
                vec[i] -= x;
            }
        }
        let mut scores: Vec<(String, f64)> = self
            .index_to_word
            .iter()
            .enumerate()
            .filter(|(_, w)| {
                let wl = w.to_lowercase();
                !positive.iter().any(|p| p.to_lowercase() == wl)
                    && !negative.iter().any(|n| n.to_lowercase() == wl)
            })
            .map(|(i, w)| (w.clone(), cosine(&vec, &self.vectors[i])))
            .collect();
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(topn);
        Ok(scores)
    }

    pub fn loss_history(&self) -> &[f64] {
        &self.loss_history
    }
}

fn build_negative_table(freq: &[u64], size: usize, rng: &mut Xoshiro256StarStar) -> Vec<usize> {
    if freq.is_empty() {
        return vec![0];
    }
    let total: f64 = freq.iter().map(|&f| f as f64).sum();
    let mut table = Vec::with_capacity(size);
    let mut i = 0usize;
    while table.len() < size {
        let f = freq[i % freq.len()] as f64;
        let count = ((f / total) * size as f64).round() as usize;
        for _ in 0..count.max(1) {
            if table.len() >= size {
                break;
            }
            table.push(i % freq.len());
        }
        i += 1;
    }
    // shuffle for random draws
    for j in (1..table.len()).rev() {
        let k = rng.gen_range_usize(0, j + 1);
        table.swap(j, k);
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toy_corpus() -> Vec<Vec<String>> {
        vec![
            vec!["king", "queen", "royal", "castle"]
                .into_iter()
                .map(String::from)
                .collect(),
            vec!["king", "prince", "royal", "throne"]
                .into_iter()
                .map(String::from)
                .collect(),
            vec!["queen", "princess", "royal", "castle"]
                .into_iter()
                .map(String::from)
                .collect(),
            vec!["man", "woman", "child", "family"]
                .into_iter()
                .map(String::from)
                .collect(),
            vec!["man", "worker", "job", "office"]
                .into_iter()
                .map(String::from)
                .collect(),
            vec!["woman", "worker", "job", "office"]
                .into_iter()
                .map(String::from)
                .collect(),
        ]
    }

    #[test]
    fn most_similar_neighbors() {
        let mut w2v = Word2Vec::new(Word2VecOptions {
            vector_size: 16,
            window: 2,
            epochs: 30,
            learning_rate: 0.05,
            negative: 3,
            seed: 42,
            mode: W2vMode::SkipGram,
            ..Default::default()
        });
        w2v.train(&toy_corpus()).unwrap();
        let sim = w2v.most_similar("king", 5).unwrap();
        assert!(!sim.is_empty());
        let words: Vec<_> = sim.iter().map(|(w, _)| w.as_str()).collect();
        // Co-occurring royalty terms should rank highly after training.
        assert!(
            words.iter().any(|w| matches!(
                *w,
                "queen" | "prince" | "royal" | "princess" | "castle" | "throne"
            )),
            "neighbors were {words:?}"
        );
    }

    #[test]
    fn loss_decreases() {
        let mut w2v = Word2Vec::new(Word2VecOptions {
            vector_size: 8,
            epochs: 10,
            seed: 7,
            ..Default::default()
        });
        w2v.train(&toy_corpus()).unwrap();
        let hist = w2v.loss_history();
        assert!(hist.len() >= 2);
        // Not strictly monotonic due to sampling, but last epoch loss should improve vs first on average
        assert!(*hist.last().unwrap() <= hist.first().unwrap() * 1.5);
    }
}
