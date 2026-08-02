//! Integration tests mirroring `tests/nopenapi.niao` (ntest suite).

use crate::{
    client_stub_str, from_ahiru, from_routes, infer_schema, is_valid, merge, normalize_path,
    operation_id, path_params, OpenApiDoc,
};
use serde_json::{json, Map, Value};

fn info(title: &str, version: &str) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("title".into(), json!(title));
    m.insert("version".into(), json!(version));
    m
}

#[test]
fn ntest_normalize_path_colon() {
    assert_eq!(normalize_path("/users/:id"), "/users/{id}");
}

#[test]
fn ntest_normalize_path_empty() {
    assert_eq!(normalize_path(""), "/");
}

#[test]
fn ntest_path_params() {
    let p = path_params("/users/:id/posts/:pid");
    assert_eq!(p, vec!["id", "pid"]);
}

#[test]
fn ntest_operation_id() {
    assert_eq!(
        operation_id("GET", "/users/:id").unwrap(),
        "get_users_by_id"
    );
}

#[test]
fn ntest_from_ahiru_paths() {
    let routes = vec![
        json!({"method": "GET", "path": "/health"}),
        json!({
            "method": "GET",
            "path": "/users/:id",
            "permission": "users.read"
        }),
    ];
    let doc = from_ahiru(&routes, Some(&info("T", "1")), None).unwrap();
    assert_eq!(doc.paths(), vec!["/health", "/users/{id}"]);
    assert!(is_valid(&doc).unwrap());
}

#[test]
fn ntest_infer_schema_object() {
    let s = infer_schema(&json!({"id": 1, "name": "a"}));
    assert_eq!(s["type"], "object");
}

#[test]
fn ntest_client_stub_contains_http() {
    let routes = vec![json!({
        "method": "GET",
        "path": "/health",
        "summary": "ok"
    })];
    let doc = from_routes(&routes, Some(&info("API", "1")), None).unwrap();
    let stub = client_stub_str(&doc, None).unwrap();
    assert!(stub.contains("import \"http\""));
    assert!(stub.contains("http.get"));
}

#[test]
fn ntest_parse_invalid() {
    assert!(OpenApiDoc::parse_str("{").is_err());
}

#[test]
fn ntest_merge_paths() {
    let a = from_routes(
        &[json!({"method": "GET", "path": "/a"})],
        Some(&info("A", "1")),
        None,
    )
    .unwrap();
    let b = from_routes(
        &[json!({"method": "GET", "path": "/b"})],
        Some(&info("B", "2")),
        None,
    )
    .unwrap();
    let m = merge(&a, &b).unwrap();
    let paths = m.paths();
    assert_eq!(paths.len(), 2);
    assert!(paths.contains(&"/a".to_string()));
    assert!(paths.contains(&"/b".to_string()));
}

#[test]
fn ntest_unicode_path() {
    let routes = vec![json!({
        "method": "GET",
        "path": "/こんにちは",
        "summary": "挨拶"
    })];
    let doc = from_routes(&routes, Some(&info("日本語", "1")), None).unwrap();
    assert!(is_valid(&doc).unwrap());
}

#[test]
fn ntest_empty_routes_rejected_for_stub() {
    let doc = OpenApiDoc::create(&info("T", "1"), None).unwrap();
    assert!(client_stub_str(&doc, None).is_err());
}

#[test]
fn ntest_websocket_skipped_by_default() {
    let routes = vec![json!({
        "method": "GET",
        "path": "/ws",
        "websocket": true
    })];
    let doc = from_ahiru(&routes, None, None).unwrap();
    assert!(doc.paths().is_empty());
}
