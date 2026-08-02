use crate::error::{FeedError, FeedResult};
use crate::model::{ContentPart, FeedDocument, FeedEntry, FeedMeta};
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Writer;
use serde_json::{json, Value};
use std::io::Cursor;

/// Emit options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmitOptions {
    /// Output format: `rss2`, `atom`, or `json`.
    pub format: EmitFormat,
    /// Pretty-print JSON output.
    pub pretty: bool,
    /// XML indent spaces (0 = compact).
    pub indent: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitFormat {
    Rss2,
    Atom,
    Json,
}

impl EmitFormat {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "rss" | "rss2" | "rss20" | "rss2.0" => Some(Self::Rss2),
            "atom" | "atom10" | "atom1.0" => Some(Self::Atom),
            "json" | "jsonfeed" | "json10" | "json11" => Some(Self::Json),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rss2 => "rss2",
            Self::Atom => "atom",
            Self::Json => "json",
        }
    }
}

impl Default for EmitOptions {
    fn default() -> Self {
        Self {
            format: EmitFormat::Rss2,
            pretty: false,
            indent: 0,
        }
    }
}

/// Serialize a feed document to XML or JSON.
///
/// >>> use niao_feed::{emit, EmitOptions, EmitFormat, model::FeedDocument};
/// >>> let mut doc = FeedDocument::new("rss20");
/// >>> doc.feed.title = Some("T".into());
/// >>> emit(&doc, &EmitOptions { format: EmitFormat::Rss2, ..Default::default() }).unwrap().contains("<rss")
/// true
pub fn emit(doc: &FeedDocument, opts: &EmitOptions) -> FeedResult<String> {
    match opts.format {
        EmitFormat::Rss2 => emit_rss2(doc, opts.indent),
        EmitFormat::Atom => emit_atom(doc, opts.indent),
        EmitFormat::Json => emit_json(doc, opts.pretty),
    }
}

fn emit_json(doc: &FeedDocument, pretty: bool) -> FeedResult<String> {
    let version = if doc.version.starts_with("json11") {
        "https://jsonfeed.org/version/1.1"
    } else {
        "https://jsonfeed.org/version/1"
    };
    let feed = &doc.feed;
    let mut root = json!({
        "version": version,
        "title": feed.title.clone().unwrap_or_default(),
        "home_page_url": feed.link.clone(),
        "feed_url": feed.link.clone(),
        "description": feed.subtitle.clone(),
        "language": feed.language.clone(),
        "icon": feed.icon.clone(),
        "favicon": feed.logo.clone(),
        "items": doc.entries.iter().map(entry_to_json).collect::<Vec<_>>(),
    });
    if let Some(id) = &feed.id {
        root["id"] = json!(id);
    }
    if let Some(obj) = root.as_object_mut() {
        obj.retain(|_, v| !v.is_null());
    }
    if pretty {
        serde_json::to_string_pretty(&root).map_err(|e| FeedError::Emit(e.to_string()))
    } else {
        serde_json::to_string(&root).map_err(|e| FeedError::Emit(e.to_string()))
    }
}

fn entry_to_json(e: &FeedEntry) -> Value {
    let mut item = json!({
        "id": e.id.clone().or(e.guid.clone()),
        "title": e.title.clone(),
        "url": e.link.clone(),
        "summary": e.summary.clone(),
        "date_published": e.published.clone(),
        "date_modified": e.updated.clone(),
    });
    if let Some(obj) = item.as_object_mut() {
        if !e.content.is_empty() {
            obj.insert("content_html".into(), json!(e.content[0].value.clone()));
        }
        if !e.tags.is_empty() {
            obj.insert(
                "tags".into(),
                json!(e.tags.iter().map(|t| &t.term).collect::<Vec<_>>()),
            );
        }
        obj.retain(|_, v| !v.is_null());
    }
    item
}

