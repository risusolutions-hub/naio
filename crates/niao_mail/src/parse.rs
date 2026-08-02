//! Parse RFC 5322 / MIME messages via `mail-parser`.

use crate::error::{MailError, MAX_BYTES};
use crate::header::decode_header;
use crate::message::{Attachment, InlinePart, MailMessage, PartInfo};
use mail_parser::{Address, MessageParser, MimeHeaders, PartType};

/// Parse options (reserved for future relaxed/strict modes).
#[derive(Debug, Clone, Default)]
pub struct ParseOptions {
    pub relaxed: bool,
}

/// Parse a message from UTF-8 text (lossy if invalid sequences present for relaxed).
pub fn parse(input: &str, opts: &ParseOptions) -> Result<MailMessage, MailError> {
    parse_bytes(input.as_bytes(), opts)
}

/// Parse raw message bytes.
pub fn parse_bytes(input: &[u8], _opts: &ParseOptions) -> Result<MailMessage, MailError> {
    if input.is_empty() {
        return Err(MailError::Empty);
    }
    if input.len() > MAX_BYTES {
        return Err(MailError::TooLarge(MAX_BYTES));
    }
    let parsed = MessageParser::default()
        .parse(input)
        .ok_or_else(|| MailError::Parse("unable to parse message".into()))?;

    let mut msg = MailMessage::new();

    // Headers from root part.
    if let Some(root) = parsed.parts.first() {
        for h in &root.headers {
            let name = h.name.as_str().to_ascii_lowercase();
            let value = header_value_string(&h.value);
            let value = decode_header(&value).unwrap_or(value);
            msg.headers.insert(name, value);
        }
    }

    msg.from = parsed.from().map(address_to_string);
    msg.to = address_list(parsed.to());
    msg.cc = address_list(parsed.cc());
    msg.bcc = address_list(parsed.bcc());
    msg.reply_to = parsed.reply_to().map(address_to_string);
    msg.subject = parsed
        .subject()
        .map(|s| decode_header(s).unwrap_or_else(|_| s.to_string()));
    msg.message_id = parsed.message_id().map(|s| s.to_string());
    if let Some(dt) = parsed.date() {
        msg.date = Some(dt.to_rfc822());
    } else if let Some(d) = msg.headers.get("date") {
        msg.date = Some(d.clone());
    }

    if let Some(ct) = parsed.content_type() {
        msg.content_type = content_type_string(ct);
    }
    msg.multipart = parsed
        .content_type()
        .is_some_and(|ct| ct.c_type.eq_ignore_ascii_case("multipart"))
        || parsed.parts.len() > 1;

    msg.text = parsed.body_text(0).map(|c| trim_body_text(c.as_ref()));
    msg.html = parsed.body_html(0).map(|c| trim_body_text(c.as_ref()));

    for (idx, part) in parsed.parts.iter().enumerate() {
        let ct = part
            .content_type()
            .map(content_type_string)
            .unwrap_or_else(|| "application/octet-stream".into());
        let disposition = part.content_disposition().map(|cd| cd.c_type.to_string());
        let filename = part.attachment_name().map(|s| s.to_string());
        let cid = part.content_id().map(|s| s.to_string());
        let is_mp = matches!(part.body, PartType::Multipart(_));
        let data = part.contents().to_vec();
        let text = part.text_contents().map(|s| s.to_string());
        let size = data.len();

        let is_attachment = disposition
            .as_deref()
            .is_some_and(|d| d.eq_ignore_ascii_case("attachment"))
            || (filename.is_some()
                && !matches!(part.body, PartType::Text(_) | PartType::Html(_))
                && !cid.is_some());

        let is_inline = cid.is_some()
            && (disposition
                .as_deref()
                .is_some_and(|d| d.eq_ignore_ascii_case("inline"))
                || matches!(part.body, PartType::InlineBinary(_)));

        if is_attachment && idx > 0 {
            msg.attachments.push(Attachment {
                filename: filename.clone(),
                content_type: ct.clone(),
                disposition: disposition.clone().unwrap_or_else(|| "attachment".into()),
                data: data.clone(),
            });
        } else if is_inline && idx > 0 {
            if let Some(cid_v) = cid.clone() {
                msg.inline.push(InlinePart {
                    cid: cid_v,
                    filename: filename.clone(),
                    content_type: ct.clone(),
                    data: data.clone(),
                });
            }
        }

        msg.parts.push(PartInfo {
            index: idx,
            content_type: ct,
            disposition,
            filename,
            cid,
            is_multipart: is_mp,
            size,
            data,
            text,
        });
    }

    // Prefer mail-parser attachment iterator when our heuristic missed.
    if msg.attachments.is_empty() {
        for part in parsed.attachments() {
            if matches!(part.body, PartType::Multipart(_)) {
                continue;
            }
            let filename = part.attachment_name().map(|s| s.to_string());
            let ct = part
                .content_type()
                .map(content_type_string)
                .unwrap_or_else(|| "application/octet-stream".into());
            let disposition = part
                .content_disposition()
                .map(|cd| cd.c_type.to_string())
                .unwrap_or_else(|| "attachment".into());
            // Skip pure text/html body parts listed as attachments only if they lack filename.
            if filename.is_none()
                && (part.is_text() || part.is_text_html())
                && !disposition.eq_ignore_ascii_case("attachment")
            {
                continue;
            }
            let data = part.contents().to_vec();
            if let Some(cid) = part.content_id() {
                if disposition.eq_ignore_ascii_case("inline")
                    || matches!(part.body, PartType::InlineBinary(_))
                {
                    msg.inline.push(InlinePart {
                        cid: cid.to_string(),
                        filename,
                        content_type: ct,
                        data,
                    });
                    continue;
                }
            }
            msg.attachments.push(Attachment {
                filename,
                content_type: ct,
                disposition,
                data,
            });
        }
    }

    Ok(msg)
}

