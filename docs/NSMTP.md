# nsmtp — SMTP email sending

Ergonomic SMTP wrapper around `lettre`. Uses a single config object instead of positional `net_smtp_send` arguments.

## Import

```niao
import "nsmtp"
```

Paths `import "std/nsmtp"` and `import "nsmtp"` are equivalent. Flat builtins (`nsmtp_send`, `nsmtp_send_html`) are also available globally after import.

## Quick start

```niao
import "nsmtp"

nsmtp.send({
    host: "smtp.example.com",
    port: 587,
    from: "alice@example.com",
    to: "bob@example.com",
    subject: "Hello",
    body: "Plain text body",
    user: "alice",
    pass: "secret"
})

nsmtp.send_html({
    host: "smtp.example.com",
    from: "alice@example.com",
    to: ["bob@example.com", "carol@example.com"],
    subject: "Welcome",
    body: "Plain fallback",
    html: "<p><strong>Welcome</strong></p>"
})
```

## Functions

| Method | Description |
|--------|-------------|
| `nsmtp.send(config)` | Send a plain-text email. Returns `true` on success. |
| `nsmtp.send_html(config)` | Send multipart `text/plain` + `text/html` (alternative). Returns `true` on success. |

## Config object

| Field | Type | Required | Default | Notes |
|-------|------|----------|---------|-------|
| `host` | string | yes | — | SMTP relay hostname. |
| `port` | int | no | `587` | Submission port (0..=65535). |
| `from` | string | yes | — | Sender address. |
| `to` | string \| array | yes | — | One recipient or list of addresses. |
| `subject` | string | yes | — | Email subject. |
| `body` | string | yes | — | Plain-text body (`send_html` uses this as the text alternative). |
| `html` | string | `send_html` only | — | HTML body for multipart messages. |
| `user` | string | no | — | SMTP username (used with `pass`). |
| `pass` | string | no | — | SMTP password. |
| `tls` | bool | no | `true` | When `false`, connects without TLS (`Tls::None`). |

On SMTP or address errors, functions return a catchable `nsmtp_error` value (not a thrown exception). Missing or mistyped config fields throw before send is attempted.

## Errors

| Code | Meaning |
|------|---------|
| 2890 | Wrong argument count. |
| 2891 | SMTP / message error (catchable `nsmtp_error`). |
| 2892 | Wrong config type or missing required field. |

## See also

- `net` — `net_smtp_send(host, port, from, to, subject, body, opts?)` positional API.
- `nvalid` — `is_email()` for address validation.