fn emit_rss2(doc: &FeedDocument, indent: usize) -> FeedResult<String> {
    let mut w = Writer::new(Cursor::new(Vec::new()));
    w.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
        .map_err(|e| FeedError::Emit(e.to_string()))?;
    let mut rss = BytesStart::new("rss");
    rss.push_attribute(("version", "2.0"));
    w.write_event(Event::Start(rss)).map_err(map_io)?;
    write_indent(&mut w, indent, 0)?;
    write_channel_rss(&mut w, &doc.feed, &doc.entries, indent, 1)?;
    w.write_event(Event::End(BytesEnd::new("rss")))
        .map_err(map_io)?;
    let buf = w.into_inner().into_inner();
    String::from_utf8(buf).map_err(|e| FeedError::Emit(e.to_string()))
}

fn write_channel_rss(
    w: &mut Writer<Cursor<Vec<u8>>>,
    feed: &FeedMeta,
    entries: &[FeedEntry],
    indent: usize,
    depth: usize,
) -> FeedResult<()> {
    w.write_event(Event::Start(BytesStart::new("channel")))
        .map_err(map_io)?;
    write_text_el(w, "title", feed.title.as_deref(), indent, depth)?;
    write_text_el(w, "link", feed.link.as_deref(), indent, depth)?;
    write_text_el(w, "description", feed.subtitle.as_deref(), indent, depth)?;
    write_text_el(w, "language", feed.language.as_deref(), indent, depth)?;
    write_text_el(w, "copyright", feed.rights.as_deref(), indent, depth)?;
    if let Some(ms) = feed.updated_ms {
        write_text_el(w, "lastBuildDate", Some(&chrono_rfc822(ms)), indent, depth)?;
    }
    if let Some(ttl) = feed.ttl {
        write_text_el(w, "ttl", Some(&ttl.to_string()), indent, depth)?;
    }
    if let Some(img) = &feed.image {
        write_rss_image(w, img, indent, depth)?;
    }
    for entry in entries {
        write_rss_item(w, entry, indent, depth)?;
    }
    write_indent(w, indent, depth - 1)?;
    w.write_event(Event::End(BytesEnd::new("channel")))
        .map_err(map_io)?;
    Ok(())
}

fn write_rss_item(
    w: &mut Writer<Cursor<Vec<u8>>>,
    entry: &FeedEntry,
    indent: usize,
    depth: usize,
) -> FeedResult<()> {
    write_indent(w, indent, depth)?;
    w.write_event(Event::Start(BytesStart::new("item")))
        .map_err(map_io)?;
    write_text_el(w, "title", entry.title.as_deref(), indent, depth + 1)?;
    write_text_el(w, "link", entry.link.as_deref(), indent, depth + 1)?;
    let guid = entry.guid.as_deref().or(entry.id.as_deref());
    if let Some(g) = guid {
        let mut gs = BytesStart::new("guid");
        if entry.guid_is_permalink.unwrap_or(true) {
            gs.push_attribute(("isPermaLink", "true"));
        } else {
            gs.push_attribute(("isPermaLink", "false"));
        }
        write_indent(w, indent, depth + 1)?;
        w.write_event(Event::Start(gs)).map_err(map_io)?;
        w.write_event(Event::Text(BytesText::new(g)))
            .map_err(map_io)?;
        w.write_event(Event::End(BytesEnd::new("guid")))
            .map_err(map_io)?;
    }
    if let Some(ms) = entry.published_ms {
        write_text_el(w, "pubDate", Some(&chrono_rfc822(ms)), indent, depth + 1)?;
    }
    if let Some(s) = entry.summary.as_deref() {
        write_text_el(w, "description", Some(s), indent, depth + 1)?;
    }
    for enc in &entry.enclosures {
        let mut e = BytesStart::new("enclosure");
        e.push_attribute(("url", enc.url.as_str()));
        if let Some(mt) = &enc.mime_type {
            e.push_attribute(("type", mt.as_str()));
        }
        if let Some(len) = enc.length {
            let len_s = len.to_string();
            e.push_attribute(("length", len_s.as_str()));
        }
        write_indent(w, indent, depth + 1)?;
        w.write_event(Event::Empty(e)).map_err(map_io)?;
    }
    for cat in &entry.tags {
        let mut c = BytesStart::new("category");
        if let Some(scheme) = &cat.scheme {
            c.push_attribute(("domain", scheme.as_str()));
        }
        write_indent(w, indent, depth + 1)?;
        w.write_event(Event::Start(c)).map_err(map_io)?;
        w.write_event(Event::Text(BytesText::new(&cat.term)))
            .map_err(map_io)?;
        w.write_event(Event::End(BytesEnd::new("category")))
            .map_err(map_io)?;
    }
    write_indent(w, indent, depth)?;
    w.write_event(Event::End(BytesEnd::new("item")))
        .map_err(map_io)?;
    Ok(())
}

