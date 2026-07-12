# nprovider standard library

Provider profiles, model aliases, failover chains, and a built-in LLM pricing table for offline planning. No network calls.

## Import

```niao
import "nprovider"
```

Paths `import "std/nprovider"` and `import "nprovider"` are equivalent.

## Quick start

```niao
import "nprovider"

nprovider.profile("openai-main", {
    provider: "openai",
    model:    "gpt-4o-mini",
    key_env:  "OPENAI_API_KEY"
})
nprovider.profile("anthropic-backup", {
    provider: "anthropic",
    model:    "claude-sonnet"
})
nprovider.alias("fast", "openai-main")

print(nprovider.resolve("fast").model)          // gpt-4o-mini
print(nprovider.price("gpt-4o-mini", 1e6, 0))   // ~0.15 USD

let chain = nprovider.chain(["fast", "anthropic-backup"])
print(nprovider.next(chain).key)                // fast
print(nprovider.next(chain).key)                // anthropic-backup (round-robin)
nprovider.close(chain)
```

## Functions

| Method | Description |
|--------|-------------|
| `nprovider.profile(name, config)` | Register `{provider, model, api_base?, key_env?}`. Returns `nil`. |
| `nprovider.alias(alias, target)` | Map alias → profile name. |
| `nprovider.resolve(key)` | Profile object `{name, provider, model, …}` or catchable error. |
| `nprovider.chain([keys])` | Failover chain handle (round-robin). |
| `nprovider.next(handle, advance?)` | `{chain, key, profile, index}`. Advances cursor by default; pass `false` to peek. |
| `nprovider.close(handle)` | Free chain; returns `true` if it existed. |
| `nprovider.price(model, tokens_in, tokens_out?)` | USD estimate from built-in / override table. |
| `nprovider.set_price(model, in_per_mtok, out_per_mtok)` | Thread-local price override ($/million tokens). |
| `nprovider.table()` | All known models → `{in_per_mtok, out_per_mtok}`. |
| `nprovider.list()` | `{profiles, aliases}`. |

### Built-in models

| Model | in $/Mtok | out $/Mtok |
|-------|-----------|------------|
| `gpt-4o` | 2.5 | 10 |
| `gpt-4o-mini` | 0.15 | 0.6 |
| `claude-sonnet` | 3 | 15 |
| `gemini-pro` | 1.25 | 5 |
| `llama-local` | 0 | 0 |

Rough planning numbers — not billing truth.

## Errors

| Code | Meaning |
|------|---------|
| 3330 | Wrong argument count. |
| 3331 | Unknown profile/alias/model, empty name, invalid chain (catchable). |
| 3332 | Wrong argument type. |