/// Return true when the input parses as a message.
pub fn is_valid(input: &str) -> bool {
    if input.is_empty() || input.len() > MAX_BYTES {
        return false;
    }
    MessageParser::default().parse(input.as_bytes()).is_some()
}

fn trim_body_text(s: &str) -> String {
    s.trim_end_matches(['\r', '\n']).to_string()
}

fn address_to_string(addr: &Address<'_>) -> String {
    addr.iter()
        .map(|a| {
            let email = a.address.as_deref().unwrap_or("");
            match a.name.as_deref() {
                Some(n) if !n.is_empty() => {
                    if n.bytes()
                        .any(|b| !b.is_ascii_alphanumeric() && b != b' ' && b != b'-' && b != b'.')
                    {
                        format!(
                            "\"{}\" <{email}>",
                            n.replace('\\', "\\\\").replace('"', "\\\"")
                        )
                    } else {
                        format!("{n} <{email}>")
                    }
                }
                _ => email.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn address_list(addr: Option<&Address<'_>>) -> Vec<String> {
    match addr {
        Some(a) => a
            .iter()
            .filter_map(|x| {
                x.address.as_ref().map(|e| match x.name.as_deref() {
                    Some(n) if !n.is_empty() => format!("{n} <{e}>"),
                    _ => e.to_string(),
                })
            })
            .collect(),
        None => Vec::new(),
    }
}

fn content_type_string(ct: &mail_parser::ContentType<'_>) -> String {
    match &ct.c_subtype {
        Some(sub) => {
            let mut s = format!("{}/{}", ct.c_type, sub);
            if let Some(attrs) = &ct.attributes {
                for attr in attrs {
                    s.push_str(&format!("; {}=\"{}\"", attr.name, attr.value));
                }
            }
            s
        }
        None => ct.c_type.to_string(),
    }
}

fn header_value_string(v: &mail_parser::HeaderValue<'_>) -> String {
    use mail_parser::HeaderValue;
    match v {
        HeaderValue::Text(t) => t.to_string(),
        HeaderValue::TextList(list) => list
            .iter()
            .map(|t| t.as_ref())
            .collect::<Vec<_>>()
            .join(", "),
        HeaderValue::Address(a) => address_to_string(a),
        HeaderValue::DateTime(dt) => dt.to_rfc822(),
        HeaderValue::ContentType(ct) => content_type_string(ct),
        HeaderValue::Received(r) => format!("{r:?}"),
        HeaderValue::Empty => String::new(),
    }
}

/// Read a file and parse.
pub fn parse_file(path: &str, opts: &ParseOptions) -> Result<MailMessage, MailError> {
    let data = std::fs::read(path).map_err(|e| MailError::Io(e.to_string()))?;
    parse_bytes(&data, opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple() {
        let raw = "From: a@b.com\r\nTo: c@d.com\r\nSubject: Hi\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nHello";
        let msg = parse(raw, &ParseOptions::default()).unwrap();
        assert_eq!(msg.subject.as_deref(), Some("Hi"));
        assert_eq!(msg.text.as_deref(), Some("Hello"));
        assert!(msg.from.as_ref().unwrap().contains("a@b.com"));
    }

    #[test]
    fn empty_fails() {
        assert!(matches!(
            parse("", &ParseOptions::default()),
            Err(MailError::Empty)
        ));
    }
}
