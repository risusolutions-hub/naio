use crate::error::{FeedError, FeedResult};
use crate::model::{
    Category, ContentPart, Enclosure, FeedDocument, FeedEntry, FeedImage, FeedMeta, Person,
};
use std::collections::HashMap;

/// Build a feed document from a loose field map (Niao object shape).
///
/// >>> use niao_feed::build;
/// >>> let doc = build(&[("title", "My Feed"), ("link", "https://ex.com"), ("entries", "[]")].into_iter().collect()).unwrap();
/// >>> doc.feed.title.as_deref() == Some("My Feed")
/// true
pub fn build(fields: &HashMap<String, String>) -> FeedResult<FeedDocument> {
    let version = fields
        .get("version")
        .cloned()
        .unwrap_or_else(|| "rss20".into());
    let mut doc = FeedDocument::new(version);
    doc.feed.title = fields.get("title").cloned();
    doc.feed.link = fields.get("link").cloned();
    doc.feed.id = fields.get("id").cloned();
    doc.feed.subtitle = fields
        .get("subtitle")
        .or_else(|| fields.get("description"))
        .cloned();
    doc.feed.language = fields.get("language").cloned();
    doc.feed.rights = fields.get("rights").cloned();
    doc.feed.generator = fields.get("generator").cloned();
    doc.feed.icon = fields.get("icon").cloned();
    doc.feed.logo = fields.get("logo").cloned();
    if let Some(ttl) = fields.get("ttl").and_then(|s| s.parse().ok()) {
        doc.feed.ttl = Some(ttl);
    }
    Ok(doc)
}

/// Build a feed entry from string fields.
pub fn build_entry(fields: &HashMap<String, String>) -> FeedResult<FeedEntry> {
    if fields.is_empty() {
        return Err(FeedError::InvalidField("empty entry".into()));
    }
    let mut entry = FeedEntry::default();
    entry.title = fields.get("title").cloned();
    entry.link = fields.get("link").cloned();
    entry.id = fields.get("id").or_else(|| fields.get("guid")).cloned();
    entry.guid = entry.id.clone();
    if let Some(v) = fields.get("guid_is_permalink") {
        entry.guid_is_permalink = Some(matches!(v.as_str(), "true" | "1" | "yes"));
    }
    entry.summary = fields
        .get("summary")
        .or_else(|| fields.get("description"))
        .cloned();
    if let Some(s) = &entry.summary {
        entry.summary_detail = Some(ContentPart {
            value: s.clone(),
            mime_type: fields
                .get("summary_type")
                .cloned()
                .unwrap_or_else(|| "text/html".into()),
            language: fields.get("language").cloned(),
            base: None,
        });
    }
    if let Some(body) = fields.get("content") {
        entry.content.push(ContentPart {
            value: body.clone(),
            mime_type: fields
                .get("content_type")
                .cloned()
                .unwrap_or_else(|| "text/html".into()),
            language: fields.get("language").cloned(),
            base: None,
        });
    }
    entry.published = fields.get("published").cloned();
    entry.updated = fields.get("updated").cloned();
    if let Some(ms) = fields.get("published_ms").and_then(|s| s.parse().ok()) {
        entry.published_ms = Some(ms);
    }
    if let Some(ms) = fields.get("updated_ms").and_then(|s| s.parse().ok()) {
        entry.updated_ms = Some(ms);
    }
    if let Some(a) = fields.get("author").cloned() {
        entry.author = Some(a.clone());
        entry.authors.push(Person {
            name: Some(a),
            email: None,
            uri: None,
        });
    }
    if let Some(url) = fields.get("enclosure_url") {
        entry.enclosures.push(Enclosure {
            url: url.clone(),
            mime_type: fields.get("enclosure_type").cloned(),
            length: fields.get("enclosure_length").and_then(|s| s.parse().ok()),
            title: fields.get("enclosure_title").cloned(),
        });
    }
    Ok(entry)
}

/// Merge feed-level metadata and entries into a document.
pub fn assemble(meta: FeedMeta, entries: Vec<FeedEntry>, version: &str) -> FeedDocument {
    FeedDocument {
        version: version.into(),
        bozo: false,
        bozo_exception: None,
        encoding: None,
        feed: meta,
        entries,
    }
}

pub fn meta_from_strings(
    title: Option<String>,
    link: Option<String>,
    subtitle: Option<String>,
    language: Option<String>,
) -> FeedMeta {
    FeedMeta {
        title,
        link,
        subtitle,
        language,
        ..FeedMeta::default()
    }
}

pub fn image_from_strings(url: String, title: Option<String>, link: Option<String>) -> FeedImage {
    FeedImage {
        url,
        title,
        link,
        width: None,
        height: None,
    }
}

pub fn category(term: impl Into<String>) -> Category {
    Category {
        term: term.into(),
        scheme: None,
        label: None,
    }
}
