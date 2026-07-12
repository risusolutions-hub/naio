# ntok standard library

Byte-level BPE tokenizer (GPT-2 style): encode/decode/count, per-word cache, approximate counting, chunking, and context-budget fitting. Std-only — no external tokenizer crates.

## Import

```niao
import "ntok"
```

Paths `import "std/ntok"` and `import "ntok"` are equivalent. Flat builtins (`ntok_encode`, `ntok_count`, …) are also available globally after import.

## Quick start

```niao
import "ntok"

let tok = ntok.builtin()
let text = "Hello, world! Token counting for context budgets."
print(ntok.count(tok, text))
print(ntok.encode(tok, text))
print(ntok.chunk(tok, text, 10))
print(ntok.fit(tok, text, 12))
ntok.close(tok)
```

Run: `niao run examples/ntok_demo.niao`

## Functions

| Method | Description |
|--------|-------------|
| `ntok.builtin()` | Create a built-in byte-level BPE tokenizer handle (offline, no files). |
| `ntok.load(vocab_path, merges_path?)` | Load GPT-2-style `vocab.json` (+ optional `merges.txt`). Auto-detects sibling `merges.txt` when omitted. |
| `ntok.encode(handle, text)` | Returns `int_array` of token ids. |
| `ntok.decode(handle, ids)` | Decode token ids back to UTF-8 text. |
| `ntok.count(handle, text)` | Exact token count for the handle's vocabulary. |
| `ntok.count_approx(text)` | Fast heuristic count (no handle) for budgeting. |
| `ntok.chunk(handle, text, max_tokens)` | Split text into `string_array` chunks each ≤ `max_tokens`. |
| `ntok.fit(handle, text, max_tokens)` | Truncate text to fit within `max_tokens`. |
| `ntok.close(handle)` | Free the tokenizer handle. |

Built-in tokenizer uses GPT-2 byte encoding with a compact merge table. Per-word BPE results are cached on the handle for repeated counting/encoding.

## Errors

| Code | Meaning |
|------|---------|
| 2770 | Wrong argument count. |
| 2771 | Load/parse/decode failure (catchable). |
| 2772 | Wrong argument type. |
| 2773 | Invalid or closed tokenizer handle (catchable). |
