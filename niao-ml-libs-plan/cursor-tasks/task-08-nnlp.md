# Task 08 — nnlp: nltk / gensim + sklearn text (crate `niao_nlp`)
Wave 2 (needs nnum, nframe, ntok, nlearn). Read `../MASTER_PLAN.md` + `../specs/niao_nlp__nnlp.md`. Error block **4080–4089**.
Depends on: `nnum`, `nframe`, `ntok`, `nlearn`, `nembed`. **Do NOT rebuild ntok (BPE) or nembed (neural) — call them.**

## Build (`crates/niao_nlp`, zero new deps)
- Normalize: lowercase, Unicode NFC/NFKC (reuse niao facilities), punctuation/whitespace, accent strip, contractions, mask url/email/number.
- Tokenize: word/sentence (via nregex + rules), whitespace/Penn; wrap ntok for subword. Stopwords (English + pluggable).
- Stemming: Porter (canonical 5-step, verbatim) + Snowball-English; dictionary lemmatizer (full = v2).
- N-grams (word+char), skip-grams. Vectorizers: CountVectorizer, **TfidfVectorizer** (idf=ln((1+n)/(1+df))+1, sublinear tf, l2 —
  match sklearn exactly), HashingVectorizer → CSR sparse (indptr/indices/data), to_nnum/to_dense; min_df/max_df/max_features.
- word2vec (CBOW+skip-gram, negative sampling via nrand, frequent-word subsample) train/most_similar/analogy; GloVe loader.
- Similarity: cosine/Jaccard/Levenshtein/Jaro–Winkler/BM25. Baselines: language detect (n-gram profile), lexicon sentiment,
  RAKE/TF-IDF keywords, text-classification pipeline (Tfidf→nlearn LogReg/NB). Same fit/transform shape as nlearn.
- No per-token String alloc in hot loops (work on &str + offsets); SIMD word2vec dot products with fallback.

## Wire up
- `niao_libs/nnlp/` wrapper + builtins; `docs/NNLP.md`; `examples/nnlp_demo.niao` (clean→vectorize→classify).

## Acceptance
- Porter stemmer vs canonical reference word list (exact, hundreds); Count/Tfidf vocab+matrix vs sklearn fixtures rtol 1e-8;
  ngrams/stopwords/tokenizer vs fixtures; Levenshtein/Jaro/cosine/Jaccard known values; word2vec most_similar expected on
  seeded corpus + loss decreases; Tfidf→nlearn pipeline hits expected accuracy; language detect + BM25 correct.
- transform-before-fit→4083, empty vocab→4084.
- `benchmarks/benchmark_nnlp.py` vs sklearn/nltk/gensim. `cargo test -p niao_nlp` green.

See `../cursor-rules.md`.
