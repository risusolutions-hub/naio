//! MIME type categories and classification helpers.

use crate::types::FileKind;

/// Classify a MIME top-level / tree into a [`FileKind`].
pub fn kind_of_mime(mime: &str) -> FileKind {
    let m = mime.trim().to_ascii_lowercase();
    let (top, _) = m.split_once('/').unwrap_or((m.as_str(), ""));
    match top {
        "image" => FileKind::Image,
        "video" => FileKind::Video,
        "audio" => FileKind::Audio,
        "text" => FileKind::Text,
        "font" => FileKind::Font,
        "model" => FileKind::Application,
        "application" => classify_application(&m),
        "chemical" | "message" | "multipart" => FileKind::Application,
        _ => FileKind::Unknown,
    }
}

fn classify_application(mime: &str) -> FileKind {
    if mime.contains("json")
        || mime.contains("xml")
        || mime.contains("yaml")
        || mime.contains("javascript")
        || mime.contains("typescript")
        || mime.ends_with("+json")
        || mime.ends_with("+xml")
        || mime.contains("csv")
        || mime.contains("calendar")
        || mime.contains("html")
    {
        return FileKind::Text;
    }
    if mime.contains("zip")
        || mime.contains("compressed")
        || mime.contains("archive")
        || mime.contains("tar")
        || mime.contains("gzip")
        || mime.contains("x-7z")
        || mime.contains("x-rar")
        || mime.contains("x-bzip")
        || mime.contains("zstd")
    {
        return FileKind::Archive;
    }
    if mime.contains("font") {
        return FileKind::Font;
    }
    FileKind::Application
}

pub fn is_image_mime(mime: &str) -> bool {
    kind_of_mime(mime) == FileKind::Image
}

pub fn is_video_mime(mime: &str) -> bool {
    kind_of_mime(mime) == FileKind::Video
}

pub fn is_audio_mime(mime: &str) -> bool {
    kind_of_mime(mime) == FileKind::Audio
}

pub fn is_text_mime(mime: &str) -> bool {
    kind_of_mime(mime) == FileKind::Text
}

pub fn is_archive_mime(mime: &str) -> bool {
    kind_of_mime(mime) == FileKind::Archive
}

pub fn is_font_mime(mime: &str) -> bool {
    kind_of_mime(mime) == FileKind::Font
}

pub fn kind_name(kind: FileKind) -> &'static str {
    kind.as_str()
}