fn write_rss_image(
    w: &mut Writer<Cursor<Vec<u8>>>,
    img: &crate::model::FeedImage,
    indent: usize,
    depth: usize,
) -> FeedResult<()> {
    write_indent(w, indent, depth)?;
    w.write_event(Event::Start(BytesStart::new("image")))
        .map_err(map_io)?;
    write_text_el(w, "url", Some(&img.url), indent, depth + 1)?;
    write_text_el(w, "title", img.title.as_deref(), indent, depth + 1)?;
    write_text_el(w, "link", img.link.as_deref(), indent, depth + 1)?;
    write_indent(w, indent, depth)?;
    w.write_event(Event::End(BytesEnd::new("image")))
        .map_err(map_io)?;
    Ok(())
}

fn emit_atom(doc: &FeedDocument, indent: usize) -> FeedResult<String> {
    let mut w = Writer::new(Cursor::new(Vec::new()));
    w.write_event(Event::Decl(BytesDecl::new("1.0", Some("utf-8"), None)))
        .map_err(map_io)?;
    let mut feed = BytesStart::new("feed");
    feed.push_attribute(("xmlns", "http://www.w3.org/2005/Atom"));
    w.write_event(Event::Start(feed)).map_err(map_io)?;
    let feed_meta = &doc.feed;
    write_text_el(&mut w, "title", feed_meta.title.as_deref(), indent, 1)?;
    if let Some(id) = &feed_meta.id {
        write_text_el(&mut w, "id", Some(id), indent, 1)?;
    } else if let Some(link) = &feed_meta.link {
        write_text_el(&mut w, "id", Some(link), indent, 1)?;
    }
    if let Some(link) = &feed_meta.link {
        let mut l = BytesStart::new("link");
        l.push_attribute(("href", link.as_str()));
        l.push_attribute(("rel", "alternate"));
        write_indent(&mut w, indent, 1)?;
        w.write_event(Event::Empty(l)).map_err(map_io)?;
    }
    if let Some(ms) = feed_meta.updated_ms {
        write_text_el(&mut w, "updated", Some(&chrono_rfc3339(ms)), indent, 1)?;
    }
    write_text_el(&mut w, "subtitle", feed_meta.subtitle.as_deref(), indent, 1)?;
    write_text_el(&mut w, "rights", feed_meta.rights.as_deref(), indent, 1)?;
    for entry in &doc.entries {
        write_atom_entry(&mut w, entry, indent, 1)?;
    }
    write_indent(&mut w, indent, 0)?;
    w.write_event(Event::End(BytesEnd::new("feed")))
        .map_err(map_io)?;
    let buf = w.into_inner().into_inner();
    String::from_utf8(buf).map_err(|e| FeedError::Emit(e.to_string()))
}

