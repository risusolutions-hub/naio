use crate::dates::optional_datetime;
use crate::detect::version_from_feed_rs;
use crate::error::{check_len, FeedError, FeedResult};
use crate::model::{
    Category, ContentPart, Enclosure, FeedDocument, FeedEntry, FeedImage, FeedLink, FeedMeta,
    Person,
};
use feed_rs::model::{Content, Link, Person as FrPerson, Text};
use niao_encoding::{decode, detect, DecodeErrorMode};
use std::io::Cursor;

/// Parse options (feedparser-style).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseOptions {
    /// Sanitize HTML in summary/content bodies.
    pub sanitize: bool,
    /// When true, return partial results with `bozo=true` on recoverable errors.
    pub relaxed: bool,
    /// Override charset label (otherwise detect from BOM / XML / chardet).
    pub encoding: Option<&'static str>,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            sanitize: false,
            relaxed: false,
            encoding: None,
        }
    }
}

/// Parse feed text (UTF-8 or detected encoding).
///
/// >>> use niao_feed::{parse, ParseOptions};
/// >>> let xml = r#"<?xml version="1.0"?><rss version="2.0"><channel><title>T</title><item><title>E</title></item></channel></rss>"#;
/// >>> let doc = parse(xml, &ParseOptions::default()).unwrap();
/// >>> doc.feed.title.as_deref() == Some("T")
/// true
pub fn parse(input: &str, opts: &ParseOptions) -> FeedResult<FeedDocument> {
    check_len(input.len())?;
    parse_bytes(input.as_bytes(), opts)
}

/// Parse raw bytes with charset detection / BOM handling.
///
/// >>> use niao_feed::{parse_bytes, ParseOptions};
/// >>> let xml = b"<rss version=\"2.0\"><channel><title>X</title></channel></rss>";
/// >>> parse_bytes(xml, &ParseOptions::default()).unwrap().version.starts_with("rss")
/// true
pub fn parse_bytes(bytes: &[u8], opts: &ParseOptions) -> FeedResult<FeedDocument> {
    check_len(bytes.len())?;
    let (text, encoding) = decode_input(bytes, opts.encoding)?;
    parse_utf8(&text, encoding, opts)
}

fn decode_input(bytes: &[u8], encoding: Option<&str>) -> FeedResult<(String, Option<String>)> {
    if let Some(enc) = encoding {
        let text = decode(bytes, Some(enc), DecodeErrorMode::Replace)
            .map_err(|e| FeedError::Parse(e.message()))?;
        return Ok((text, Some(enc.to_string())));
    }
    let det = detect(bytes);
    let enc = if det.confidence >= 0.5 {
        det.encoding.clone()
    } else {
        "utf-8".into()
    };
    let text = decode(bytes, Some(&enc), DecodeErrorMode::Replace)
        .map_err(|e| FeedError::Parse(e.message()))?;
    Ok((text, Some(enc)))
}

fn parse_utf8(
    text: &str,
    encoding: Option<String>,
    opts: &ParseOptions,
) -> FeedResult<FeedDocument> {
    let mut builder = feed_rs::parser::Builder::new();
    if opts.sanitize {
        builder = builder.sanitize_content(true);
    }
    let parser = builder.build();
    let cursor = Cursor::new(text.as_bytes());
    match parser.parse(cursor) {
        Ok(feed) => {
            let mut doc = convert_feed(&feed);
            doc.encoding = encoding;
            if doc.version.is_empty() {
                doc.version = version_from_feed_rs(&feed);
            }
            Ok(doc)
        }
        Err(e) if opts.relaxed => match feed_rs::parser::parse(text.as_bytes()) {
            Ok(feed) => {
                let mut doc = convert_feed(&feed);
                doc.encoding = encoding;
                doc.bozo = true;
                doc.bozo_exception = Some(e.to_string());
                Ok(doc)
            }
            Err(e2) => Err(FeedError::Parse(format!("{e}; fallback: {e2}"))),
        },
        Err(e) => Err(FeedError::Parse(e.to_string())),
    }
}

/// True when bytes look like a syndication feed.
///
/// >>> use niao_feed::is_valid;
/// >>> is_valid("<rss version=\"2.0\"><channel><title>A</title></channel></rss>")
/// true
pub fn is_valid(input: &str) -> bool {
    parse(input, &ParseOptions::default()).is_ok()
}

pub fn convert_feed(feed: &feed_rs::model::Feed) -> FeedDocument {
    let version = version_from_feed_rs(feed);
    let mut doc = FeedDocument::new(version);
    doc.feed = convert_meta(feed);
    doc.entries = feed.entries.iter().map(convert_entry).collect();
    doc
}

fn convert_meta(feed: &feed_rs::model::Feed) -> FeedMeta {
    let (updated, updated_ms) = optional_datetime(feed.updated);
    let (published, published_ms) = optional_datetime(feed.published);
    let link = primary_link(&feed.links);
    FeedMeta {
        title: text_content(&feed.title),
        link,
        id: if feed.id.is_empty() {
            None
        } else {
            Some(feed.id.clone())
        },
        subtitle: text_content(&feed.description),
        rights: text_content(&feed.rights),
        language: feed.language.clone(),
        updated,
        updated_ms,
        published,
        published_ms,
        generator: feed.generator.as_ref().map(|g| g.content.clone()),
        icon: feed.icon.as_ref().map(|i| i.uri.clone()),
        logo: feed.logo.as_ref().map(|i| i.uri.clone()),
        ttl: feed.ttl.map(|t| t as i64),
        authors: feed.authors.iter().map(convert_person).collect(),
        links: feed.links.iter().map(convert_link).collect(),
        categories: feed.categories.iter().map(convert_category).collect(),
        image: feed.logo.as_ref().map(|img| FeedImage {
            url: img.uri.clone(),
            title: img.title.clone(),
            link: img.link.as_ref().map(|l| l.href.clone()),
            width: img.width.map(|w| w as i64),
            height: img.height.map(|h| h as i64),
        }),
    }
}

