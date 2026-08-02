//! RFC 6901 JSON Pointer — native implementation over `serde_json::Value`.

use crate::error::{JpathError, JpathResult};
use crate::value::values_equal;
use serde_json::{Map, Value};

/// Escape a single reference token per RFC 6901.
///
/// >>> njpath.pointer_escape("a/b")
/// "a~1b"
pub fn escape(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

/// Unescape a single reference token per RFC 6901.
///
/// >>> njpath.pointer_unescape("a~1b")
/// "a/b"
pub fn unescape(token: &str) -> String {
    let mut out = String::with_capacity(token.len());
    let mut chars = token.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '~' {
            match chars.next() {
                Some('0') => out.push('~'),
                Some('1') => out.push('/'),
                Some(other) => {
                    return format!("~{other}");
                }
                None => return token.to_string(),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn parse_tokens(pointer: &str) -> JpathResult<Vec<String>> {
    if pointer.is_empty() {
        return Ok(Vec::new());
    }
    if !pointer.starts_with('/') {
        return Err(JpathError::InvalidPointer(format!(
            "pointer must start with '/' or be empty, got {pointer:?}"
        )));
    }
    if pointer == "/" {
        return Ok(vec![String::new()]);
    }
    let mut tokens = Vec::new();
    for raw in pointer[1..].split('/') {
        tokens.push(unescape(raw));
    }
    Ok(tokens)
}

fn navigate<'a>(doc: &'a Value, tokens: &[String]) -> JpathResult<&'a Value> {
    let mut current = doc;
    for token in tokens {
        current = match current {
            Value::Object(map) => map
                .get(token.as_str())
                .ok_or_else(|| JpathError::PointerNotFound(token.clone()))?,
            Value::Array(arr) => {
                let idx: usize = token.parse().map_err(|_| {
                    JpathError::InvalidPointer(format!("invalid array index {token:?}"))
                })?;
                arr.get(idx)
                    .ok_or_else(|| JpathError::PointerNotFound(token.clone()))?
            }
            _ => {
                return Err(JpathError::PointerNotFound(token.clone()));
            }
        };
    }
    Ok(current)
}

fn navigate_mut<'a>(doc: &'a mut Value, tokens: &[String]) -> JpathResult<&'a mut Value> {
    let mut current = doc;
    for token in tokens {
        current = match current {
            Value::Object(map) => map
                .get_mut(token.as_str())
                .ok_or_else(|| JpathError::PointerNotFound(token.clone()))?,
            Value::Array(arr) => {
                let idx: usize = token.parse().map_err(|_| {
                    JpathError::InvalidPointer(format!("invalid array index {token:?}"))
                })?;
                arr.get_mut(idx)
                    .ok_or_else(|| JpathError::PointerNotFound(token.clone()))?
            }
            _ => {
                return Err(JpathError::PointerNotFound(token.clone()));
            }
        };
    }
    Ok(current)
}

/// Join pointer base and token.
///
/// >>> njpath.pointer_join("/a", "b")
/// "/a/b"
pub fn join(base: &str, token: &str) -> JpathResult<String> {
    if base.is_empty() {
        return Ok(format!("/{}", escape(token)));
    }
    if !base.starts_with('/') {
        return Err(JpathError::InvalidPointer(format!(
            "base pointer must start with '/', got {base:?}"
        )));
    }
    Ok(format!("{}/{}", base.trim_end_matches('/'), escape(token)))
}

/// Parent pointer, or empty string for root-level tokens.
///
/// >>> njpath.pointer_parent("/a/b/c")
/// "/a/b"
pub fn parent(pointer: &str) -> JpathResult<String> {
    if pointer.is_empty() {
        return Ok(String::new());
    }
    if !pointer.starts_with('/') {
        return Err(JpathError::InvalidPointer(format!(
            "pointer must start with '/', got {pointer:?}"
        )));
    }
    match pointer.rfind('/') {
        None | Some(0) => Ok(String::new()),
        Some(pos) => Ok(pointer[..pos].to_string()),
    }
}

/// True when the pointer resolves in `doc`.
///
/// >>> njpath.pointer_exists({"a": 1}, "/a")
/// true
pub fn exists(doc: &Value, pointer: &str) -> JpathResult<bool> {
    let tokens = parse_tokens(pointer)?;
    Ok(navigate(doc, &tokens).is_ok())
}

/// Get value at pointer, or `Null` when missing (non-throwing).
///
/// >>> njpath.pointer_get({"a": {"b": 2}}, "/a/b")
/// 2
pub fn get(doc: &Value, pointer: &str) -> JpathResult<Value> {
    let tokens = parse_tokens(pointer)?;
    match navigate(doc, &tokens) {
        Ok(v) => Ok(v.clone()),
        Err(JpathError::PointerNotFound(_)) if !pointer.is_empty() => Ok(Value::Null),
        Err(e) => Err(e),
    }
}

/// Get value at pointer; error when missing.
pub fn resolve(doc: &Value, pointer: &str) -> JpathResult<Value> {
    let tokens = parse_tokens(pointer)?;
    Ok(navigate(doc, &tokens)?.clone())
}

/// Set value at pointer, returning a new document (immutable update).
///
/// >>> njpath.pointer_set({"a": 1}, "/a", 2)["a"]
/// 2
pub fn set(doc: &Value, pointer: &str, value: Value) -> JpathResult<Value> {
    let mut out = doc.clone();
    set_in_place(&mut out, pointer, value)?;
    Ok(out)
}

