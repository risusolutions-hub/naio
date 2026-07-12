# Library spec: `nnlp`  →  crate `niao_nlp`

| | |
|---|---|
| Category | Classical NLP |
| Replaces (Python) | `nltk` / `gensim` (classical) + `sklearn.feature_extraction.text` |
| Rust reference | `rust-tokenizers`, `whatlang` |
| Target Niao crate | `crates/niao_nlp` |
| Niao import name | `nnlp` |
| Difficulty | 4/5 — Very Hard |
| Wave | 2 (needs nnum, nframe, ntok, nlearn) |
| Depends on Niao libs | `nnum`, `nframe`, `ntok`, `nlearn`, `nembed` (reuse, don't rebuild) |
| Error block | 4080–4089 |

## Goal
Classical text processing + feature extraction: normalization, tokenization, stemming, n-grams, bag-of-words /
TF-IDF vectorizers, word2vec-style embeddings, similarity, and text-classification glue. **Zero external deps.**
**Do not rebuild** the BPE tokenizer (`ntok`) or neural embeddings (`nembed`) — call them.

## Scope (v1)
- **Text normalization:** lowercasing, Unicode NFC/NFKC (via existing niao facilities), punctuation/whitespace
  cleanup, accent stripping, contraction expansion, number/URL/email masking.
- **Tokenization:** word/sentence tokenizers (regex + rules via `nregex`), whitespace/Penn-style; wrap `ntok`
  for subword/BPE. Sentence splitter with abbreviation handling.
- **Stopwords:** built-in English list (+ pluggable), `remove_stopwords`.
- **Stemming / lemmatization:** Porter stemmer (classic, fully specified) + Snowball-English; a lightweight
  lookup lemmatizer (dictionary-based) — full morphological lemmatization is v2.
- **N-grams:** word + char n-grams, `ngrams(tokens, n)`, skip-grams.
- **Vectorizers (sklearn-compatible):** `CountVectorizer`, `TfidfVectorizer` (sublinear tf, smooth idf, l2 norm),
  `HashingVectorizer`; `fit/transform` → sparse feature matrix interop with `nlearn`.
- **Embeddings:** word2vec (CBOW + skip-gram, negative sampling) trainer + loader; GloVe loader;
  `most_similar`, `analogy`, cosine similarity. Sentence embeddings via `nembed` (call it).
- **Similarity / distance:** cosine, Jaccard, Levenshtein/edit distance, Jaro–Winkler, BM25 ranking.
- **Baseline tasks:** language detection (n-gram profile), sentiment (lexicon baseline), keyword extraction
  (TF-IDF / RAKE), text classification pipeline (TfidfVectorizer → `nlearn` LogisticRegression/NB).

## Implementation blueprint
- **Sparse features.** Vectorizers emit CSR-style sparse matrices (indptr/indices/data), not dense — vocab is huge.
  Provide `to_dense()`/`to_nnum()` for small cases and a sparse handoff for `nlearn` linear models/NB.
- Vocabulary: hash map term→index built in `fit`; `transform` reuses it; `min_df/max_df/max_features` pruning.
- TF-IDF: `idf = ln((1+n)/(1+df)) + 1` (smooth), l2-normalize rows — match sklearn's default exactly for parity.
- Porter stemmer: implement the canonical 5-step algorithm verbatim (it's fully specified) and unit-test against
  the reference word list.
- word2vec: sliding-window contexts, negative sampling table (via `nrand`), SGD updates on embedding + context
  matrices; reuse `nnum`/SIMD for the dot products. Subsample frequent words.
- Levenshtein/Jaro: classic DP, allocation-free row-reuse.

### Performance rules
- No per-token `String` allocation in hot loops — work on `&str` slices and byte offsets. Sparse rows pre-sized.
- `#[inline]` the stemmer step predicates and distance inner loops; SIMD the word2vec dot products with fallback.

## Public API surface
`normalize`, `word_tokenize/sent_tokenize`, `PorterStemmer`, `remove_stopwords`, `ngrams`, `CountVectorizer`,
`TfidfVectorizer`, `HashingVectorizer`, `Word2Vec` (`train/most_similar/analogy`), `cosine/jaccard/levenshtein/bm25`,
`detect_language`, `sentiment`, `keywords`. Same `fit/transform` shape as `nlearn`. Expose to Niao via
`niao_libs/nnlp/` + builtins.

## Performance target
Correctness/parity is the gate. TfidfVectorizer output matches sklearn within `rtol=1e-8`; word2vec similarity
qualitatively sane on a small corpus. Vectorize 100k short docs in reasonable time (log it).

## Tests required
- Porter stemmer vs the canonical reference word list (exact match, hundreds of words).
- `CountVectorizer`/`TfidfVectorizer` vocab + matrix vs sklearn fixtures (same options), `rtol=1e-8` on tf-idf weights.
- N-grams, stopword removal, tokenizer outputs vs expected fixtures.
- Levenshtein/Jaro–Winkler/cosine/Jaccard vs known values.
- word2vec: on a tiny seeded corpus, `most_similar` returns expected neighbors; loss decreases across epochs.
- Text-classification pipeline (Tfidf → nlearn LogisticRegression) reaches expected accuracy on a fixture.
- Language detection correct on labeled short samples; BM25 ranks a known query as expected.
- Degenerate: transform before fit → 4083; empty vocabulary after pruning → 4084.
- Plus: in-crate unit tests, `examples/nnlp_demo.niao`, `benchmarks/benchmark_nnlp.py` vs sklearn/nltk/gensim.

## Risk / notes
- **Don't rebuild `ntok`/`nembed`.** Subword tokenization and transformer embeddings already exist — wrap them.
- TF-IDF parity requires matching sklearn's exact smoothing/normalization — copy the formula precisely or tests fail.
- Full lemmatization (WordNet-class) and neural NER are v2; ship lexicon/dictionary baselines.
- Unicode normalization should reuse existing niao facilities rather than a new tables dump.

## Done criteria
- `cargo check --workspace` and `cargo test -p niao_nlp` green; sklearn/Porter fixtures pass in tolerance.
- `niao_libs/nnlp/` wrapper + `examples/nnlp_demo.niao` runs clean→vectorize→classify end-to-end.
- Benchmark + notes in `REPORT.md`; `CHANGELOG.md` updated; shared-file edits reported, not applied.