fn convert_entry(entry: &feed_rs::model::Entry) -> FeedEntry {
    let (published, published_ms) = optional_datetime(entry.published);
    let (updated, updated_ms) = optional_datetime(entry.updated);
    let link = primary_link(&entry.links);
    let author = entry.authors.first().and_then(person_name);
    let summary_detail = entry.summary.as_ref().map(convert_text_part);
    let summary = summary_detail.as_ref().map(|c| c.value.clone());
    let mut enclosures: Vec<Enclosure> = entry
        .links
        .iter()
        .filter(|l| l.rel.as_deref() == Some("enclosure"))
        .map(|l| Enclosure {
            url: l.href.clone(),
            mime_type: l.media_type.clone(),
            length: l.length,
            title: l.title.clone(),
        })
        .collect();
    for mobj in &entry.media {
        for mc in &mobj.content {
            if let Some(url) = &mc.url {
                enclosures.push(Enclosure {
                    url: url.to_string(),
                    mime_type: mc.content_type.as_ref().map(|m| m.to_string()),
                    length: mc.size,
                    title: mobj.title.as_ref().map(|t| t.content.clone()),
                });
            }
        }
    }
    FeedEntry {
        id: if entry.id.is_empty() {
            None
        } else {
            Some(entry.id.clone())
        },
        title: text_content(&entry.title),
        link,
        summary,
        summary_detail,
        content: entry
            .content
            .as_ref()
            .map(|c| vec![convert_content_part(c)])
            .unwrap_or_default(),
        published,
        published_ms,
        updated,
        updated_ms,
        author,
        authors: entry.authors.iter().map(convert_person).collect(),
        links: entry.links.iter().map(convert_link).collect(),
        tags: entry.categories.iter().map(convert_category).collect(),
        enclosures,
        guid: if entry.id.is_empty() {
            None
        } else {
            Some(entry.id.clone())
        },
        guid_is_permalink: None,
    }
}

fn convert_person(p: &FrPerson) -> Person {
    Person {
        name: Some(p.name.clone()),
        email: p.email.clone(),
        uri: p.uri.clone(),
    }
}

fn person_name(p: &FrPerson) -> Option<String> {
    if p.name.is_empty() {
        p.email.clone().or_else(|| p.uri.clone())
    } else {
        Some(p.name.clone())
    }
}

fn convert_link(l: &Link) -> FeedLink {
    FeedLink {
        href: l.href.clone(),
        rel: l.rel.clone(),
        title: l.title.clone(),
        mime_type: l.media_type.clone(),
        length: l.length,
    }
}

fn convert_category(c: &feed_rs::model::Category) -> Category {
    Category {
        term: c.term.clone(),
        scheme: c.scheme.clone(),
        label: c.label.clone(),
    }
}

fn convert_content_part(c: &Content) -> ContentPart {
    ContentPart {
        value: c.body.clone().unwrap_or_default(),
        mime_type: c.content_type.to_string(),
        language: None,
        base: c.src.as_ref().map(|l| l.href.clone()),
    }
}

fn convert_text_part(t: &Text) -> ContentPart {
    ContentPart {
        value: t.content.clone(),
        mime_type: t.content_type.to_string(),
        language: None,
        base: t.src.clone(),
    }
}

fn text_content(t: &Option<Text>) -> Option<String> {
    t.as_ref().map(|t| t.content.clone())
}

fn primary_link(links: &[Link]) -> Option<String> {
    links
        .iter()
        .find(|l| l.rel.as_deref() == Some("alternate") || l.rel.is_none())
        .or_else(|| links.first())
        .map(|l| l.href.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    const RSS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>Example</title>
    <link>https://example.com/</link>
    <description>Demo feed</description>
    <item>
      <title>Post</title>
      <link>https://example.com/1</link>
      <guid>1</guid>
      <pubDate>Mon, 06 Sep 2010 00:01:00 +0000</pubDate>
      <description><![CDATA[<p>Hello</p>]]></description>
    </item>
  </channel>
</rss>"#;

    const ATOM: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Atom</title>
  <link href="https://example.com/"/>
  <id>urn:uuid:1</id>
  <updated>2010-09-06T00:01:00Z</updated>
  <entry>
    <title>Entry</title>
    <link href="https://example.com/e1"/>
    <id>urn:uuid:e1</id>
    <updated>2010-09-06T00:01:00Z</updated>
    <summary>text</summary>
  </entry>
</feed>"#;

    #[test]
    fn rss_parse() {
        let doc = parse(RSS, &ParseOptions::default()).unwrap();
        assert_eq!(doc.feed.title.as_deref(), Some("Example"));
        assert_eq!(doc.entries.len(), 1);
        assert_eq!(doc.entries[0].title.as_deref(), Some("Post"));
    }

    #[test]
    fn atom_parse() {
        let doc = parse(ATOM, &ParseOptions::default()).unwrap();
        assert_eq!(doc.version, "atom10");
        assert_eq!(doc.entries.len(), 1);
    }
}
