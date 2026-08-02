# nxml — XML DOM + streaming parser

XML DOM + streaming (SAX-style) parser, namespaces, XPath subset, pretty-print. Native Rust implementation (~`xml.etree`, `lxml` subset).

## Import

```niao
import "nxml"
```

Paths `import "std/nxml"` and `import "nxml"` are equivalent. Flat builtins (`nxml_parse`, `nxml_find`, …) are also available globally after import.

## Quick start

```niao
import "nxml"

let doc = nxml.parse(r#"<catalog><book id="1"><title>Hi</title></book></catalog>"#)
let root = nxml.root(doc)
let book = nxml.find(root, "book[@id='1']")
print(nxml.findtext(book, "title"))          // Hi

let out = nxml.pretty(root)
print(out)

nxml.close(doc)
```

Streaming (SAX-style):

```niao
let s = nxml.stream(large_xml)
loop {
    let ev = nxml.stream_next(s)
    if ev == nil { break }
    if ev.kind == "start" { print(ev.tag) }
}
nxml.stream_close(s)
```

## Parse & emit

| Method | Description |
|--------|-------------|
| `nxml.parse(source, opts?)` | Parse string or `byte[]` → document handle. |
| `nxml.fromstring(s, opts?)` | Alias for `parse`. |
| `nxml.parse_file(path, opts?)` | Read file and parse. |
| `nxml.tostring(node, opts?)` | Serialize document or element handle to XML string. |
| `nxml.pretty(node, indent?, opts?)` | Pretty-print with indentation (default two spaces). |
| `nxml.close(doc)` | Free document handle and its element handles. |
| `nxml.parallel_parse(sources, opts?)` | Parallel parse many strings → array of doc handles or errors. |

Parse `opts`: `{keep_comments, keep_pi, recover, huge_tree, xml_declaration, encoding, indent, pretty}`.

## DOM navigation

| Method | Description |
|--------|-------------|
| `nxml.root(doc)` | Root element handle. |
| `nxml.element(tag, attrs?, text?)` | Create a new one-element document; returns element handle. |
| `nxml.tag(elem)` / `nxml.set_tag(elem, tag)` | Local tag name. |
| `nxml.text(elem)` / `nxml.set_text(elem, text)` | Direct text content. |
| `nxml.tail(elem)` | Tail text after element (serialization). |
| `nxml.attrib(elem)` | Attribute object. |
| `nxml.get(elem, key, default?)` / `nxml.set(elem, key, value)` | Attribute access. |
| `nxml.keys(elem)` | Attribute names array. |
| `nxml.namespace(elem)` / `nxml.qname(elem)` | Namespace URI and qualified name. |
| `nxml.children(elem)` | Direct child element handles. |
| `nxml.parent(elem)` | Parent handle or `nil`. |
| `nxml.iter(elem, tag?)` | Depth-first element handles (optional tag filter). |
| `nxml.sub_element(parent, tag, attrs?, text?)` | Create and append child; returns new handle. |
| `nxml.clear(elem)` | Remove attributes, text, and children. |
| `nxml.copy(elem)` | Deep copy subtree → new document root handle. |

## XPath subset

| Method | Description |
|--------|-------------|
| `nxml.find(elem, path)` | First match or `nil`. |
| `nxml.findall(elem, path)` | All matches. |
| `nxml.findtext(elem, path, default?)` | Text of first match. |

Supported path syntax (ElementTree-style subset):

- Child steps: `item`, `/root/child`
- Descendant: `.//book`, `//entry`
- Wildcard: `*`
- Attribute predicates: `[@id]`, `[@id='x']`, `[@id!="x"]`
- Index: `[1]`, `[-1]` (last), `[2]`
- Namespace: `{http://example.com}tag`
- Text predicate: `[title='Hello']`

## Streaming

| Method | Description |
|--------|-------------|
| `nxml.stream(source, opts?)` | Open SAX-style stream handle. |
| `nxml.stream_next(stream)` | Next event object or `nil` at EOF. |
| `nxml.stream_close(stream)` | Close stream. |

Event `kind` values: `decl`, `start`, `end`, `text`, `comment`, `pi`. `start` events include `tag` and `attrs` object.

Stream `opts`: `{trim_text, expand_empty}`.

## Errors

| Code | Meaning |
|------|---------|
| 4310 | Wrong argument count. |
| 4311 | Operation failed — catchable `nxml_error`. |
| 4312 | Wrong argument type. |
| 4313 | Parse error — catchable `nxml_error`. |
| 4314 | Invalid or closed handle — catchable `nxml_error`. |

## Limits

- Maximum input size: **64 MiB** per parse/stream operation.
- Maximum nodes per document: **4M** (disable with `{huge_tree: true}`).

## Deferred vs lxml / full XPath 1.0

Not in v0.1.0: XML Schema / RelaxNG validation, XSLT, canonical XML (C14N), `iterparse` pull API with in-place mutation, custom entity resolvers, full XPath axes (`following-sibling`, `ancestor::`, functions), XInclude, HTML parsing, and in-place `remove()` / `append()` of existing subtree handles (use `sub_element` + `copy` instead).

## See also

- `json` — JSON parse/stringify.
- `nyaml` — YAML configuration files.
- `nsanitize` — HTML sanitization (not XML).