fn write_atom_entry(
    w: &mut Writer<Cursor<Vec<u8>>>,
    entry: &FeedEntry,
    indent: usize,
    depth: usize,
) -> FeedResult<()> {
    write_indent(w, indent, depth)?;
    w.write_event(Event::Start(BytesStart::new("entry")))
        .map_err(map_io)?;
    write_text_el(w, "title", entry.title.as_deref(), indent, depth + 1)?;
    if let Some(id) = entry.id.as_deref().or(entry.guid.as_deref()) {
        write_text_el(w, "id", Some(id), indent, depth + 1)?;
    }
    if let Some(link) = &entry.link {
        let mut l = BytesStart::new("link");
        l.push_attribute(("href", link.as_str()));
        write_indent(w, indent, depth + 1)?;
        w.write_event(Event::Empty(l)).map_err(map_io)?;
    }
    if let Some(ms) = entry.updated_ms.or(entry.published_ms) {
        write_text_el(w, "updated", Some(&chrono_rfc3339(ms)), indent, depth + 1)?;
    }
    if let Some(ms) = entry.published_ms {
        write_text_el(w, "published", Some(&chrono_rfc3339(ms)), indent, depth + 1)?;
    }
    if let Some(s) = entry.summary.as_deref() {
        let mut sum = BytesStart::new("summary");
        sum.push_attribute(("type", "html"));
        write_indent(w, indent, depth + 1)?;
        w.write_event(Event::Start(sum)).map_err(map_io)?;
        w.write_event(Event::Text(BytesText::new(s)))
            .map_err(map_io)?;
        w.write_event(Event::End(BytesEnd::new("summary")))
            .map_err(map_io)?;
    }
    for part in &entry.content {
        write_atom_content(w, part, indent, depth + 1)?;
    }
    write_indent(w, indent, depth)?;
    w.write_event(Event::End(BytesEnd::new("entry")))
        .map_err(map_io)?;
    Ok(())
}

fn write_atom_content(
    w: &mut Writer<Cursor<Vec<u8>>>,
    part: &ContentPart,
    indent: usize,
    depth: usize,
) -> FeedResult<()> {
    let ctype = if part.mime_type.contains("html") {
        "html"
    } else {
        "text"
    };
    let mut c = BytesStart::new("content");
    c.push_attribute(("type", ctype));
    write_indent(w, indent, depth)?;
    w.write_event(Event::Start(c)).map_err(map_io)?;
    w.write_event(Event::Text(BytesText::new(&part.value)))
        .map_err(map_io)?;
    w.write_event(Event::End(BytesEnd::new("content")))
        .map_err(map_io)?;
    Ok(())
}

fn write_text_el(
    w: &mut Writer<Cursor<Vec<u8>>>,
    name: &str,
    value: Option<&str>,
    indent: usize,
    depth: usize,
) -> FeedResult<()> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.is_empty() {
        return Ok(());
    }
    write_indent(w, indent, depth)?;
    w.write_event(Event::Start(BytesStart::new(name)))
        .map_err(map_io)?;
    w.write_event(Event::Text(BytesText::new(value)))
        .map_err(map_io)?;
    w.write_event(Event::End(BytesEnd::new(name)))
        .map_err(map_io)?;
    Ok(())
}

fn write_indent(w: &mut Writer<Cursor<Vec<u8>>>, indent: usize, depth: usize) -> FeedResult<()> {
    if indent == 0 || depth == 0 {
        return Ok(());
    }
    let pad = " ".repeat(indent * depth);
    w.write_event(Event::Text(BytesText::new(&format!("\n{pad}"))))
        .map_err(map_io)?;
    Ok(())
}

fn chrono_rfc822(ms: i64) -> String {
    use chrono::{TimeZone, Utc};
    let secs = ms.div_euclid(1000);
    let nsec = ((ms.rem_euclid(1000)) * 1_000_000) as u32;
    let dt = Utc
        .timestamp_opt(secs, nsec)
        .single()
        .unwrap_or_else(Utc::now);
    dt.to_rfc2822()
}

fn chrono_rfc3339(ms: i64) -> String {
    use chrono::{TimeZone, Utc};
    let secs = ms.div_euclid(1000);
    let nsec = ((ms.rem_euclid(1000)) * 1_000_000) as u32;
    let dt = Utc
        .timestamp_opt(secs, nsec)
        .single()
        .unwrap_or_else(Utc::now);
    dt.to_rfc3339()
}

fn map_io(e: std::io::Error) -> FeedError {
    FeedError::Emit(e.to_string())
}
