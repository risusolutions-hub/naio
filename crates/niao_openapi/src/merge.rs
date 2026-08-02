//! Deep-merge of OpenAPI documents (paths + components + tags).

use crate::doc::OpenApiDoc;
use crate::error::OpenApiResult;
use serde_json::{Map, Value};

/// Merge `overlay` into a clone of `base` (overlay wins on scalar conflicts;
/// objects deep-merge; arrays concatenate unique-by-serialization).
pub fn merge(base: &OpenApiDoc, overlay: &OpenApiDoc) -> OpenApiResult<OpenApiDoc> {
    let mut out = base.root.clone();
    deep_merge_map(&mut out, &overlay.root);
    // preserve base openapi version if overlay omitted nothing meaningful —
    // overlay's openapi wins if present (already handled by deep_merge).
    Ok(OpenApiDoc { root: out })
}

fn deep_merge_map(dst: &mut Map<String, Value>, src: &Map<String, Value>) {
    for (k, v) in src {
        match (dst.get_mut(k), v) {
            (Some(Value::Object(d)), Value::Object(s)) => deep_merge_map(d, s),
            (Some(Value::Array(d)), Value::Array(s)) => {
                for item in s {
                    if !d.contains(item) {
                        d.push(item.clone());
                    }
                }
            }
            _ => {
                dst.insert(k.clone(), v.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merge_paths() {
        let a = OpenApiDoc::parse_value(json!({
            "openapi": "3.1.0",
            "info": {"title": "A", "version": "1"},
            "paths": {"/a": {"get": {"responses": {"200": {"description": "ok"}}}}}
        }))
        .unwrap();
        let b = OpenApiDoc::parse_value(json!({
            "openapi": "3.1.0",
            "info": {"title": "B", "version": "2"},
            "paths": {"/b": {"get": {"responses": {"200": {"description": "ok"}}}}}
        }))
        .unwrap();
        let m = merge(&a, &b).unwrap();
        assert!(m.paths().contains(&"/a".to_string()));
        assert!(m.paths().contains(&"/b".to_string()));
        assert_eq!(m.root["info"]["title"], "B");
    }
}
