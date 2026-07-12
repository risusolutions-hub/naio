# neval standard library

Model evaluation metrics: exact match, token-F1, string similarity, classification and regression scores, dataset runner, and latency benchmarking.

## Import

```niao
import "neval"
```

Paths `import "std/neval"` and `import "neval"` are equivalent. Flat builtins (`neval_exact`, `neval_run`, …) are also available globally after import.

## Quick start

```niao
import "neval"

print(neval.exact("hello", "hello"))           // true
print(neval.token_f1("a b c", "a b d"))
print(neval.accuracy(["a", "b"], ["a", "c"]))

let data = [
    { input: "2+2", expected: "4" },
    { input: "3+3", expected: "6" },
]
fn predict(x) { return "4" }
print(neval.run(data, predict))
```

Run: `niao run examples/neval_demo.niao`

## Text metrics

| Method | Description |
|--------|-------------|
| `neval.exact(a, b)` | Case-sensitive string equality. |
| `neval.similarity(a, b)` | `1 - normalized_levenshtein` in `[0, 1]`. |
| `neval.token_f1(pred, reference)` | `{precision, recall, f1}` over whitespace tokens (case-insensitive). |

## Classification

| Method | Description |
|--------|-------------|
| `neval.accuracy(preds, labels)` | Fraction correct (string arrays, equal length). |
| `neval.precision(preds, labels)` | Macro-averaged precision. |
| `neval.recall(preds, labels)` | Macro-averaged recall. |
| `neval.f1(preds, labels)` | Macro-averaged F1. |
| `neval.confusion(preds, labels)` | Nested object: `actual_label -> predicted_label -> count`. |

## Regression

| Method | Description |
|--------|-------------|
| `neval.mae(preds, labels)` | Mean absolute error (number arrays). |
| `neval.mse(preds, labels)` | Mean squared error. |
| `neval.rmse(preds, labels)` | Root mean squared error. |
| `neval.r2(preds, labels)` | Coefficient of determination R². |

## Runner & bench

| Method | Description |
|--------|-------------|
| `neval.run(dataset, predict_fn, opts?)` | Call `predict_fn(input)` per row; returns `{count, exact, accuracy, avg_similarity, avg_token_f1, macro_f1}`. Dataset rows are objects with `input` / `expected` (override via `opts.input_key` / `opts.expected_key`). |
| `neval.bench(fn, iters?)` | Time `fn()` (default 100 iterations); returns `{iters, mean_ms, min_ms, max_ms, p50_ms, p95_ms}`. |
| `neval.compare(a, b)` | Numeric delta object for keys present in both metric maps. |

## Errors

| Code | Meaning |
|------|---------|
| 2760 | Wrong argument count. |
| 2761 | Empty dataset or bench failure (catchable). |
| 2762 | Wrong argument type. |
| 2763 | Shape mismatch — unequal array lengths, bad dataset row (hard error). |
