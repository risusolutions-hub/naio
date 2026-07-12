# ncolor standard library

ANSI terminal styling: 16 named colors, bright variants, 256-color and truecolor RGB, text attributes, and `strip()`. Honors the `NO_COLOR` convention and can be toggled at runtime — when disabled every function returns its input unchanged.

## Import

```niao
import "ncolor"
```

Paths `import "std/ncolor"` and `import "ncolor"` are equivalent. Flat builtins (`ncolor_red`, `ncolor_style`, …) are also available globally after import.

## Quick start

```niao
import "ncolor"

print(ncolor.green("✓ build passed"))
print(ncolor.bold(ncolor.red("✗ 2 tests failed")))
print(ncolor.style("deploy", {fg: "black", bg: "yellow", bold: true}))
print(ncolor.rgb("niao", 255, 128, 0))
```

## Named colors & attributes

Single-argument helpers: `black red green yellow blue magenta cyan white gray` and `bold dim italic underline strike reverse`.

| Method | Description |
|--------|-------------|
| `ncolor.fg(s, name)` / `ncolor.bg(s, name)` | Any named color incl. `bright_*` variants. |
| `ncolor.rgb(s, r, g, b)` / `ncolor.on_rgb(s, r, g, b)` | Truecolor fore/background. |
| `ncolor.c256(s, index)` | 256-palette foreground. |

## Composite styling

```niao
ncolor.style(text, {
    fg: "red",           // name, 0..=255 int, or [r, g, b]
    bg: [30, 30, 30],
    bold: true, underline: true
})
```

Attributes: `bold`, `dim`, `italic`, `underline`, `blink`, `reverse`, `strike`.

## Control

| Method | Description |
|--------|-------------|
| `ncolor.strip(s)` | Remove all ANSI escape sequences (for logs/files). |
| `ncolor.set_enabled(bool)` / `ncolor.is_enabled()` | Runtime toggle. Startup default: enabled unless `NO_COLOR` is set. |

## Errors

| Code | Meaning |
|------|---------|
| 2690 | Wrong argument count. |
| 2691 | Unknown color name or out-of-range RGB/palette value. |
