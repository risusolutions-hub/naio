# nprompt standard library

Interactive TTY prompts on stdin/stdout. Falls back to plain line reads when stdin is not a terminal (pipes, redirects, CI). Std-only — no external prompt crates.

## Import

```niao
import "nprompt"
```

Paths `import "std/nprompt"` and `import "nprompt"` are equivalent. Flat builtins (`nprompt_input`, `nprompt_confirm`, …) are also available globally after import.

## Quick start

```niao
import "nprompt"

let name = nprompt.input("Your name", { default: "guest" })
let ok   = nprompt.confirm("Continue?", { default: true })
let color = nprompt.select("Pick a color", ["red", "green", "blue"], { default_index: 0 })
let pass = nprompt.password("Password")

print("Hello", name, ok, color)
```

Run interactively: `niao run examples/nprompt_demo.niao`

## Functions

| Method | Description |
|--------|-------------|
| `nprompt.input(label, opts?)` | Print `label`, read one line from stdin. Returns the line, or `opts.default` when the user presses Enter on an empty line. |
| `nprompt.confirm(label, opts?)` | y/n prompt. Returns `true` or `false`. Empty line uses `opts.default` when set; re-prompts on invalid input in TTY mode. |
| `nprompt.select(label, choices, opts?)` | Pick from `choices` (string array). TTY: numbered menu (1-based), returns the selected string. Pipe mode: numeric input returns an **int index** (0-based); matching label returns a **string**. Empty line uses `opts.default_index`. |
| `nprompt.password(label)` | Read a secret line. No echo on Unix TTY (via `stty`); best-effort echo on Windows and non-TTY stdin. |

### Options

| Key | Used by | Type | Description |
|-----|---------|------|-------------|
| `default` | `input` | string | Value when the user submits an empty line. |
| `default` | `confirm` | bool | Default answer for an empty line (`[Y/n]` / `[y/N]` hints in TTY mode). |
| `default_index` | `select` | int | 0-based index used when the user submits an empty line. |

Stdout is flushed before every read. Labels use dim/bold ANSI styling in TTY mode when `NO_COLOR` is unset.

## Non-TTY / pipe mode

When stdin or stdout is not a terminal, prompts use a simple `label: ` format (no numbered menu, no re-prompt loops for confirm). Scripts can pipe answers:

```bash
echo "alice" | niao run script.niao          # nprompt.input
echo "y"     | niao run script.niao          # nprompt.confirm
echo "1"     | niao run script.niao          # nprompt.select → int 1
echo "green" | niao run script.niao          # nprompt.select → "green"
```

## Errors

| Code | Meaning |
|------|---------|
| 2920 | Wrong argument count. |
| 2921 | I/O failure or invalid selection (catchable `nprompt_error`). |
| 2922 | Type error — wrong argument type. |
