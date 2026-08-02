//! High-level message builder from structured options.

use crate::address::{format_addr, format_date, make_msgid, parse_addrs};
use crate::error::MailError;
use crate::message::{Attachment, InlinePart, MailMessage};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Build a [`MailMessage`] from field map helpers used by the VM bridge.
#[derive(Debug, Clone, Default)]
pub struct BuildSpec {
    pub from: Option<String>,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub reply_to: Option<String>,
    pub subject: Option<String>,
    pub text: Option<String>,
    pub html: Option<String>,
    pub date: Option<String>,
    pub message_id: Option<String>,
    pub headers: BTreeMap<String, String>,
    pub attachments: Vec<Attachment>,
    pub inline: Vec<InlinePart>,
    pub auto_date: bool,
    pub auto_message_id: bool,
    pub msgid_domain: Option<String>,
}

impl BuildSpec {
    pub fn build(self) -> Result<MailMessage, MailError> {
        if self.from.is_none() {
            return Err(MailError::MissingField("from".into()));
        }
        if self.to.is_empty() {
            return Err(MailError::MissingField("to".into()));
        }
        let mut msg = MailMessage::new();
        msg.from = self.from;
        msg.to = self.to;
        msg.cc = self.cc;
        msg.bcc = self.bcc;
        msg.reply_to = self.reply_to;
        msg.subject = self.subject;
        msg.text = self.text;
        msg.html = self.html;
        msg.attachments = self.attachments;
        msg.inline = self.inline;
        msg.date = self.date.or_else(|| {
            if self.auto_date {
                let secs = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                Some(format_date(Some(secs)))
            } else {
                None
            }
        });
        msg.message_id = self.message_id.or_else(|| {
            if self.auto_message_id {
                Some(make_msgid(self.msgid_domain.as_deref()))
            } else {
                None
            }
        });
        for (k, v) in self.headers {
            msg.set_header(&k, v);
        }
        // Sync common headers map.
        if let Some(f) = &msg.from {
            msg.headers.insert("from".into(), f.clone());
        }
        if !msg.to.is_empty() {
            msg.headers.insert("to".into(), msg.to.join(", "));
        }
        if let Some(s) = &msg.subject {
            msg.headers.insert("subject".into(), s.clone());
        }
        if let Some(d) = &msg.date {
            msg.headers.insert("date".into(), d.clone());
        }
        if let Some(m) = &msg.message_id {
            msg.headers.insert("message-id".into(), m.clone());
        }
        msg.multipart = msg.html.is_some()
            || !msg.attachments.is_empty()
            || !msg.inline.is_empty()
            || (msg.text.is_some() && msg.html.is_some());
        if msg.multipart {
            msg.content_type = if !msg.attachments.is_empty() {
                "multipart/mixed".into()
            } else if !msg.inline.is_empty() {
                "multipart/related".into()
            } else if msg.text.is_some() && msg.html.is_some() {
                "multipart/alternative".into()
            } else if msg.html.is_some() {
                "text/html; charset=utf-8".into()
            } else {
                "text/plain; charset=utf-8".into()
            };
        }
        Ok(msg)
    }
}

/// Parse recipient field that may be string or already-split list.
pub fn recipients_from_csv(raw: &str) -> Result<Vec<String>, MailError> {
    let addrs = parse_addrs(raw)?;
    let mut out = Vec::with_capacity(addrs.len());
    for a in addrs {
        out.push(format_addr(a.name.as_deref(), &a.email)?);
    }
    Ok(out)
}

/// Attach binary payload to a message (consumes and returns updated).
pub fn attach(
    mut msg: MailMessage,
    filename: Option<String>,
    content_type: String,
    disposition: String,
    data: Vec<u8>,
) -> MailMessage {
    msg.attachments.push(Attachment {
        filename,
        content_type,
        disposition: if disposition.is_empty() {
            "attachment".into()
        } else {
            disposition
        },
        data,
    });
    msg.multipart = true;
    msg
}

/// Add an inline CID part.
pub fn add_inline(
    mut msg: MailMessage,
    cid: String,
    filename: Option<String>,
    content_type: String,
    data: Vec<u8>,
) -> MailMessage {
    msg.inline.push(InlinePart {
        cid,
        filename,
        content_type,
        data,
    });
    msg.multipart = true;
    msg
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit::{emit, EmitOptions};
    use crate::parse::{parse, ParseOptions};

    #[test]
    fn build_roundtrip() {
        let msg = BuildSpec {
            from: Some("Ada <ada@example.com>".into()),
            to: vec!["bob@example.com".into()],
            subject: Some("Hi café".into()),
            text: Some("hello".into()),
            html: Some("<p>hello</p>".into()),
            auto_date: true,
            auto_message_id: true,
            ..Default::default()
        }
        .build()
        .unwrap();
        let raw = emit(&msg, &EmitOptions::default()).unwrap();
        let back = parse(&raw, &ParseOptions::default()).unwrap();
        assert!(
            back.subject.as_ref().unwrap().contains("café")
                || back.subject.as_ref().unwrap().contains("Hi")
        );
        assert_eq!(back.text.as_deref(), Some("hello"));
        assert!(back.html.as_ref().unwrap().contains("hello"));
    }
}
