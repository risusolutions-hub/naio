# nargs standard library

Declarative CLI argument parsing: flags, typed options, positionals, `--key=value`, short bundling (`-abc`), `--` terminator, and generated `--help` text.

## Import

```niao
import "nargs"
```

Paths `import "std/nargs"` and `import "nargs"` are equivalent. Flat builtins (`nargs_parse`, …) are also available globally after import.

## Quick start

```niao
import "nargs"

let spec = {
    name: "greet",
    about: "Greets people, fast",
    flags: [
        {name: "verbose", short: "v", help: "Chatty output"}
    ],
    options: [
        {name: "port", short: "p", type: "int", default: 8080, help: "Listen port"},
        {name: "mode", type: "string", required: true, help: "run mode"}
    ],
    positionals: [
        {name: "input", required: true, help: "Input file"},
        {name: "extras", variadic: true, help: "More files"}
    ]
}

let r = nargs.parse_env(spec)          // parses the real process argv
if is_error(r) { print(r) } else {
    if r.help { print(r.text) } else {
        print(r.values.port)
        print(r.values.input)
    }
}
```

## Spec object

| Key | Description |
|-----|-------------|
| `name`, `about` | Program name/summary for help text. |
| `flags` | `[{name, short?, help?}]` — boolean switches, default `false`. |
| `options` | `[{name, short?, type?, default?, required?, help?}]` — `type`: `string` (default), `int`, `float`, `bool`. |
| `positionals` | `[{name, required?, variadic?, help?}]` — variadic must be last, collects the remainder as an array. |

## Functions

| Method | Description |
|--------|-------------|
| `nargs.parse(spec, argv)` | Parse an explicit argv array. |
| `nargs.parse_env(spec)` | Parse the real process arguments. |
| `nargs.help(spec)` | Generated usage text. |
| `nargs.argv()` | Raw process argv (after the program name). |

## Result object

`{ok: true, help: bool, values: {...}, rest: [...]}`

- `values` — every flag/option/positional by name (defaults applied, missing optionals are `nil`).
- `rest` — tokens after `--` plus extra positionals beyond the spec.
- `--help`/`-h` short-circuits: `help` is `true` and `text` holds the usage string.
- Parse problems (unknown option, bad int, missing required) return a catchable `error` value — use `is_error()` or try/catch.

## Syntax accepted

`--port 8080`, `--port=8080`, `-p 8080`, flag bundling `-abc` (a value-taking short must be last), `--` stops option parsing. Note: bare tokens starting with `-` (like negative numbers) must come after `--`.

## Errors

| Code | Meaning |
|------|---------|
| 2650 | Wrong argument count to the builtin itself. |
| 2651 | argv parse error (catchable `error` value). |
| 2652 | Invalid spec (hard error — programmer mistake). |
