//! Path helpers: `:id` / `{id}` normalization and parameter extraction.

use crate::error::{OpenApiError, OpenApiResult};

/// Convert ahiru-style `/users/:id` (or already OpenAPI `/users/{id}`) to OpenAPI path form.
pub fn normalize_path(path: &str) -> String {
    if path.is_empty() || path == "/" {
        return "/".to_string();
    }
    let mut out = String::with_capacity(path.len() + 4);
    for part in path.split('/') {
        if part.is_empty() {
            continue;
        }
        out.push('/');
        if let Some(name) = part.strip_prefix(':') {
            out.push('{');
            out.push_str(name);
            out.push('}');
        } else {
            out.push_str(part);
        }
    }
    if out.is_empty() {
        "/".into()
    } else {
        out
    }
}

/// Extract `{param}` names from an OpenAPI-style path (after normalize).
pub fn path_params(path: &str) -> Vec<String> {
    let norm = normalize_path(path);
    let mut params = Vec::new();
    for part in norm.split('/') {
        if let Some(inner) = part.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
            if !inner.is_empty() {
                params.push(inner.to_string());
            }
        }
    }
    params
}

/// HTTP method → lowercase OpenAPI key (`get`, `post`, …).
pub fn method_key(method: &str) -> OpenApiResult<String> {
    let m = method.trim().to_ascii_lowercase();
    match m.as_str() {
        "get" | "post" | "put" | "delete" | "patch" | "options" | "head" | "trace" => Ok(m),
        "" => Err(OpenApiError::new("HTTP method must not be empty")),
        other => Err(OpenApiError::new(format!(
            "unsupported HTTP method: {other}"
        ))),
    }
}

/// Derive a FastAPI-style `operationId` from method + path.
pub fn operation_id(method: &str, path: &str) -> OpenApiResult<String> {
    let m = method_key(method)?;
    let norm = normalize_path(path);
    let mut parts: Vec<String> = Vec::new();
    parts.push(m);
    for part in norm.split('/') {
        if part.is_empty() {
            continue;
        }
        if let Some(inner) = part.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
            parts.push(format!("by_{}", sanitize_ident(inner)));
        } else {
            parts.push(sanitize_ident(part));
        }
    }
    if parts.len() == 1 {
        parts.push("root".into());
    }
    Ok(parts.join("_"))
}

fn sanitize_ident(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "x".into()
    } else if out.as_bytes()[0].is_ascii_digit() {
        format!("n{out}")
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_colon_and_brace() {
        assert_eq!(normalize_path("/users/:id"), "/users/{id}");
        assert_eq!(normalize_path("/users/{id}"), "/users/{id}");
        assert_eq!(normalize_path(""), "/");
        assert_eq!(normalize_path("/"), "/");
    }

    #[test]
    fn extract_params() {
        assert_eq!(
            path_params("/users/:id/posts/:postId"),
            vec!["id", "postId"]
        );
        assert_eq!(path_params("/health"), Vec::<String>::new());
    }

    #[test]
    fn op_id() {
        assert_eq!(
            operation_id("GET", "/users/:id").unwrap(),
            "get_users_by_id"
        );
        assert_eq!(operation_id("post", "/").unwrap(), "post_root");
    }
}
