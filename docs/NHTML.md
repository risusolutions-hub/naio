# nhtml standard library

Forgiving HTML5 parser, CSS selectors, tree walking, text extraction, escape/unescape. Native Rust implementation (~BeautifulSoup4 subset).

## Import

```niao
import "nhtml"
```

Paths `import "std/nhtml"` and `import "nhtml"` are equivalent. Flat builtins (`nhtml_parse`, `nhtml_select`, …) are also available globally after import.

## Quick start

```niao
import "nhtml"

let doc = nhtml.parse("<html><body><p class='lead'>Hello <b>world</b></p></body></html>")
let p = nhtml.select_one(doc, "p.lead")
print(nhtml.tag(p))              // "p"
print(nhtml.text(p, {strip: true}))  // "Hello world"
print(nhtml.attr(p, "class"))    // "lead"

let links = nhtml.select(doc, "a[href]")
for n in links {
    print(nhtml.attr(n, "href"))
}

nhtml.close(doc)
```

## Parse & handles

| Method | Description |
|--------|-------------|
| `nhtml.parse(html, opts?)` | Parse a document → document handle. `opts`: `{fragment: false}`. |
| `nhtml.parse_fragment(html)` | Parse an HTML fragment. |
| `nhtml.close(doc)` | Release a document handle. |
| `nhtml.root(doc)` | Root element node handle. |

Document and node handles are positive integers. Node handles are valid only while their document is open.

## CSS selectors

| Method | Description |
|--------|-------------|
| `nhtml.select(doc, css)` | All matches from document root. |
| `nhtml.select_one(doc, css)` | First match or `nil`. |
| `nhtml.select_on(node, css)` | Select within a subtree. |
| `nhtml.select_one_on(node, css)` | First match under `node`. |
| `nhtml.compile_selector(css)` | Compile CSS → selector handle. |
| `nhtml.close_selector(sel)` | Release selector handle. |
| `nhtml.select_with(node, sel)` | Select using compiled selector. |
| `nhtml.matches(node, css)` | True when `node` matches selector. |
| `nhtml.valid_selector(css)` | True when CSS syntax is valid. |

## Node metadata

| Method | Description |
|--------|-------------|
| `nhtml.tag(node)` | Element tag name (lowercase). |
| `nhtml.attr(node, name)` | Attribute value or `nil`. |
| `nhtml.attrs(node)` | Object of all attributes. |
| `nhtml.has_attr(node, name)` | Attribute present. |
| `nhtml.id(node)` | `id` attribute. |
| `nhtml.classes(node)` | `class` tokens as string array. |
| `nhtml.has_class(node, name)` | Class token present. |
| `nhtml.node_type(node)` | `"element"`, `"text"`, `"comment"`, … |
| `nhtml.is_element(node)` | True for elements. |
| `nhtml.is_text(node)` | True for text nodes. |
| `nhtml.is_comment(node)` | True for comments. |
| `nhtml.is_tag(node, name)` | True when element tag matches. |

## Text & serialization

| Method | Description |
|--------|-------------|
| `nhtml.text(node, opts?)` | Descendant text. `opts`: `{strip, separator}`. |
| `nhtml.direct_text(node)` | Direct child text nodes only. |
| `nhtml.html(node)` | Outer HTML. |
| `nhtml.inner_html(node)` | Inner HTML. |
| `nhtml.prettify(node, opts?)` | Pretty-printed HTML. `opts.indent` (default `2`). |
| `nhtml.strip_tags(html)` | Fast tag strip (no DOM). |
| `nhtml.extract_text(html, selector?, opts?)` | One-shot parse + text. |

## Tree walking

| Method | Description |
|--------|-------------|
| `nhtml.parent(node)` | Parent node or `nil`. |
| `nhtml.children(node)` | Child node handles. |
| `nhtml.child_elements(node)` | Child elements only. |
| `nhtml.descendants(node)` | All descendant nodes (excludes self). |
| `nhtml.ancestors(node)` | Ancestor nodes (excludes self). |
| `nhtml.next_sibling(node)` | Next sibling or `nil`. |
| `nhtml.prev_sibling(node)` | Previous sibling or `nil`. |
| `nhtml.siblings(node)` | Sibling node handles. |
| `nhtml.find(node, tag, opts?)` | First descendant by tag; `opts.attrs` for one attribute filter. |
| `nhtml.find_all(node, tag, opts?)` | All descendants by tag/attrs. |

## Escape

| Method | Description |
|--------|-------------|
| `nhtml.escape(text)` | Escape for HTML body. |
| `nhtml.escape_attr(text)` | Escape for attribute values. |
| `nhtml.unescape(text)` | Decode HTML entities. |

## Parallel batch

| Method | Description |
|--------|-------------|
| `nhtml.parallel_extract(htmls, selector?, opts?)` | Parallel text extraction. `opts.threads` defaults to CPU count. |
| `nhtml.parallel_select(htmls, css, opts?)` | Parse each HTML and select; returns `[{doc, nodes}, …]`. Call `nhtml.close` on each `doc`. |

## Errors

| Code | Meaning |
|------|---------|
| 3542 | Wrong argument count. |
| 3543 | Operation failed (parse/selector) — catchable `nhtml_error`. |
| 3544 | Wrong argument type. |
| 3545 | Invalid or closed handle — catchable `nhtml_error`. |

## Deferred vs BeautifulSoup4

Not in v0.1.0: DOM mutation (`decompose`, `append`, `insert`, attribute setters), `NavigableString`/`Tag` distinct types, XML mode, `lxml`/`html.parser` backend choice, full multi-attribute `find` dict filters, `SoupStrainer` parse-time filtering, automatic encoding detection, and `diagnose`/formatter hooks. Use `nhtml.extract_text` / `strip_tags` for lightweight scraping without handles.
