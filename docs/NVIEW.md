# nview standard library

Jinja-style templating: inheritance, blocks, filters, autoescape, partials — for HTML/text output (~jinja2 subset). Distinct from `ntemplate` (LLM prompt `{{var}}` templates).

## Import

```niao
import "nview"
```

Paths `import "std/nview"` and `import "nview"` are equivalent. Flat builtins (`nview_render`, `nview_compile`, …) are also available globally after import.

## Quick start

```niao
import "nview"

let html = nview.render(
    "<h1>{{ title|e }}</h1><ul>{% for u in users %}<li>{{ u.name|e }}</li>{% endfor %}</ul>",
    {title: "Team", users: [{name: "Ada"}, {name: "Grace"}]},
    {autoescape: true}
)
print(html)

// Multi-template inheritance
let env = nview.env({autoescape: true})
nview.add(env, "base.html", "<!doctype html><body>{% block body %}{% endblock %}</body>")
nview.add(env, "page.html", "{% extends \"base.html\" %}{% block body %}<p>{{ msg|e }}</p>{% endblock %}")
print(nview.render_named(env, "page.html", {msg: "Hello <world>"}))
nview.env_close(env)
```

Run: `niao run examples/nview_demo.niao`

## One-shot & compile

| Method | Description |
|--------|-------------|
| `nview.render(source, ctx, opts?)` | Render a template string. `opts`: `{autoescape, keep_trailing_newline, trim_blocks, lstrip_blocks}`. |
| `nview.compile(source, opts?)` | Compile → positive template handle. |
| `nview.run(tpl, ctx)` | Render a compiled handle. |
| `nview.close(tpl)` | Free a template handle. Returns `true` if removed. |

`autoescape` accepts `true`/`"html"`, `false`/`"none"`, or `"auto"` (by template name extension).

## Environment (inheritance & partials)

| Method | Description |
|--------|-------------|
| `nview.env(opts?)` | Create an environment handle. |
| `nview.add(env, name, source)` | Register/replace a named template (`{% extends %}` / `{% include %}`). |
| `nview.has(env, name)` | Whether `name` is registered. |
| `nview.names(env)` | Sorted template names. |
| `nview.remove(env, name)` | Remove a named template. |
| `nview.render_named(env, name, ctx)` | Render by name. |
| `nview.render_in(env, source, ctx)` | Render anonymous source with env templates available. |
| `nview.env_close(env)` | Free the environment. |

## Files

| Method | Description |
|--------|-------------|
| `nview.render_file(path, ctx, opts?)` | Load a file and render it. |
| `nview.add_file(env, name, path)` | Load a file into the environment. |
| `nview.load_dir(env, dir)` | Load `*.html` / `*.htm` / `*.j2` / `*.jinja` / `*.jinja2` / `*.txt` from a directory. Returns count. |

## Introspection & escape

| Method | Description |
|--------|-------------|
| `nview.valid(source)` | `true` when the template parses. |
| `nview.vars(source)` | Undeclared top-level variable names. |
| `nview.filters()` | Built-in filter names (`upper`, `escape`, `join`, …). |
| `nview.escape(s)` / `nview.escape_attr(s)` | HTML-escape. |
| `nview.unescape(s)` | Decode HTML entities. |
| `nview.batch(source_or_tpl, ctxs, opts?)` | Render many contexts in parallel. `opts.threads` optional. |

## Errors

Catchable errors use kind `nview_error`. Arity/type mistakes throw.

| Code | Meaning |
|------|---------|
| 4470 | Wrong argument count. |
| 4471 | Render / IO / semantic error (catchable). |
| 4472 | Wrong argument type. |
| 4473 | Invalid template or env handle (catchable). |
| 4474 | Template parse / syntax error (catchable). |
