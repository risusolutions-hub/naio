//! MIME email compose / serialize.

use crate::error::{MailError, MAX_BYTES};
use crate::header::encode_header;
use crate::message::MailMessage;
use crate::qp;
use niao_codec::base64;

/// Emit options.
#[derive(Debug, Clone)]
pub struct EmitOptions {
    pub crlf: bool,
}

impl Default for EmitOptions {
    fn default() -> Self {
        Self { crlf: true }
    }
}

/// Serialize a message to RFC 5322 / MIME bytes.
pub fn emit_bytes(msg: &MailMessage, opts: &EmitOptions) -> Result<Vec<u8>, MailError> {
    let s = emit(msg, opts)?;
    Ok(s.into_bytes())
}

/// Serialize a message to a string (UTF-8; binary parts base64-encoded).
pub fn emit(msg: &MailMessage, opts: &EmitOptions) -> Result<String, MailError> {
    let nl = if opts.crlf { "\r\n" } else { "\n" };
    let mut out = String::new();

    write_header(&mut out, "From", msg.from.as_deref(), nl)?;
    if !msg.to.is_empty() {
        write_header(&mut out, "To", Some(&msg.to.join(", ")), nl)?;
    }
    if !msg.cc.is_empty() {
        write_header(&mut out, "Cc", Some(&msg.cc.join(", ")), nl)?;
    }
    if !msg.bcc.is_empty() {
        write_header(&mut out, "Bcc", Some(&msg.bcc.join(", ")), nl)?;
    }
    write_header(&mut out, "Reply-To", msg.reply_to.as_deref(), nl)?;
    write_header(
        &mut out,
        "Subject",
        msg.subject.as_deref().map(encode_header).as_deref(),
        nl,
    )?;
    write_header(&mut out, "Date", msg.date.as_deref(), nl)?;
    write_header(&mut out, "Message-ID", msg.message_id.as_deref(), nl)?;

    // Extra custom headers (skip ones already written).
    for (k, v) in &msg.headers {
        match k.as_str() {
            "from"
            | "to"
            | "cc"
            | "bcc"
            | "reply-to"
            | "subject"
            | "date"
            | "message-id"
            | "content-type"
            | "mime-version"
            | "content-transfer-encoding" => continue,
            _ => {
                write_header(&mut out, &title_case_header(k), Some(&encode_header(v)), nl)?;
            }
        }
    }

    out.push_str("MIME-Version: 1.0");
    out.push_str(nl);

    let has_text = msg.text.is_some();
    let has_html = msg.html.is_some();
    let has_inline = !msg.inline.is_empty();
    let has_attach = !msg.attachments.is_empty();

    if !has_text && !has_html && !has_attach && !has_inline {
        // Empty body, default text/plain
        out.push_str("Content-Type: text/plain; charset=utf-8");
        out.push_str(nl);
        out.push_str(nl);
    } else if has_attach || has_inline {
        let mixed_boundary = boundary("mixed");
        out.push_str(&format!(
            "Content-Type: multipart/mixed; boundary=\"{mixed_boundary}\""
        ));
        out.push_str(nl);
        out.push_str(nl);

        // First subpart: body (related or alternative or plain)
        out.push_str("--");
        out.push_str(&mixed_boundary);
        out.push_str(nl);
        write_body_part(&mut out, msg, has_text, has_html, has_inline, nl)?;

        for att in &msg.attachments {
            out.push_str("--");
            out.push_str(&mixed_boundary);
            out.push_str(nl);
            write_attachment(&mut out, att, nl);
        }
        out.push_str("--");
        out.push_str(&mixed_boundary);
        out.push_str("--");
        out.push_str(nl);
    } else {
        write_body_part(&mut out, msg, has_text, has_html, false, nl)?;
    }

    if out.len() > MAX_BYTES {
        return Err(MailError::TooLarge(MAX_BYTES));
    }
    Ok(out)
}

fn write_body_part(
    out: &mut String,
    msg: &MailMessage,
    has_text: bool,
    has_html: bool,
    has_inline: bool,
    nl: &str,
) -> Result<(), MailError> {
    if has_inline {
        let related_boundary = boundary("related");
        out.push_str(&format!(
            "Content-Type: multipart/related; boundary=\"{related_boundary}\""
        ));
        out.push_str(nl);
        out.push_str(nl);
        out.push_str("--");
        out.push_str(&related_boundary);
        out.push_str(nl);
        write_alternative_or_single(out, msg, has_text, has_html, nl)?;
        for inl in &msg.inline {
            out.push_str("--");
            out.push_str(&related_boundary);
            out.push_str(nl);
            write_inline(out, inl, nl);
        }
        out.push_str("--");
        out.push_str(&related_boundary);
        out.push_str("--");
        out.push_str(nl);
    } else {
        write_alternative_or_single(out, msg, has_text, has_html, nl)?;
    }
    Ok(())
}

fn write_alternative_or_single(
    out: &mut String,
    msg: &MailMessage,
    has_text: bool,
    has_html: bool,
    nl: &str,
) -> Result<(), MailError> {
    if has_text && has_html {
        let alt = boundary("alt");
        out.push_str(&format!(
            "Content-Type: multipart/alternative; boundary=\"{alt}\""
        ));
        out.push_str(nl);
        out.push_str(nl);
        out.push_str("--");
        out.push_str(&alt);
        out.push_str(nl);
        write_text_part(out, msg.text.as_deref().unwrap_or(""), "text/plain", nl);
        out.push_str("--");
        out.push_str(&alt);
        out.push_str(nl);
        write_text_part(out, msg.html.as_deref().unwrap_or(""), "text/html", nl);
        out.push_str("--");
        out.push_str(&alt);
        out.push_str("--");
        out.push_str(nl);
    } else if has_html {
        write_text_part(out, msg.html.as_deref().unwrap_or(""), "text/html", nl);
    } else {
        write_text_part(out, msg.text.as_deref().unwrap_or(""), "text/plain", nl);
    }
    Ok(())
}

