# NNLP — Classical NLP for Niao

`nnlp` replaces core **nltk/gensim/sklearn.text** classical NLP with a std-only
native library (`crates/niao_nlp`). Depends on `nnum`, `nframe`, and `nrand`.
Subword tokenization via `ntok` is runtime-only (not rebuilt here). **No transformer embeddings.**

Import:

```niao
import "nnlp"
```

## Normalization

| Function | Description |
|----------|-------------|
| `nnlp.normalize(text, opts?)` | Lowercase, accent strip, punctuation/WS cleanup, contractions, URL/email/number masking |

## Tokenization

| Function | Description |
|----------|-------------|
| `nnlp.word_tokenize(text)` | sklearn `\b\w\w+\b` word tokens |
| `nnlp.sent_tokenize(text)` | Sentence splitter with abbreviation handling |
| `nnlp.remove_stopwords(tokens, stopwords?)` | Filter English/pluggable stopwords |

## Stemming / lemmatization

| Type | Description |
|------|-------------|
| `nnlp.PorterStemmer` | Martin Porter reference C algorithm |
| `nnlp.SnowballEnglish` | English Snowball (Porter-aligned) |
| `nnlp.DictLemmatizer` | Lightweight dictionary lookup (v1) |

## N-grams

| Function | Description |
|----------|-------------|
| `nnlp.ngrams(tokens, n)` | Word n-grams |
| `nnlp.char_ngrams(text, n)` | Character n-grams |
| `nnlp.skip_grams(tokens, n, skip)` | Skip-grams |

## Vectorizers (sklearn-compatible)

CSR sparse output (`indptr`, `indices`, `data`) with `to_dense()` / `to_nnum()`.

| Estimator | Description |
|-----------|-------------|
| `nnlp.CountVectorizer` | Bag-of-words counts |
| `nnlp.TfidfVectorizer` | TF-IDF: `idf = ln((1+n)/(1+df))+1`, sublinear tf, L2 norm |
| `nnlp.HashingVectorizer` | Feature hashing (no fit) |

Options: `min_df`, `max_df`, `max_features`, `ngram_range`, `binary`, `sublinear_tf`.

## word2vec (classical)

| API | Description |
|-----|-------------|
| `nnlp.Word2Vec.train(sentences)` | CBOW or skip-gram + negative sampling |
| `nnlp.Word2Vec.most_similar(word, topn)` | Cosine neighbors |
| `nnlp.Word2Vec.analogy(pos, neg, topn)` | Vector analogy |

## Similarity & ranking

| Function | Description |
|----------|-------------|
| `nnlp.cosine(a, b)` | Cosine similarity |
| `nnlp.jaccard(a, b)` | Jaccard index |
| `nnlp.levenshtein(a, b)` | Edit distance |
| `nnlp.jaro_winkler(a, b)` | Jaro–Winkler similarity |
| `nnlp.Bm25.fit(docs)` / `.rank(query)` | BM25 ranking |

## Baselines

| Function | Description |
|----------|-------------|
| `nnlp.detect_language(text)` | N-gram profile language ID |
| `nnlp.sentiment(text)` | Lexicon baseline (pos/neg/neutral) |
| `nnlp.keywords(text, topn)` | RAKE + TF-IDF fallback |

## Errors (4080–4089)

| Code | Meaning |
|------|---------|
| 4080 | Arity mismatch |
| 4081 | General error |
| 4082 | Type error |
| 4083 | Estimator not fitted |
| 4084 | Empty vocabulary after pruning |
| 4085 | Shape mismatch |
| 4086 | Out-of-vocabulary (word2vec) |

## Example

See `examples/nnlp_demo.niao` — clean → vectorize → similarity pipeline.
