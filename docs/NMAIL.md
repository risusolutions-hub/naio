# nmail — MIME email compose & parse

MIME email compose + parse with attachments, HTML+text alternatives, and inline images. Python `email` subset; pairs with `nsmtp` for transport.

## Import

```niao
import "nmail"
```

Paths `import "std/nmail"` and `import "nmail"` are equivalent. Flat builtins (`nmail_parse`, `nmail_emit`, …) are also available globally after import.

## Quick start

```niao
import "nmail"

let msg = nmail.build({
    from: "Ada <ada@example.com>",
    to: "bob@example.com",
    subject: "Welcome",
    text: "Hello in plain text",
    html: "<p>Hello in <b>HTML</b></p>",
    attachments: [{
        filename: "notes.txt",
        content_type: "text/plain",
        data: "secret notes"
    }],
    inline: [{
        cid: "logo",
        content_type: "image/png",
        data: byte_array[137, 80, 78, 71]
    }]
})

let raw = nmail.emit(msg)
let back = nmail.parse(raw)
print(back.subject, nmail.text(back))
```

Send with `nsmtp` by handing it the composed fields (or raw via your transport layer):

```niao
import "nsmtp"
import "nmail"

let msg = nmail.build({
    from: from_addr,
    to: to_addr,
    subject: "Hi",
    text: body,
    html: html_body
})
// nsmtp builds its own MIME today; use nmail for parse/inspect/round-trip
let preview = nmail.emit(msg)
```

## Compose

| Method | Description |
|--------|-------------|
| `nmail.build(opts)` | Build a message object from fields (see below). |
| `nmail.attach(msg, opts)` | Add an attachment; returns updated message. |
| `nmail.add_inline(msg, opts)` | Add a CID inline part; returns updated message. |
| `nmail.set_header(msg, name, value)` | Set/replace a header; returns updated message. |
| `nmail.emit(msg, opts?)` | Serialize to RFC 5322 / MIME text. |
| `nmail.emit_bytes(msg, opts?)` | Serialize to a bytearray. |
| `nmail.emit_file(path, msg, opts?)` | Write serialized message to a file; returns `true`. |

### Build options

| Key | Required | Description |
|-----|----------|-------------|
| `from` | yes | From mailbox (`Name <email>` or bare address). |
| `to` | yes | String, array of strings, or `{name,email}` objects. |
| `cc` / `bcc` | no | Same shape as `to`. |
| `reply_to` | no | Reply-To mailbox. |
| `subject` | no | Subject (non-ASCII encoded via RFC 2047 on emit). |
| `text` / `body` | no | Plain text body. |
| `html` | no | HTML body (creates `multipart/alternative` with `text`). |
| `attachments` | no | Array of `{filename?, content_type?, disposition?, data}`. |
| `inline` | no | Array of `{cid, filename?, content_type?, data}` (CID images). |
| `headers` | no | Extra header map. |
| `date` / `message_id` | no | Override generated values. |
| `auto_date` | no | Default `true`. |
| `auto_message_id` | no | Default `true`. |
| `msgid_domain` | no | Domain used by `make_msgid`. |

### Emit options

| Key | Default | Description |
|-----|---------|-------------|
| `crlf` | `true` | Use CRLF line endings. |

## Parse

| Method | Description |
|--------|-------------|
| `nmail.parse(text, opts?)` | Parse RFC 5322 text into a message object. |
| `nmail.parse_bytes(bytes, opts?)` | Parse raw bytes / bytearray. |
| `nmail.parse_file(path, opts?)` | Read a file and parse. |
| `nmail.valid(text)` | `true` when the text parses as a message. |

Parse failures return catchable `nmail_error` values (E2896).

## Message accessors

Parsed and built messages are objects with `kind: "message"`, `headers`, `from`, `to`, `cc`, `bcc`, `subject`, `text`, `html`, `attachments`, `inline`, `parts`, `multipart`, etc.

| Method | Description |
|--------|-------------|
| `nmail.get(msg, name)` | Header by name (case-insensitive), or `nil`. |
| `nmail.headers(msg)` | Header map. |
| `nmail.subject` / `from_addr` / `to_addrs` / `cc_addrs` / `bcc_addrs` / `reply_to` / `date` / `message_id` | Convenience getters. |
| `nmail.text(msg)` / `nmail.html(msg)` | Body parts (or `nil`). |
| `nmail.attachments(msg)` / `nmail.inline_parts(msg)` | Attachment / CID arrays. |
| `nmail.parts(msg)` / `nmail.walk(msg)` | Flat part summary list. |
| `nmail.is_multipart(msg)` / `nmail.content_type(msg)` | Structure helpers. |
| `nmail.payload(part, opts?)` | Decoded part payload. |

## Address & header utilities

| Method | Description |
|--------|-------------|
| `nmail.format_addr(name, email)` or `nmail.format_addr({name?, email})` | Format a mailbox. |
| `nmail.parse_addr(s)` | `{name, email}` from one address. |
| `nmail.parse_addrs(s)` | Array of addresses (quotes-aware). |
| `nmail.make_msgid(domain?)` | Generate a `Message-ID`. |
| `nmail.format_date(unix_secs?)` | RFC 2822 date string (UTC). |
| `nmail.encode_header(text)` | RFC 2047 encode when needed. |
| `nmail.decode_header(text)` | Decode RFC 2047 encoded-words. |

## Errors

| Code | Kind | When |
|------|------|------|
| E2893 | `nmail_error` | Wrong arity. |
| E2894 | `nmail_error` | General failure (I/O, missing fields, encode). |
| E2895 | `nmail_error` | Type error (thrown). |
| E2896 | `nmail_error` | Parse failure (catchable). |

Arity/type problems throw; parse/I/O/build field errors return catchable error values.

## Notes

- **Not** the same as `nmime` (file magic sniffing).
- Does not send mail — use `nsmtp` for SMTP transport.
- v0.1 does not include DKIM/S/MIME, IMAP, or mbox/maildir mailbox stores.
