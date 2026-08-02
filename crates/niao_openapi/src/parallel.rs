//! Parallel validate / client-stub over many documents.

use crate::client::client_stub_str;
use crate::doc::OpenApiDoc;
use crate::validate::{validate, ValidationReport};
use niao_parallel::{available_threads, map as parallel_map};
use serde_json::{Map, Value};

pub fn parallel_validate(docs: &[OpenApiDoc], threads: usize) -> Vec<ValidationReport> {
    if docs.is_empty() {
        return Vec::new();
    }
    let threads = if threads == 0 {
        available_threads()
    } else {
        threads.max(1)
    };
    parallel_map(docs, threads, |doc| {
        validate(doc).unwrap_or_else(|e| ValidationReport {
            ok: false,
            errors: vec![crate::validate::ValidationIssue {
                path: "".into(),
                message: e.message().to_string(),
            }],
        })
    })
}

pub fn parallel_client_stubs(
    docs: &[OpenApiDoc],
    opts: Option<&Map<String, Value>>,
    threads: usize,
) -> Vec<Result<String, String>> {
    if docs.is_empty() {
        return Vec::new();
    }
    let threads = if threads == 0 {
        available_threads()
    } else {
        threads.max(1)
    };
    let opts_owned = opts.cloned();
    parallel_map(docs, threads, move |doc| {
        client_stub_str(doc, opts_owned.as_ref()).map_err(|e| e.message().to_string())
    })
}

/// Convenience for benches: build N identical shallow docs.
pub fn sample_routes(n: usize) -> Vec<Value> {
    let mut routes = Vec::with_capacity(n);
    for i in 0..n {
        routes.push(serde_json::json!({
            "method": if i % 2 == 0 { "GET" } else { "POST" },
            "path": format!("/items/{i}"),
            "summary": format!("Item {i}"),
            "body": if i % 2 == 1 { serde_json::json!({"name": "x", "n": i}) } else { Value::Null }
        }));
    }
    // remove null body key for GETs — cleaner
    for r in &mut routes {
        if let Some(obj) = r.as_object_mut() {
            if obj.get("method").and_then(|m| m.as_str()) == Some("GET") {
                obj.remove("body");
            }
        }
    }
    routes
}
