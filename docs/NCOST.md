# ncost standard library

Rough preflight USD estimates for LLM token usage, S3 storage, and Lambda compute. Built-in model price table with thread-local overrides — no network calls, no external crates.

## Import

```niao
import "ncost"
```

Paths `import "std/ncost"` and `import "ncost"` are equivalent. Flat builtins (`ncost_price`, `ncost_estimate`, …) are also available globally after import.

## Quick start

```niao
import "ncost"

print(ncost.price("gpt-4o", 1000, 500))   // USD for 1k in + 500 out

let e = ncost.estimate({
    model: "gpt-4o-mini",
    tokens_in: 10000,
    tokens_out: 2000,
    s3_gb: 5,
    lambda_ms: 200,
    requests: 1000
})
print(e.usd, e.breakdown)

ncost.set_price("my-model", 1.0, 3.0)     // $/million tokens in/out
print(ncost.table())
```

## Built-in prices

Per **million tokens** (USD), approximate public list prices:

| Model | `in_per_mtok` | `out_per_mtok` |
|-------|---------------|----------------|
| `gpt-4o` | 2.5 | 10 |
| `gpt-4o-mini` | 0.15 | 0.6 |
| `claude-sonnet` | 3 | 15 |
| `llama-local` | 0 | 0 |

S3 helper uses ~`$0.023` per GB. Lambda helper uses ~`$0.0000166667` per GB-second assuming 1 GB memory: `(ms / 1000) * requests * 0.0000166667`.

These are **rough planning numbers**, not billing truth.

## Functions

| Method | Description |
|--------|-------------|
| `ncost.price(model, tokens_in, tokens_out?)` | USD float for token usage. `tokens_out` defaults to `0`. Unknown model → catchable error. |
| `ncost.estimate(obj)` | `{usd, breakdown}` from optional keys `model`, `tokens_in`, `tokens_out`, `s3_gb`, `lambda_ms`, `requests` (default `1`). |
| `ncost.table()` | Object of known models → `{in_per_mtok, out_per_mtok}` (builtins + overrides). |
| `ncost.set_price(model, in_per_mtok, out_per_mtok)` | Thread-local override / custom model. Returns `nil`. |
| `ncost.s3_cost(gb)` | Rough S3 storage USD for `gb` gigabytes. |
| `ncost.lambda_cost(ms, requests?)` | Rough Lambda compute USD; `requests` defaults to `1`. |

`estimate` breakdown keys present when applicable: `llm`, `model`, `s3`, `lambda`.

## Errors

| Code | Meaning |
|------|---------|
| 2950 | Wrong argument count. |
| 2951 | Unknown model, negative amounts, or other semantic error (catchable). |
| 2952 | Wrong argument type. |
