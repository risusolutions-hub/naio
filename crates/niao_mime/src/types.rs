//! Shared result types.

use crate::magic::MagicSignature;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Image,
    Video,
    Audio,
    Text,
    Archive,
    Font,
    Application,
    Unknown,
}

impl FileKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Text => "text",
            Self::Archive => "archive",
            Self::Font => "font",
            Self::Application => "application",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "image" => Self::Image,
            "video" => Self::Video,
            "audio" => Self::Audio,
            "text" => Self::Text,
            "archive" => Self::Archive,
            "font" => Self::Font,
            "application" => Self::Application,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchSource {
    Magic,
    Extension,
    Combined,
}

/// A resolved MIME match.
#[derive(Debug, Clone, PartialEq)]
pub struct MimeMatch {
    pub mime: String,
    pub extension: String,
    pub kind: FileKind,
    pub source: MatchSource,
    /// 0.0–1.0 heuristic confidence.
    pub confidence: f64,
}

impl MimeMatch {
    pub fn from_static(sig: &MagicSignature) -> Self {
        Self {
            mime: sig.mime.into(),
            extension: sig.ext.into(),
            kind: sig.kind,
            source: MatchSource::Magic,
            confidence: 0.95,
        }
    }

    pub fn beats(&self, other: &Self, priority: u8) -> bool {
        if priority >= 90 && self.confidence > other.confidence {
            return true;
        }
        if (self.confidence - other.confidence).abs() > f64::EPSILON {
            return self.confidence > other.confidence;
        }
        self.source == MatchSource::Magic && other.source == MatchSource::Extension
    }
}

/// Result of `guess_type` (mimetypes-style).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuessTypeResult {
    pub mime: Option<String>,
    pub encoding: Option<String>,
}
