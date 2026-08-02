//! MIME email compose + parse for Niao (`nmail`).
//!
//! Native RFC 5322 / MIME parser (via `mail-parser`) and zero-dep-ish compose
//! (quoted-printable + base64 via `niao_codec`). Pairs with `nsmtp` for transport.

mod address;
mod builder;
mod emit;
mod error;
mod header;
mod message;
mod parse;
mod qp;

pub use address::{format_addr, format_date, make_msgid, parse_addr, parse_addrs, MailAddr};
pub use builder::{add_inline, attach, recipients_from_csv, BuildSpec};
pub use emit::{emit, emit_bytes, emit_file, EmitOptions};
pub use error::{MailError, MAX_BYTES};
pub use header::{decode_header, encode_header};
pub use message::{Attachment, InlinePart, MailMessage, PartInfo};
pub use parse::{is_valid, parse, parse_bytes, parse_file, ParseOptions};
pub use qp::{decode as qp_decode, encode as qp_encode};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_pipeline() {
        let msg = BuildSpec {
            from: Some("sender@example.com".into()),
            to: vec!["recv@example.com".into()],
            subject: Some("Demo".into()),
            text: Some("text body".into()),
            html: Some("<p>html body</p>".into()),
            attachments: vec![Attachment {
                filename: Some("a.txt".into()),
                content_type: "text/plain".into(),
                disposition: "attachment".into(),
                data: b"file".to_vec(),
            }],
            inline: vec![InlinePart {
                cid: "logo".into(),
                filename: Some("logo.png".into()),
                content_type: "image/png".into(),
                data: vec![1, 2, 3, 4],
            }],
            auto_date: true,
            auto_message_id: true,
            ..Default::default()
        }
        .build()
        .unwrap();

        let raw = emit(&msg, &EmitOptions::default()).unwrap();
        assert!(is_valid(&raw));
        let back = parse(&raw, &ParseOptions::default()).unwrap();
        assert_eq!(back.subject.as_deref(), Some("Demo"));
        assert!(back.text.as_ref().unwrap().contains("text body"));
        assert!(!back.attachments.is_empty() || !back.parts.is_empty());
    }
}
