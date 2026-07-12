//! Shared HTTP types (`http` crate-compatible surface).

mod header_name;
mod header_value;
mod status_code;
mod uri;

pub use header_name::{HeaderName, InvalidHeaderName};
pub use header_value::{HeaderValue, InvalidHeaderValue, ToStrError};
pub use status_code::{InvalidStatusCode, StatusCode};
pub use uri::{InvalidUri, Uri};

pub use crate::headers::HeaderMap;
pub use crate::method::Method;

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    /// Vectors aligned with `http` crate semantics (generated before removal).
    const FIXTURE_HEADERS: &[(&str, &str)] = &[
        ("Accept", "text/html"),
        ("Content-Type", "application/json"),
        ("X-Custom", "value"),
    ];

    #[test]
    fn header_case_insensitivity() {
        let mut map = HeaderMap::new();
        for (name, value) in FIXTURE_HEADERS {
            map.insert(*name, *value);
        }
        for (name, value) in FIXTURE_HEADERS {
            assert_eq!(map.get(&name.to_ascii_lowercase()), Some(*value));
            assert_eq!(map.get(&name.to_ascii_uppercase()), Some(*value));
        }
    }

    #[test]
    fn status_code_constants() {
        assert_eq!(StatusCode::OK.as_u16(), 200);
        assert_eq!(StatusCode::NOT_FOUND.canonical_reason(), Some("Not Found"));
        assert!(StatusCode::NO_CONTENT.is_success());
    }

    #[test]
    fn uri_and_method_parse() {
        let uri: Uri = "/api/v1?q=1".parse().unwrap();
        assert_eq!(uri.path(), "/api/v1");
        assert_eq!(Method::from_str("POST").unwrap(), Method::POST);
    }

    #[test]
    fn header_name_value_pipeline() {
        let name = HeaderName::from_str("ETag").unwrap();
        let value = HeaderValue::from_str("\"abc\"").unwrap();
        let mut map = HeaderMap::new();
        map.insert_typed(name, value);
        assert_eq!(map.get("etag"), Some("\"abc\""));
    }
}