fn write_text_part(out: &mut String, body: &str, ctype: &str, nl: &str) {
    let use_qp = !body.is_ascii() || body.contains('=') || body.len() > 200;
    out.push_str(&format!("Content-Type: {ctype}; charset=utf-8"));
    out.push_str(nl);
    if use_qp {
        out.push_str("Content-Transfer-Encoding: quoted-printable");
        out.push_str(nl);
        out.push_str(nl);
        let enc = qp::encode(body.as_bytes());
        // qp::encode already uses \r\n; normalize if needed
        if nl == "\n" {
            out.push_str(&enc.replace("\r\n", "\n"));
        } else {
            out.push_str(&enc);
        }
    } else {
        out.push_str("Content-Transfer-Encoding: 7bit");
        out.push_str(nl);
        out.push_str(nl);
        for line in body.split('\n') {
            let line = line.trim_end_matches('\r');
            out.push_str(line);
            out.push_str(nl);
        }
    }
    if !out.ends_with(nl) {
        out.push_str(nl);
    }
}

fn write_attachment(out: &mut String, att: &crate::message::Attachment, nl: &str) {
    out.push_str(&format!("Content-Type: {}", att.content_type));
    if let Some(name) = &att.filename {
        out.push_str(&format!("; name=\"{}\"", sanitize_filename(name)));
    }
    out.push_str(nl);
    out.push_str(&format!(
        "Content-Disposition: {}",
        if att.disposition.is_empty() {
            "attachment"
        } else {
            &att.disposition
        }
    ));
    if let Some(name) = &att.filename {
        out.push_str(&format!("; filename=\"{}\"", sanitize_filename(name)));
    }
    out.push_str(nl);
    out.push_str("Content-Transfer-Encoding: base64");
    out.push_str(nl);
    out.push_str(nl);
    write_b64_lines(out, &att.data, nl);
}

fn write_inline(out: &mut String, inl: &crate::message::InlinePart, nl: &str) {
    out.push_str(&format!("Content-Type: {}", inl.content_type));
    if let Some(name) = &inl.filename {
        out.push_str(&format!("; name=\"{}\"", sanitize_filename(name)));
    }
    out.push_str(nl);
    out.push_str("Content-Transfer-Encoding: base64");
    out.push_str(nl);
    out.push_str("Content-Disposition: inline");
    if let Some(name) = &inl.filename {
        out.push_str(&format!("; filename=\"{}\"", sanitize_filename(name)));
    }
    out.push_str(nl);
    let cid = inl.cid.trim_matches(|c| c == '<' || c == '>');
    out.push_str(&format!("Content-ID: <{cid}>"));
    out.push_str(nl);
    out.push_str(nl);
    write_b64_lines(out, &inl.data, nl);
}

fn write_b64_lines(out: &mut String, data: &[u8], nl: &str) {
    let encoded = base64::encode_standard(data);
    for chunk in encoded.as_bytes().chunks(76) {
        out.push_str(std::str::from_utf8(chunk).unwrap_or(""));
        out.push_str(nl);
    }
}

fn write_header(
    out: &mut String,
    name: &str,
    value: Option<&str>,
    nl: &str,
) -> Result<(), MailError> {
    let Some(v) = value else {
        return Ok(());
    };
    if v.contains('\r') || v.contains('\n') {
        return Err(MailError::InvalidHeader(format!(
            "{name} contains bare newlines"
        )));
    }
    out.push_str(name);
    out.push_str(": ");
    out.push_str(v);
    out.push_str(nl);
    Ok(())
}

fn title_case_header(name: &str) -> String {
    name.split('-')
        .map(|p| {
            let mut c = p.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_ascii_uppercase().to_string() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join("-")
}

fn sanitize_filename(name: &str) -> String {
    name.replace('\\', "_")
        .replace('"', "_")
        .replace('\r', "")
        .replace('\n', "")
}

fn boundary(kind: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("nmail_{kind}_{n:x}")
}

/// Write serialized message to a file.
pub fn emit_file(path: &str, msg: &MailMessage, opts: &EmitOptions) -> Result<(), MailError> {
    let data = emit_bytes(msg, opts)?;
    std::fs::write(path, data).map_err(|e| MailError::Io(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{Attachment, InlinePart};

    #[test]
    fn emit_text_html_attach() {
        let mut msg = MailMessage::new();
        msg.from = Some("a@b.com".into());
        msg.to = vec!["c@d.com".into()];
        msg.subject = Some("Hello".into());
        msg.text = Some("plain".into());
        msg.html = Some("<b>html</b>".into());
        msg.attachments.push(Attachment {
            filename: Some("note.txt".into()),
            content_type: "text/plain".into(),
            disposition: "attachment".into(),
            data: b"data".to_vec(),
        });
        msg.inline.push(InlinePart {
            cid: "img1".into(),
            filename: Some("x.png".into()),
            content_type: "image/png".into(),
            data: vec![0x89, 0x50, 0x4E, 0x47],
        });
        let s = emit(&msg, &EmitOptions::default()).unwrap();
        assert!(s.contains("multipart/mixed"));
        assert!(s.contains("multipart/alternative"));
        assert!(s.contains("multipart/related"));
        assert!(s.contains("Content-ID: <img1>"));
        assert!(s.contains("note.txt"));
    }
}
