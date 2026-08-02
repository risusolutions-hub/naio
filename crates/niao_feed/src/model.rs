use serde::{Deserialize, Serialize};

/// MIME / text body part (summary, content).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentPart {
    pub value: String,
    pub mime_type: String,
    pub language: Option<String>,
    pub base: Option<String>,
}

/// Person (author / contributor).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Person {
    pub name: Option<String>,
    pub email: Option<String>,
    pub uri: Option<String>,
}

/// Hyperlink with optional relation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedLink {
    pub href: String,
    pub rel: Option<String>,
    pub title: Option<String>,
    pub mime_type: Option<String>,
    pub length: Option<u64>,
}

/// Media enclosure (podcast episode, attachment).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Enclosure {
    pub url: String,
    pub mime_type: Option<String>,
    pub length: Option<u64>,
    pub title: Option<String>,
}

/// Category / tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Category {
    pub term: String,
    pub scheme: Option<String>,
    pub label: Option<String>,
}

/// Feed-level metadata.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FeedMeta {
    pub title: Option<String>,
    pub link: Option<String>,
    pub id: Option<String>,
    pub subtitle: Option<String>,
    pub rights: Option<String>,
    pub language: Option<String>,
    pub updated: Option<String>,
    pub updated_ms: Option<i64>,
    pub published: Option<String>,
    pub published_ms: Option<i64>,
    pub generator: Option<String>,
    pub icon: Option<String>,
    pub logo: Option<String>,
    pub ttl: Option<i64>,
    pub authors: Vec<Person>,
    pub links: Vec<FeedLink>,
    pub categories: Vec<Category>,
    pub image: Option<FeedImage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedImage {
    pub url: String,
    pub title: Option<String>,
    pub link: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
}

/// Single feed entry / item.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FeedEntry {
    pub id: Option<String>,
    pub title: Option<String>,
    pub link: Option<String>,
    pub summary: Option<String>,
    pub summary_detail: Option<ContentPart>,
    pub content: Vec<ContentPart>,
    pub published: Option<String>,
    pub published_ms: Option<i64>,
    pub updated: Option<String>,
    pub updated_ms: Option<i64>,
    pub author: Option<String>,
    pub authors: Vec<Person>,
    pub links: Vec<FeedLink>,
    pub tags: Vec<Category>,
    pub enclosures: Vec<Enclosure>,
    pub guid: Option<String>,
    pub guid_is_permalink: Option<bool>,
}

/// Parsed or built feed document (feedparser-shaped).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedDocument {
    /// Detected format version, e.g. `rss20`, `atom10`, `json11`.
    pub version: String,
    /// True when the source was malformed but partially recovered.
    pub bozo: bool,
    pub bozo_exception: Option<String>,
    pub encoding: Option<String>,
    pub feed: FeedMeta,
    pub entries: Vec<FeedEntry>,
}

impl FeedDocument {
    pub fn new(version: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            bozo: false,
            bozo_exception: None,
            encoding: None,
            feed: FeedMeta::default(),
            entries: Vec::new(),
        }
    }
}
