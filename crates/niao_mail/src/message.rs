//! Owned MIME email message model.

use std::collections::BTreeMap;

/// A file attachment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub filename: Option<String>,
    pub content_type: String,
    pub disposition: String,
    pub data: Vec<u8>,
}

impl Attachment {
    pub fn size(&self) -> usize {
        self.data.len()
    }
}

/// An inline related part (typically CID image).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlinePart {
    pub cid: String,
    pub filename: Option<String>,
    pub content_type: String,
    pub data: Vec<u8>,
}

/// Summary of a MIME part for `walk` / `parts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartInfo {
    pub index: usize,
    pub content_type: String,
    pub disposition: Option<String>,
    pub filename: Option<String>,
    pub cid: Option<String>,
    pub is_multipart: bool,
    pub size: usize,
    /// Decoded payload when available (text parts include UTF-8; binaries as-is).
    pub data: Vec<u8>,
    pub text: Option<String>,
}

/// Full email message (compose + parse result).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MailMessage {
    /// Raw headers keyed by lowercase name; values are unfolded decoded strings.
    pub headers: BTreeMap<String, String>,
    pub from: Option<String>,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub reply_to: Option<String>,
    pub subject: Option<String>,
    pub date: Option<String>,
    pub message_id: Option<String>,
    pub content_type: String,
    pub text: Option<String>,
    pub html: Option<String>,
    pub attachments: Vec<Attachment>,
    pub inline: Vec<InlinePart>,
    pub parts: Vec<PartInfo>,
    pub multipart: bool,
}

impl MailMessage {
    pub fn new() -> Self {
        Self {
            content_type: "text/plain; charset=utf-8".into(),
            ..Default::default()
        }
    }

    pub fn get_header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
            .or_else(|| match name.to_ascii_lowercase().as_str() {
                "from" => self.from.as_deref(),
                "subject" => self.subject.as_deref(),
                "date" => self.date.as_deref(),
                "message-id" => self.message_id.as_deref(),
                "reply-to" => self.reply_to.as_deref(),
                "to" if !self.to.is_empty() => Some(self.to[0].as_str()),
                _ => None,
            })
    }

    pub fn set_header(&mut self, name: &str, value: impl Into<String>) {
        let key = name.to_ascii_lowercase();
        let value = value.into();
        match key.as_str() {
            "from" => self.from = Some(value.clone()),
            "subject" => self.subject = Some(value.clone()),
            "date" => self.date = Some(value.clone()),
            "message-id" => self.message_id = Some(value.clone()),
            "reply-to" => self.reply_to = Some(value.clone()),
            "to" => self.to = vec![value.clone()],
            "cc" => self.cc = vec![value.clone()],
            "bcc" => self.bcc = vec![value.clone()],
            "content-type" => self.content_type = value.clone(),
            _ => {}
        }
        self.headers.insert(key, value);
    }
}
