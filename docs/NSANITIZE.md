# nsanitize standard library

Allowlist HTML sanitizer for user content (XSS-safe), URL scheme policy. Native Rust implementation (~bleach + nh3 subset).

## Import

```niao
import "nsanitize"
```

Paths `import "std/nsanitize"` and `import "nsanitize"` are equivalent. Flat builtins (`nsanitize_clean`, `nsanitize_linkify`, …) are also available globally after import.

## Quick start

```niao
import "nsanitize"

let dirty = "<b>hi</b><script>alert(1)</script>"
print(nsanitize.clean(dirty))                    // "<b>hi</b>"

print(nsanitize.strip("<p>text</p>"))            // "text"
print(nsanitize.linkify("see https://example.com"))

print(nsanitize.allowed_url("https://x.com"))    // true
print(nsanitize.allowed_url("javascript:alert")) // false

let h = nsanitize.compile({tags: ["b", "i", "a"], protocols: ["http", "https"]})
print(nsanitize.apply(h, dirty))
nsanitize.close(h)
```

## Sanitize & strip

| Method | Description |
|--------|-------------|
| `nsanitize.clean(html, opts?)` | Allowlist HTML sanitization. Strips `<script>`/`<style>` content, blocks event handlers and disallowed URL schemes. |
| `nsanitize.strip(html, opts?)` | Remove all tags; keep text. `opts`: `{strip_comments}`. |
| `nsanitize.clean_text(text)` | Escape plain text for safe HTML insertion (no tags). |
| `nsanitize.escape(text, opts?)` | HTML escape. `opts`: `{attribute: true}` for attribute context. |
| `nsanitize.is_html(text)` | True when input contains HTML markup. |

### `clean` / `compile` options

| Option | Default | Description |
|--------|---------|-------------|
| `tags` | bleach/ammonia default set | Allowed tag names (array). Empty array strips all tags. |
| `attributes` | per-tag defaults | Object mapping tag → allowed attribute names. |
| `generic_attributes` | `[]` | Attributes allowed on any tag (e.g. `id`, `title`). |
| `protocols` | `http`, `https`, `mailto`, `ftp` | Allowed URL schemes in `href`/`src`. |
| `strip_comments` | `true` | Remove HTML comments. |
| `link_rel` | `"noopener noreferrer"` | `rel` on links; set `null` to omit. |
| `nofollow_links` | `false` | Append `nofollow` to `rel`. |
| `relative_urls` | `"pass"` | `"pass"`, `"drop"`, or `"sanitize"`. |
| `allowed_classes` | `{}` | Per-tag class allowlist. |
| `clean_content_tags` | `script`, `style` | Tags whose *contents* are removed. |

## Linkify

| Method | Description |
|--------|-------------|
| `nsanitize.linkify(text, opts?)` | Turn bare URLs (and optionally emails) into `<a>` tags, then sanitize. |

Linkify options: `parse_email` (default `true`), `new_tab` (default `true`), `nofollow`, `sanitize_after` (default `true`), plus all `clean` options.

## URL policy

| Method | Description |
|--------|-------------|
| `nsanitize.allowed_url(url, opts?)` | Fast scheme check. `opts`: `{protocols}`. Blocks `javascript:`, `data:`, `vbscript:` by default. |

Relative paths (`/x`, `#frag`, `./x`) are always allowed.

## Compiled sanitizer

| Method | Description |
|--------|-------------|
| `nsanitize.compile(opts?)` | Build reusable handle from options. |
| `nsanitize.apply(handle, html)` | Sanitize with compiled policy. |
| `nsanitize.close(handle)` | Free handle. |

## Batch & introspection

| Method | Description |
|--------|-------------|
| `nsanitize.parallel_clean(htmls, opts?)` | Parallel batch clean. `opts.threads` defaults to CPU count. |
| `nsanitize.default_tags()` | Default allowed tag list. |
| `nsanitize.default_attributes()` | Default per-tag attribute map. |
| `nsanitize.default_protocols()` | Default URL schemes. |

## Errors

| Code | Meaning |
|------|---------|
| 3538 | Wrong argument count. |
| 3539 | Operation failed — catchable `nsanitize_error`. |
| 3540 | Wrong argument type. |
| 3541 | Invalid or closed handle — catchable `nsanitize_error`. |

Input limit: 16 MiB per string.

## Deferred vs Python bleach

Not in v0.1.0: CSS property sanitization (`style` attribute rules / tinycss2), custom per-element callbacks/filters, `Cleaner` strip-vs-escape mode for disallowed tags (disallowed tags are always removed; text kept), and bleach's `version`/`summary` metadata helpers. Use `nhtml` for DOM parse/select workflows; use `nsanitize` for security-focused cleaning.
