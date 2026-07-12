# npace standard library

Adaptive loop pacing: a thread-local pace level (0..=3) maps to sleep delays
`0 / 2 / 8 / 25` ms. Use it to back off busy loops when temperature or load rises.

## Import

```niao
import "npace"
```

Paths `import "std/npace"` and `import "npace"` are equivalent. Flat builtins
(`npace_set_level`, `npace_tick`, …) are also available globally after import.

## Quick start

```niao
import "npace"

npace.set_level(npace.from_load(80))   // level 3
print(npace.sleep_ms())                // 25
npace.tick()                           // sleep 25 ms

let lvl = npace.from_temp(72, 90)      // map °C vs max → level
npace.set_level(lvl)
```

## Levels and delays

| Level | Sleep (ms) |
|-------|------------|
| 0 | 0 |
| 1 | 2 |
| 2 | 8 |
| 3 | 25 |

Default level is `0` (no delay). State is **thread-local**.

## Functions

| Method | Description |
|--------|-------------|
| `npace.set_level(n)` | Set pace level `0..=3`. Returns the level. Out of range → hard error. |
| `npace.level()` | Current pace level. |
| `npace.sleep_ms()` | Delay for the current level (`0/2/8/25`) — does **not** sleep. |
| `npace.tick()` | Sleep for the current level's delay; returns the ms slept. |
| `npace.from_temp(c, max)` | Map temperature `c` vs `max` → level `0..=3` (ratio quarters). `max` must be `> 0`. |
| `npace.from_load(pct)` | Map load percent → level (`0–24→0`, `25–49→1`, `50–74→2`, `75+→3`). Clamped to `0..=100`. |
| `npace.delays()` | Object `{ "0": 0, "1": 2, "2": 8, "3": 25 }`. |

`with_level` is not provided — native builtins cannot take callable function arguments.

## Mapping notes

- `from_temp(c, max)` uses `ratio = clamp(c / max, 0, 1)`, then
  `floor(ratio * 4)` capped at 3. So `0%→0`, `25%→1`, `50%→2`, `75%+→3`.
- `from_load(pct)` uses `floor(clamp(pct, 0, 100) / 25)` capped at 3.

## Errors

| Code | Meaning |
|------|---------|
| 3020 | Wrong argument count. |
| 3021 | Invalid level, non-positive `max`, or non-finite number. |
| 3022 | Type mismatch (expected int/number). |
