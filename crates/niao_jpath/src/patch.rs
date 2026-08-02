//! RFC 6902 JSON Patch and RFC 7396 JSON Merge Patch.

use crate::error::{JpathError, JpathResult};
use json_patch::{Patch, PatchOperation};
use serde_json::Value;

/// Apply RFC 6902 JSON Patch; returns a new document.
///
/// >>> njpath.patch_apply({"a": 1}, [{"op": "add", "path": "/b", "value": 2}])
/// {"a": 1, "b": 2}
pub fn apply(doc: &Value, patch_ops: &Value) -> JpathResult<Value> {
    let patch: Patch = serde_json::from_value(patch_ops.clone())
        .map_err(|e| JpathError::InvalidPatch(format!("invalid patch document: {e}")))?;
    let mut out = doc.clone();
    json_patch::patch(&mut out, &patch).map_err(|e| JpathError::PatchFailed(e.to_string()))?;
    Ok(out)
}

/// Convert patch Value to list of operation names (for introspection).
///
/// >>> njpath.patch_valid([{"op": "remove", "path": "/x"}])
/// true
pub fn valid(patch_ops: &Value) -> bool {
    serde_json::from_value::<Patch>(patch_ops.clone()).is_ok()
}

/// Test whether patch would succeed (dry-run via clone).
pub fn test(doc: &Value, patch_ops: &Value) -> JpathResult<bool> {
    match apply(doc, patch_ops) {
        Ok(_) => Ok(true),
        Err(JpathError::PatchFailed(_)) => Ok(false),
        Err(e) => Err(e),
    }
}

/// Generate RFC 6902 diff from `from` to `to`.
///
/// >>> len(njpath.diff({"a": 1}, {"a": 2}))
/// 1
pub fn diff(from: &Value, to: &Value) -> Value {
    let patch = json_patch::diff(from, to);
    serde_json::to_value(patch).unwrap_or(Value::Array(vec![]))
}

/// Apply RFC 7396 JSON Merge Patch.
///
/// >>> njpath.merge({"a": {"b": 1}}, {"a": {"c": 2}})
pub fn merge(doc: &Value, patch: &Value) -> JpathResult<Value> {
    let mut out = doc.clone();
    json_patch::merge(&mut out, patch);
    Ok(out)
}

/// Convert patch Value to list of operation names (for introspection).
pub fn op_names(patch_ops: &Value) -> JpathResult<Vec<String>> {
    let patch: Patch = serde_json::from_value(patch_ops.clone())
        .map_err(|e| JpathError::InvalidPatch(format!("invalid patch document: {e}")))?;
    Ok(patch
        .0
        .iter()
        .map(|op| match op {
            PatchOperation::Add(_) => "add".to_string(),
            PatchOperation::Remove(_) => "remove".to_string(),
            PatchOperation::Replace(_) => "replace".to_string(),
            PatchOperation::Move(_) => "move".to_string(),
            PatchOperation::Copy(_) => "copy".to_string(),
            PatchOperation::Test(_) => "test".to_string(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn apply_add() {
        let doc = json!({"a": 1});
        let patch = json!([{"op": "add", "path": "/b", "value": 2}]);
        let out = apply(&doc, &patch).unwrap();
        assert_eq!(out, json!({"a": 1, "b": 2}));
    }

    #[test]
    fn diff_and_merge() {
        let a = json!({"title": "Hi", "tags": ["x", "y"]});
        let b = json!({"title": "Bye", "tags": ["x"]});
        let patch = diff(&a, &b);
        assert!(valid(&patch));
        let merged = merge(&a, &json!({"title": "Bye"})).unwrap();
        assert_eq!(merged["title"], json!("Bye"));
    }
}