/// Set value at pointer in place.
pub fn set_in_place(doc: &mut Value, pointer: &str, value: Value) -> JpathResult<()> {
    let tokens = parse_tokens(pointer)?;
    if tokens.is_empty() {
        *doc = value;
        return Ok(());
    }
    let last = tokens.len() - 1;
    let parent_tokens = &tokens[..last];
    let key = &tokens[last];

    let parent = if parent_tokens.is_empty() {
        doc
    } else {
        navigate_mut(doc, parent_tokens)?
    };

    match parent {
        Value::Object(map) => {
            map.insert(key.clone(), value);
        }
        Value::Array(arr) => {
            let idx: usize = key
                .parse()
                .map_err(|_| JpathError::InvalidPointer(format!("invalid array index {key:?}")))?;
            if idx == arr.len() {
                arr.push(value);
            } else if idx < arr.len() {
                arr[idx] = value;
            } else {
                return Err(JpathError::InvalidPointer(format!(
                    "array index {idx} out of range (len {})",
                    arr.len()
                )));
            }
        }
        _ => {
            return Err(JpathError::TypeMismatch(format!(
                "cannot set {pointer:?} on non-container value"
            )));
        }
    }
    Ok(())
}

/// Remove value at pointer, returning new document.
pub fn remove(doc: &Value, pointer: &str) -> JpathResult<Value> {
    let mut out = doc.clone();
    remove_in_place(&mut out, pointer)?;
    Ok(out)
}

/// Remove value at pointer in place.
pub fn remove_in_place(doc: &mut Value, pointer: &str) -> JpathResult<()> {
    let tokens = parse_tokens(pointer)?;
    if tokens.is_empty() {
        return Err(JpathError::InvalidPointer(
            "cannot remove document root".into(),
        ));
    }
    let last = tokens.len() - 1;
    let parent_tokens = &tokens[..last];
    let key = &tokens[last];
    let parent = navigate_mut(doc, parent_tokens)?;
    match parent {
        Value::Object(map) => {
            if map.remove(key.as_str()).is_none() {
                return Err(JpathError::PointerNotFound(key.clone()));
            }
        }
        Value::Array(arr) => {
            let idx: usize = key
                .parse()
                .map_err(|_| JpathError::InvalidPointer(format!("invalid array index {key:?}")))?;
            if idx >= arr.len() {
                return Err(JpathError::PointerNotFound(key.clone()));
            }
            arr.remove(idx);
        }
        _ => {
            return Err(JpathError::TypeMismatch(format!(
                "cannot remove {pointer:?} from non-container"
            )));
        }
    }
    Ok(())
}

/// RFC 6902 test: compare value at pointer with expected.
pub fn test(doc: &Value, pointer: &str, expected: &Value) -> JpathResult<bool> {
    let actual = resolve(doc, pointer)?;
    Ok(values_equal(&actual, expected))
}

/// Create intermediate objects/arrays along a pointer path.
pub fn create_path(doc: &mut Value, pointer: &str, leaf: Value) -> JpathResult<()> {
    let tokens = parse_tokens(pointer)?;
    if tokens.is_empty() {
        *doc = leaf;
        return Ok(());
    }

    let mut current = doc;
    for (i, token) in tokens.iter().enumerate() {
        let is_last = i + 1 == tokens.len();
        if is_last {
            match current {
                Value::Object(map) => {
                    map.insert(token.clone(), leaf);
                }
                Value::Array(arr) => {
                    let idx: usize = token.parse().map_err(|_| {
                        JpathError::InvalidPointer(format!("invalid array index {token:?}"))
                    })?;
                    while arr.len() <= idx {
                        arr.push(Value::Null);
                    }
                    arr[idx] = leaf;
                }
                _ => {
                    return Err(JpathError::TypeMismatch(format!(
                        "cannot create leaf at {pointer:?}"
                    )));
                }
            }
            return Ok(());
        }

        let next_is_index = tokens
            .get(i + 1)
            .map(|t| t.parse::<usize>().is_ok())
            .unwrap_or(false);

        match current {
            Value::Object(map) => {
                let entry = map.entry(token.clone()).or_insert_with(|| {
                    if next_is_index {
                        Value::Array(vec![])
                    } else {
                        Value::Object(Map::new())
                    }
                });
                current = entry;
            }
            Value::Array(arr) => {
                let idx: usize = token.parse().map_err(|_| {
                    JpathError::InvalidPointer(format!("invalid array index {token:?}"))
                })?;
                while arr.len() <= idx {
                    arr.push(if next_is_index {
                        Value::Array(vec![])
                    } else {
                        Value::Object(Map::new())
                    });
                }
                current = &mut arr[idx];
            }
            _ => {
                return Err(JpathError::TypeMismatch(format!(
                    "cannot create path at {pointer:?}"
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn escape_unescape_roundtrip() {
        assert_eq!(escape("a/b~c"), "a~1b~0c");
        assert_eq!(unescape("a~1b~0c"), "a/b~c");
    }

    #[test]
    fn get_and_set() {
        let doc = json!({"a": {"b": [1, 2]}});
        assert_eq!(get(&doc, "/a/b/0").unwrap(), json!(1));
        let updated = set(&doc, "/a/b/0", json!(99)).unwrap();
        assert_eq!(updated, json!({"a": {"b": [99, 2]}}));
    }

    #[test]
    fn exists_and_remove() {
        let doc = json!({"x": 1, "y": [0, 1]});
        assert!(exists(&doc, "/x").unwrap());
        assert!(!exists(&doc, "/z").unwrap());
        let out = remove(&doc, "/y/0").unwrap();
        assert_eq!(out, json!({"x": 1, "y": [1]}));
    }

    #[test]
    fn join_and_parent() {
        assert_eq!(join("/a", "b/c").unwrap(), "/a/b~1c");
        assert_eq!(parent("/a/b/c").unwrap(), "/a/b");
        assert_eq!(parent("/a").unwrap(), "");
    }
}
