/// Sniff feed container format from raw bytes (before full parse).
///
/// >>> use niao_feed::detect_format;
/// >>> detect_format(b"<rss version=\"2.0\">")
/// Some("rss".into())
pub fn detect_format(bytes: &[u8]) -> Option<String> {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
    let trimmed = head.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return Some("json".into());
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("<feed") && lower.contains("xmlns=\"http://www.w3.org/2005/atom\"") {
        return Some("atom".into());
    }
    if lower.contains("<feed") {
        return Some("atom".into());
    }
    if lower.contains("<rdf:rdf") || lower.contains("xmlns=\"http://purl.org/rss/1.0/\"") {
        return Some("rss1".into());
    }
    if lower.contains("<rss") {
        return Some("rss".into());
    }
    None
}

/// Infer feed version string from sniff + RSS version attribute when present.
///
/// >>> use niao_feed::detect_version;
/// >>> detect_version(b"<rss version=\"2.0\"><channel></channel></rss>")
/// Some("rss20".into())
pub fn detect_version(bytes: &[u8]) -> Option<String> {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(8192)]);
    let trimmed = head.trim_start();
    if trimmed.starts_with('{') {
        if trimmed.contains("\"version\"") && trimmed.contains("https://jsonfeed.org/version/1.1") {
            return Some("json11".into());
        }
        if trimmed.contains("\"version\"") {
            return Some("json10".into());
        }
        return Some("json".into());
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("<feed") {
        return Some("atom10".into());
    }
    if lower.contains("<rdf:rdf") || lower.contains("xmlns:rdf=") {
        return Some("rss10".into());
    }
    if let Some(start) = lower.find("<rss") {
        let slice = &lower[start..];
        if let Some(ver_start) = slice.find("version=") {
            let rest = &slice[ver_start + 8..];
            let quote = rest.chars().next()?;
            let end = rest[1..].find(quote)?;
            let ver = &rest[1..1 + end];
            return Some(
                match ver {
                    "2.0" => "rss20",
                    "0.91" => "rss091",
                    "0.92" => "rss092",
                    other => return Some(format!("rss{other}")),
                }
                .into(),
            );
        }
        return Some("rss20".into());
    }
    None
}

pub(crate) fn version_from_feed_rs(feed: &feed_rs::model::Feed) -> String {
    use feed_rs::model::FeedType;
    match feed.feed_type {
        FeedType::Atom => "atom10".into(),
        FeedType::RSS0 => "rss090".into(),
        FeedType::RSS1 => "rss10".into(),
        FeedType::RSS2 => "rss20".into(),
        FeedType::JSON => "json10".into(),
    }
}
