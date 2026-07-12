#!/usr/bin/env python3
"""Benchmark nnlp vs sklearn/nltk. Run from repo root."""

import subprocess
import sys
import time

try:
    from sklearn.feature_extraction.text import TfidfVectorizer
except ImportError:
    print("sklearn required: pip install scikit-learn")
    sys.exit(1)

N_DOCS = 100_000
docs = [
    f"document number {i} about cats dogs and natural language processing"
    for i in range(N_DOCS)
]

# sklearn baseline
t0 = time.perf_counter()
tv = TfidfVectorizer()
_ = tv.fit_transform(docs)
sklearn_ms = (time.perf_counter() - t0) * 1000

# niao_nlp via release example
result = subprocess.run(
    [
        "cargo", "run", "--manifest-path", "crates/niao_nlp/Cargo.toml",
        "--release", "--example", "bench_vectorize",
    ],
    capture_output=True,
    text=True,
    cwd=".",
)

niao_ms = None
if result.returncode == 0:
    niao_ms = float(result.stdout.strip())
else:
    print("Rust bench failed:", result.stderr, file=sys.stderr)

print(f"TF-IDF fit_transform {N_DOCS} short docs:")
print(f"  sklearn:    {sklearn_ms:.1f} ms")
if niao_ms is not None:
    ratio = niao_ms / sklearn_ms if sklearn_ms > 0 else float("inf")
    print(f"  niao_nlp:   {niao_ms:.1f} ms  ({ratio:.2f}x sklearn)")
else:
    print("  niao_nlp:   (failed)")

# Porter stemmer micro-bench via nltk if available
try:
    from nltk.stem.porter import PorterStemmer
    words = ["running", "connections", "national", "processing"] * 25_000
    ps = PorterStemmer()
    t0 = time.perf_counter()
    for w in words:
        ps.stem(w)
    nltk_ms = (time.perf_counter() - t0) * 1000
    print(f"Porter stem 100k tokens: nltk {nltk_ms:.1f} ms")
except ImportError:
    print("nltk not installed — skipping Porter bench")
