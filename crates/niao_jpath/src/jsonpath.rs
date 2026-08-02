//! JSONPath queries (~jsonpath-ng / jsonpath-rust subset).

use crate::error::{JpathError, JpathResult};
use jsonpath_lib::{self, JsonPathError};
use serde_json::Value;
use std::sync::Arc;

#[allow(deprecated)]
pub struct CompiledJsonPath {
    inner: jsonpath_lib::Compiled,
    query: Arc<str>,
}

impl CompiledJsonPath {
    pub fn query(&self) -> &str {
        &self.query
    }
}

fn map_jsonpath_err(e: JsonPathError) -> JpathError {
    JpathError::InvalidJsonPath(e.to_string())
}

fn map_compile_err(s: String) -> JpathError {
    JpathError::InvalidJsonPath(s)
}

/// True when `query` parses as JSONPath.
///
/// >>> njpath.path_valid("$.store.book[*].author")
/// true
pub fn valid(query: &str) -> bool {
    #[allow(deprecated)]
    {
        jsonpath_lib::Compiled::compile(query).is_ok()
    }
}

/// Compile a JSONPath expression for reuse.
pub fn compile(query: &str) -> JpathResult<CompiledJsonPath> {
    #[allow(deprecated)]
    let inner = jsonpath_lib::Compiled::compile(query).map_err(map_compile_err)?;
    Ok(CompiledJsonPath {
        inner,
        query: Arc::from(query),
    })
}

/// Find all matches for `query` in `doc`.
///
/// >>> njpath.find({"a": [{"b": 1}, {"b": 2}]}, "$.a[*].b")
/// [1, 2]
pub fn find(doc: &Value, query: &str) -> JpathResult<Vec<Value>> {
    jsonpath_lib::select(doc, query)
        .map(|v| v.into_iter().cloned().collect())
        .map_err(map_jsonpath_err)
}

/// First match or Null.
///
/// >>> njpath.find_one({"x": 42}, "$.x")
/// 42
pub fn find_one(doc: &Value, query: &str) -> JpathResult<Value> {
    let hits = find(doc, query)?;
    Ok(hits.into_iter().next().unwrap_or(Value::Null))
}

/// Search with a compiled expression.
pub fn search(compiled: &CompiledJsonPath, doc: &Value) -> JpathResult<Vec<Value>> {
    #[allow(deprecated)]
    {
        compiled
            .inner
            .select(doc)
            .map(|v| v.into_iter().cloned().collect())
            .map_err(map_jsonpath_err)
    }
}

/// Replace values matching JSONPath.
pub fn replace(doc: &Value, query: &str, replacement: &Value) -> JpathResult<Value> {
    let replacement = replacement.clone();
    jsonpath_lib::replace_with(doc.clone(), query, &mut |_v| Some(replacement.clone()))
        .map_err(map_jsonpath_err)
}

/// Delete (set to null) all values matching JSONPath.
pub fn delete(doc: &Value, query: &str) -> JpathResult<Value> {
    jsonpath_lib::delete(doc.clone(), query).map_err(map_jsonpath_err)
}

/// Return match paths as JSON Pointer strings (best-effort from result structure).
pub fn find_pointers(doc: &Value, query: &str) -> JpathResult<Vec<String>> {
    let hits = find(doc, query)?;
    let mut paths = Vec::with_capacity(hits.len());
    for hit in hits {
        if let Some(p) = locate_pointer(doc, &hit) {
            paths.push(p);
        }
    }
    Ok(paths)
}

fn locate_pointer(doc: &Value, target: &Value) -> Option<String> {
    fn walk(doc: &Value, target: &Value, path: &mut String) -> Option<String> {
        if doc == target {
            return Some(path.clone());
        }
        match doc {
            Value::Object(map) => {
                for (k, v) in map {
                    let saved = path.clone();
                    path.push('/');
                    path.push_str(&crate::pointer::escape(k));
                    if let Some(p) = walk(v, target, path) {
                        return Some(p);
                    }
                    *path = saved;
                }
            }
            Value::Array(arr) => {
                for (i, v) in arr.iter().enumerate() {
                    let saved = path.clone();
                    path.push('/');
                    path.push_str(&i.to_string());
                    if let Some(p) = walk(v, target, path) {
                        return Some(p);
                    }
                    *path = saved;
                }
            }
            _ => {}
        }
        None
    }
    let mut path = String::new();
    walk(doc, target, &mut path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn store() -> Value {
        json!({
            "store": {
                "book": [
                    {"author": "A", "price": 8.95},
                    {"author": "B", "price": 12.99}
                ],
                "bicycle": {"color": "red"}
            }
        })
    }

    #[test]
    fn find_authors() {
        let hits = find(&store(), "$.store.book[*].author").unwrap();
        assert_eq!(hits, vec![json!("A"), json!("B")]);
    }

    #[test]
    fn compiled_search() {
        let c = compile("$.store.book[*].price").unwrap();
        let hits = search(&c, &store()).unwrap();
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn find_one_missing() {
        assert_eq!(find_one(&json!({}), "$.missing").unwrap(), Value::Null);
    }
}
