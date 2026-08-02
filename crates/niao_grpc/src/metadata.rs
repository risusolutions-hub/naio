//! Metadata and method-path helpers.

use crate::error::{GrpcError, GrpcResult};
use std::collections::HashMap;

/// ASCII metadata map (lowercase keys). Values are single strings in v0.1.
pub type Metadata = HashMap<String, String>;

pub fn normalize_metadata(raw: &HashMap<String, String>) -> Metadata {
    let mut out = Metadata::new();
    for (k, v) in raw {
        out.insert(k.trim().to_ascii_lowercase(), v.clone());
    }
    out
}

/// Build `/package.Service/Method` (leading slash required).
pub fn method_path(service: &str, method: &str) -> GrpcResult<String> {
    let service = service.trim().trim_start_matches('/');
    let method = method.trim().trim_start_matches('/');
    if service.is_empty() || method.is_empty() {
        return Err(GrpcError::new(
            "service and method must be non-empty for method_path",
        ));
    }
    if service.contains('/') || method.contains('/') {
        return Err(GrpcError::new(
            "service/method segments must not contain '/'",
        ));
    }
    Ok(format!("/{service}/{method}"))
}

/// Parse `/package.Service/Method` into (service, method).
pub fn parse_method(path: &str) -> GrpcResult<(String, String)> {
    let path = path.trim();
    let path = path.strip_prefix('/').unwrap_or(path);
    let mut parts = path.splitn(2, '/');
    let service = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| GrpcError::new(format!("invalid method path: {path}")))?;
    let method = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| GrpcError::new(format!("invalid method path: {path}")))?;
    if method.contains('/') {
        return Err(GrpcError::new(format!("invalid method path: {path}")));
    }
    Ok((service.to_string(), method.to_string()))
}

/// Ensure a method path has a leading slash.
pub fn normalize_method_path(path: &str) -> GrpcResult<String> {
    let path = path.trim();
    if path.is_empty() {
        return Err(GrpcError::new("method path must be non-empty"));
    }
    let with_slash = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    let _ = parse_method(&with_slash)?;
    Ok(with_slash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_helpers() {
        assert_eq!(method_path("echo.Echo", "Say").unwrap(), "/echo.Echo/Say");
        assert_eq!(
            parse_method("/echo.Echo/Say").unwrap(),
            ("echo.Echo".into(), "Say".into())
        );
        assert!(parse_method("nope").is_err());
        assert!(method_path("", "x").is_err());
    }
}
