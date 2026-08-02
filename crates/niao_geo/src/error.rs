//! Error types for ngeo.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum GeoError {
    Parse(String),
    InvalidCoord(String),
    EmptyGeometry,
    RingTooShort,
    InvalidZoom,
    InvalidTile,
    InvalidQuadkey,
    InvalidHandle,
    TypeMismatch(String),
    OutOfRange(String),
}

impl fmt::Display for GeoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(m) => write!(f, "{m}"),
            Self::InvalidCoord(m) => write!(f, "invalid coordinate: {m}"),
            Self::EmptyGeometry => write!(f, "empty geometry"),
            Self::RingTooShort => write!(f, "ring needs at least 3 distinct points"),
            Self::InvalidZoom => write!(f, "zoom must be 0..=30"),
            Self::InvalidTile => write!(f, "tile coordinates out of range for zoom"),
            Self::InvalidQuadkey => write!(f, "invalid quadkey"),
            Self::InvalidHandle => write!(f, "invalid or closed ngeo handle"),
            Self::TypeMismatch(m) => write!(f, "{m}"),
            Self::OutOfRange(m) => write!(f, "out of range: {m}"),
        }
    }
}

impl std::error::Error for GeoError {}

pub type GeoResult<T> = Result<T, GeoError>;
