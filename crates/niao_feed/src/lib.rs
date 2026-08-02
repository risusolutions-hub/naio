//! RSS / Atom / JSON Feed parse + generate for Niao (`nfeed`).
//!
//! Native syndication feed parser with feedparser-shaped results, charset
//! detection, HTML sanitization, and RSS 2.0 / Atom 1.0 / JSON Feed emit.

mod batch;
mod builder;
mod dates;
mod detect;
mod emit;
mod error;
mod html;
mod model;
mod parse;

pub use batch::parallel_parse;
pub use builder::{assemble, build, build_entry, category, image_from_strings, meta_from_strings};
pub use dates::{format_date, parse_date, ParsedDate};
pub use detect::{detect_format, detect_version};
pub use emit::emit;
pub use emit::{EmitFormat, EmitOptions};
pub use error::{check_len, FeedError, FeedResult, MAX_BYTES};
pub use html::{sanitize_html, strip_html};
pub use model::{
    Category, ContentPart, Enclosure, FeedDocument, FeedEntry, FeedImage, FeedLink, FeedMeta,
    Person,
};
pub use parse::{convert_feed, is_valid, parse, parse_bytes, ParseOptions};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_rss() {
        let xml = r#"<?xml version="1.0"?><rss version="2.0"><channel><title>T</title><link>https://ex.com</link><item><title>E</title><link>https://ex.com/1</link></item></channel></rss>"#;
        let doc = parse(xml, &ParseOptions::default()).unwrap();
        let out = emit(
            &doc,
            &EmitOptions {
                format: EmitFormat::Rss2,
                ..Default::default()
            },
        )
        .unwrap();
        let doc2 = parse(&out, &ParseOptions::default()).unwrap();
        assert_eq!(doc2.feed.title, doc.feed.title);
        assert_eq!(doc2.entries.len(), 1);
    }
}
