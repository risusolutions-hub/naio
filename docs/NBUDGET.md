# nbudget standard library

Unified cooperative resource and cost budgets. Set soft limits for CPU %, RAM MB, GPU %, USD, and tokens; charge usage; check remaining headroom. No OS enforcement — accounting only.

## Import

```niao
import "nbudget"
```

Paths `import "std/nbudget"` and `import "nbudget"` are equivalent. Flat builtins (`nbudget_set`, `nbudget_charge`, …) are also available globally after import.

## Quick start

```niao
import "nbudget"

nbudget.set({ram_mb: 2048, usd: 5.0, tokens: 100000})

if nbudget.ok() {
    nbudget.charge("tokens", 1500)
    nbudget.charge("usd", 0.02)
}

let r = nbudget.check({tokens: 5000})   // proposed extra usage
if !r.ok {
    for v in r.violations { print(v) }
}

print(nbudget.remain())   // leftover per set limit
print(nbudget.used())     // charged amounts
```

## Limits

| Key | Meaning |
|-----|---------|
| `cpu_pct` | Advisory CPU utilization percent. |
| `ram_mb` | Advisory RAM budget in megabytes. |
| `gpu_pct` | Advisory GPU utilization percent. |
| `usd` | Spend ceiling in dollars. |
| `tokens` | Token / unit budget. |

All keys are optional numbers (`int` or `float`). Unset keys are unlimited.

## Functions

| Method | Description |
|--------|-------------|
| `nbudget.set(obj)` | Replace global limits from the object keys above. |
| `nbudget.get()` | Current limits object (unset keys omitted). |
| `nbudget.clear()` | Clear all limits (usage counters kept). |
| `nbudget.check(extra?)` | Soft check: `{ok: bool, violations: [string]}`. Optional `extra` is proposed additional usage. |
| `nbudget.ok()` | `true` when current used is within all set limits. |
| `nbudget.remain()` | `limit − used` for each set limit (may be negative if over). |
| `nbudget.charge(kind, amount)` | Accumulate usage for `kind`. Always charges; returns catchable exceed error if used is now over a set limit. |
| `nbudget.used()` | Charged amounts (zero keys omitted). |
| `nbudget.reset_used()` | Zero all usage counters. |

`kind` must be one of: `cpu_pct`, `ram_mb`, `gpu_pct`, `usd`, `tokens`.

## Errors

| Code | Meaning |
|------|---------|
| 2940 | Wrong argument count. |
| 2941 | Operation error (unknown kind/key, negative amount). |
| 2942 | Wrong argument type. |
| 2943 | Charge pushed usage over a set limit (catchable `error`; usage still recorded). |
