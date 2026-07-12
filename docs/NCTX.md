# nctx standard library

Token estimates, message trim strategies, context budgets, and conversation stats for LLM prompt planning. Offline heuristics only — no tokenizer API calls.

## Import

```niao
import "nctx"
```

Paths `import "std/nctx"` and `import "nctx"` are equivalent.

## Quick start

```niao
import "nctx"

let msgs = [
    {role: "system", content: "You are helpful."},
    {role: "user", content: "Summarize this long thread…"},
    {role: "assistant", content: "Here is a summary…"}
]

print(nctx.estimate("hello world"))              // ~3 tokens (chars/4)
print(nctx.estimate_messages(msgs))

let budget = nctx.budget(8192, 512)              // reserve 512 for completion
print(nctx.fits(msgs, budget))                   // true/false

let trimmed = nctx.trim(msgs, 32, "tail")        // keep recent within budget
print(nctx.stats(trimmed))
```

## Token estimation

Uses a fast **chars ÷ 4** heuristic (minimum 1 token per text) plus ~4 tokens framing overhead per message. Suitable for preflight budgeting, not exact billing.

## Trim strategies

| Strategy | Behavior |
|----------|----------|
| `tail` (default) | Drop oldest messages until within budget (keeps recent). |
| `head` | Keep earliest messages, drop from the end. |
| `middle` | Keep prefix + suffix, drop from the middle. |
| `system` | Always keep `role: "system"` messages; trim other roles from the tail. |

Messages are `{role, content}` objects (same shape as `nagent.messages`).

## Functions

| Method | Description |
|--------|-------------|
| `nctx.estimate(text)` | Token estimate for a string. |
| `nctx.estimate_messages(msgs)` | Sum of per-message estimates. |
| `nctx.trim(msgs, budget, strategy?)` | Trimmed message array. |
| `nctx.stats(msgs)` | `{messages, chars, tokens, roles: {role: count}}`. |
| `nctx.budget(max, reserve?)` | `{max, reserve, available, used}` — `used` starts at 0. |
| `nctx.fits(msgs, budget)` | `true` when `estimate_messages(msgs) <= budget.available`. |

## Errors

| Code | Meaning |
|------|---------|
| 3340 | Wrong argument count. |
| 3341 | Negative budget/reserve (catchable). |
| 3342 | Wrong argument type. |
