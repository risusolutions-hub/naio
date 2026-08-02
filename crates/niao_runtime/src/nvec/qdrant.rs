//! Qdrant REST API backend for nvec.
//!
//! Uses `niao_http` (already a dependency of `niao_runtime`) for all HTTP
//! calls. No additional third-party crates required.
//!
//! Qdrant REST reference: <https://qdrant.tech/documentation/interfaces/>
//!
//! Supported operations:
//! - ensure_collection   — idempotent create if not exists
//! - upsert_point        — PUT /collections/{col}/points (upsert=true)
//! - search              — POST /collections/{col}/points/search
//! - delete_point        — POST /collections/{col}/points/delete (by ids filter)
//! - count               — POST /collections/{col}/points/count
//!
//! All JSON is built manually (no serde) to maintain the zero-third-party-crate
//! constraint.

use crate::nvec::index::{MetaVal, SearchHit};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Qdrant backend handle
// ---------------------------------------------------------------------------

pub struct QdrantBackend {
    /// Base URL, e.g. `http://localhost:6333`
    pub base_url: String,
    /// Optional API key (x-api-key header).
    pub api_key: Option<String>,
    /// Collection name to operate on.
    pub collection: String,
    /// Vector dimension (0 = not yet known, set on first upsert).
    pub dim: usize,
}

impl QdrantBackend {
    pub fn new(base_url: String, api_key: Option<String>, collection: String) -> Self {
        QdrantBackend {
            base_url,
            api_key,
            collection,
            dim: 0,
        }
    }

    // -----------------------------------------------------------------------
    // Internal HTTP helpers
    // -----------------------------------------------------------------------

    fn build_url(&self, path: &str) -> String {
        format!("{}{}", self.base_url.trim_end_matches('/'), path)
    }

    fn apply_key(&self, rb: niao_http::RequestBuilder) -> niao_http::RequestBuilder {
        if let Some(ref key) = self.api_key {
            rb.header("api-key", key).header("x-api-key", key)
        } else {
            rb
        }
    }

    fn http_get(&self, path: &str) -> Result<(u16, String), String> {
        let url = self.build_url(path);
        let rb = self.apply_key(niao_http::get(&url));
        let resp = rb
            .header("content-type", "application/json")
            .send()
            .map_err(|e| format!("HTTP GET {url}: {e}"))?;
        let body = String::from_utf8_lossy(&resp.body).into_owned();
        Ok((resp.status, body))
    }

    fn http_put(&self, path: &str, body: &str) -> Result<(u16, String), String> {
        let url = self.build_url(path);
        let rb = self.apply_key(niao_http::put(&url));
        let resp = rb
            .header("content-type", "application/json")
            .send_string(body)
            .map_err(|e| format!("HTTP PUT {url}: {e}"))?;
        let rbody = String::from_utf8_lossy(&resp.body).into_owned();
        Ok((resp.status, rbody))
    }

    fn http_post(&self, path: &str, body: &str) -> Result<(u16, String), String> {
        let url = self.build_url(path);
        let rb = self.apply_key(niao_http::post(&url));
        let resp = rb
            .header("content-type", "application/json")
            .send_string(body)
            .map_err(|e| format!("HTTP POST {url}: {e}"))?;
        let rbody = String::from_utf8_lossy(&resp.body).into_owned();
        Ok((resp.status, rbody))
    }

    fn http_delete(&self, path: &str) -> Result<(u16, String), String> {
        let url = self.build_url(path);
        let rb = self.apply_key(niao_http::delete(&url));
        let resp = rb
            .header("content-type", "application/json")
            .send()
            .map_err(|e| format!("HTTP DELETE {url}: {e}"))?;
        let rbody = String::from_utf8_lossy(&resp.body).into_owned();
        Ok((resp.status, rbody))
    }

    // -----------------------------------------------------------------------
    // Collection management
    // -----------------------------------------------------------------------

    /// Create the collection if it does not already exist.
    pub fn ensure_collection(&self, dim: usize) -> Result<(), String> {
        // Check if collection exists.
        let path = format!("/collections/{}", self.collection);
        match self.http_get(&path) {
            Ok((200, _)) => return Ok(()), // already exists
            Ok((404, _)) => {}             // need to create
            Ok((status, body)) => {
                return Err(format!("qdrant check collection status={status}: {body}"));
            }
            Err(e) => return Err(e),
        }

        // Create collection.
        let body = format!(r#"{{"vectors":{{"size":{dim},"distance":"Cosine"}}}}"#);
        let (status, resp_body) = self.http_put(&path, &body)?;
        if status == 200 || status == 201 {
            Ok(())
        } else {
            Err(format!(
                "qdrant create_collection status={status}: {resp_body}"
            ))
        }
    }

    // -----------------------------------------------------------------------
    // Upsert
    // -----------------------------------------------------------------------

    pub fn upsert(
        &mut self,
        id: &str,
        vector: &[f32],
        metadata: &HashMap<String, MetaVal>,
    ) -> Result<(), String> {
        if self.dim == 0 {
            self.dim = vector.len();
            self.ensure_collection(self.dim)?;
        } else if vector.len() != self.dim {
            return Err(format!(
                "qdrant dimension mismatch: collection expects {}, got {}",
                self.dim,
                vector.len()
            ));
        }

        let vec_json = floats_to_json_array(vector);
        let payload_json = meta_to_json_object(metadata);
        let point_id = qdrant_id_json(id);

        let body = format!(
            r#"{{"points":[{{"id":{point_id},"vector":{vec_json},"payload":{payload_json}}}]}}"#
        );
        let path = format!("/collections/{}/points?wait=true", self.collection);
        let (status, resp_body) = self.http_put(&path, &body)?;
        if status == 200 || status == 201 {
            Ok(())
        } else {
            Err(format!("qdrant upsert status={status}: {resp_body}"))
        }
    }

    // -----------------------------------------------------------------------
    // Search
    // -----------------------------------------------------------------------

    pub fn search(
        &self,
        query: &[f32],
        top_k: usize,
        threshold: f32,
    ) -> Result<Vec<SearchHit>, String> {
        let vec_json = floats_to_json_array(query);
        let body = format!(
            r#"{{"vector":{vec_json},"limit":{top_k},"score_threshold":{threshold},"with_payload":true}}"#
        );
        let path = format!("/collections/{}/points/search", self.collection);
        let (status, resp_body) = self.http_post(&path, &body)?;
        if status != 200 {
            return Err(format!("qdrant search status={status}: {resp_body}"));
        }
        parse_search_response(&resp_body)
    }

    // -----------------------------------------------------------------------
    // Delete
    // -----------------------------------------------------------------------

    pub fn delete(&self, id: &str) -> Result<bool, String> {
        let point_id = qdrant_id_json(id);
        let body = format!(r#"{{"points":[{point_id}]}}"#);
        let path = format!("/collections/{}/points/delete?wait=true", self.collection);
        let (status, resp_body) = self.http_post(&path, &body)?;
        if status == 200 {
            Ok(true)
        } else {
            Err(format!("qdrant delete status={status}: {resp_body}"))
        }
    }

    // -----------------------------------------------------------------------
    // Count
    // -----------------------------------------------------------------------

    pub fn count(&self) -> Result<usize, String> {
        let body = r#"{"exact":true}"#;
        let path = format!("/collections/{}/points/count", self.collection);
        let (status, resp_body) = self.http_post(&path, body)?;
        if status != 200 {
            return Err(format!("qdrant count status={status}: {resp_body}"));
        }
        // Parse `{"result":{"count":N},...}`
        parse_count_response(&resp_body)
    }

    // -----------------------------------------------------------------------
    // Info (collection exists + dim)
    // -----------------------------------------------------------------------

    /// Probe the collection and return its configured vector size, or 0 if
    /// the collection does not exist yet.
    pub fn probe_dim(&self) -> usize {
        let path = format!("/collections/{}", self.collection);
        if let Ok((200, body)) = self.http_get(&path) {
            extract_json_uint(&body, "size").unwrap_or(0) as usize
        } else {
            0
        }
    }
}

// ---------------------------------------------------------------------------
// JSON helpers (manual — no serde)
// ---------------------------------------------------------------------------

/// Build a JSON array from f32 slice: `[1.0,2.0,3.0]`
fn floats_to_json_array(v: &[f32]) -> String {
    let mut s = String::with_capacity(v.len() * 8 + 2);
    s.push('[');
    for (i, f) in v.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        // Use enough precision to round-trip f32.
        write_float32(&mut s, *f);
    }
    s.push(']');
    s
}

fn write_float32(s: &mut String, f: f32) {
    if f.is_nan() {
        s.push_str("null");
    } else if f.is_infinite() {
        s.push_str(if f > 0.0 { "1e38" } else { "-1e38" });
    } else {
        s.push_str(&format!("{f:.8}"));
        // Trim trailing zeros after decimal point for compactness.
        let trimmed = s.trim_end_matches('0');
        let len = if trimmed.ends_with('.') {
            trimmed.len() + 1
        } else {
            trimmed.len()
        };
        s.truncate(len);
    }
}

/// Build a JSON object from MetaVal metadata.
fn meta_to_json_object(meta: &HashMap<String, MetaVal>) -> String {
    let mut s = String::from("{");
    let mut first = true;
    for (k, v) in meta {
        if !first {
            s.push(',');
        }
        first = false;
        s.push_str(&json_string(k));
        s.push(':');
        s.push_str(&meta_val_to_json(v));
    }
    s.push('}');
    s
}

fn meta_val_to_json(v: &MetaVal) -> String {
    match v {
        MetaVal::Nil => "null".to_string(),
        MetaVal::Bool(b) => {
            if *b {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        MetaVal::Int(n) => n.to_string(),
        MetaVal::Float(f) => format!("{f}"),
        MetaVal::Str(s) => json_string(s),
    }
}

/// JSON-encode a Qdrant point ID. Qdrant supports integer or UUID string IDs.
/// We map the vec_id: if it parses as u64, send as integer; otherwise as string UUID/text.
fn qdrant_id_json(id: &str) -> String {
    if let Ok(n) = id.parse::<u64>() {
        n.to_string()
    } else {
        json_string(id)
    }
}

/// Naive JSON string encoder (ASCII-safe subset).
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// ---------------------------------------------------------------------------
// Response parsers (minimal, hand-rolled)
// ---------------------------------------------------------------------------

/// Parse Qdrant search response body into SearchHit list.
///
/// Expected shape:
/// ```json
/// {"result":[{"id":"...","score":0.9,"payload":{"k":"v",...}},...]}
/// ```
fn parse_search_response(body: &str) -> Result<Vec<SearchHit>, String> {
    // Find "result": [ ... ]
    let result_arr = extract_json_array_str(body, "result")
        .ok_or_else(|| format!("qdrant: cannot find 'result' array in: {body}"))?;

    let mut hits = Vec::new();
    // Split by top-level object boundaries (naive but sufficient for well-formed JSON).
    for obj_str in split_json_objects(result_arr) {
        let id = extract_json_str_or_uint(&obj_str, "id")?;
        let score = extract_json_f64(&obj_str, "score").unwrap_or(0.0) as f32;
        let metadata = if let Some(payload_str) = extract_json_object_str(&obj_str, "payload") {
            parse_payload_object(payload_str)
        } else {
            std::collections::HashMap::new()
        };
        hits.push(SearchHit {
            id,
            score,
            metadata,
        });
    }
    Ok(hits)
}

/// Parse `{"result":{"count":N},...}`.
fn parse_count_response(body: &str) -> Result<usize, String> {
    // Find "result":{ ... "count": N ...}
    let result_obj = extract_json_object_str(body, "result")
        .ok_or_else(|| format!("qdrant count: no 'result' object in: {body}"))?;
    let n = extract_json_uint(result_obj, "count")
        .ok_or_else(|| format!("qdrant count: no 'count' field in: {result_obj}"))?;
    Ok(n as usize)
}

/// Parse a flat payload object into MetaVal map.
fn parse_payload_object(obj: &str) -> HashMap<String, MetaVal> {
    let mut map = HashMap::new();
    // Iterate key-value pairs at the top level of the JSON object string.
    let inner = obj.trim().trim_start_matches('{').trim_end_matches('}');
    for pair in split_json_pairs(inner) {
        let (k, v) = match pair.split_once(':') {
            Some(p) => p,
            None => continue,
        };
        let key = k.trim().trim_matches('"').to_string();
        let val_str = v.trim();
        let val = if val_str == "null" {
            MetaVal::Nil
        } else if val_str == "true" {
            MetaVal::Bool(true)
        } else if val_str == "false" {
            MetaVal::Bool(false)
        } else if val_str.starts_with('"') {
            MetaVal::Str(
                val_str
                    .trim_matches('"')
                    .replace("\\\"", "\"")
                    .replace("\\\\", "\\")
                    .replace("\\n", "\n")
                    .replace("\\r", "\r")
                    .replace("\\t", "\t"),
            )
        } else if val_str.contains('.') {
            MetaVal::Float(val_str.parse::<f64>().unwrap_or(0.0))
        } else {
            MetaVal::Int(val_str.parse::<i64>().unwrap_or(0))
        };
        map.insert(key, val);
    }
    map
}

// ---------------------------------------------------------------------------
// Micro JSON extraction helpers
// ---------------------------------------------------------------------------

/// Extract the raw string value of a JSON string field.
/// Returns the content without surrounding quotes.
fn extract_json_str<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let pattern = format!("\"{key}\":");
    let start = json.find(&pattern)?;
    let after = json[start + pattern.len()..].trim_start();
    if !after.starts_with('"') {
        return None;
    }
    // Find closing quote (naive, assumes no escaped quotes in short IDs).
    let inner = &after[1..];
    let end = inner.find('"')?;
    Some(&inner[..end])
}

/// Extract string or u64 integer as a String (Qdrant IDs can be either).
fn extract_json_str_or_uint(json: &str, key: &str) -> Result<String, String> {
    if let Some(s) = extract_json_str(json, key) {
        return Ok(s.to_string());
    }
    // Try uint.
    if let Some(n) = extract_json_uint(json, key) {
        return Ok(n.to_string());
    }
    Err(format!(
        "qdrant: cannot extract id field '{key}' from: {json}"
    ))
}

fn extract_json_f64(json: &str, key: &str) -> Option<f64> {
    let pattern = format!("\"{key}\":");
    let start = json.find(&pattern)?;
    let after = json[start + pattern.len()..].trim_start();
    let end = after
        .find(|c: char| c == ',' || c == '}' || c == ']' || c.is_whitespace())
        .unwrap_or(after.len());
    after[..end].parse::<f64>().ok()
}

fn extract_json_uint(json: &str, key: &str) -> Option<u64> {
    let pattern = format!("\"{key}\":");
    let start = json.find(&pattern)?;
    let after = json[start + pattern.len()..].trim_start();
    let end = after
        .find(|c: char| c == ',' || c == '}' || c == ']' || c.is_whitespace())
        .unwrap_or(after.len());
    after[..end].parse::<u64>().ok()
}

/// Return the raw content of a JSON array field (without outer `[` `]`).
fn extract_json_array_str<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let pattern = format!("\"{key}\":");
    let start = json.find(&pattern)?;
    let after = &json[start + pattern.len()..];
    let bracket = after.find('[')?;
    let content = &after[bracket + 1..];
    let depth_end = find_bracket_close(content, '[', ']')?;
    Some(&content[..depth_end])
}

/// Return the raw content of a JSON object field (without outer `{` `}`).
fn extract_json_object_str<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let pattern = format!("\"{key}\":");
    let start = json.find(&pattern)?;
    let after = &json[start + pattern.len()..];
    let brace = after.find('{')?;
    let content = &after[brace + 1..];
    let depth_end = find_bracket_close(content, '{', '}')?;
    Some(&content[..depth_end])
}

/// Find the position of the matching closing bracket, respecting nesting.
fn find_bracket_close(s: &str, open: char, close: char) -> Option<usize> {
    let mut depth = 1i32;
    let mut in_str = false;
    let mut escape = false;
    for (i, c) in s.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if c == '\\' && in_str {
            escape = true;
            continue;
        }
        if c == '"' {
            in_str = !in_str;
            continue;
        }
        if in_str {
            continue;
        }
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

/// Split a JSON array body (no surrounding `[` `]`) into individual object strings.
fn split_json_objects(s: &str) -> Vec<String> {
    let mut results = Vec::new();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;
    let mut start = 0;
    let chars: Vec<char> = s.chars().collect();
    let mut byte_pos = 0usize;

    for &c in &chars {
        let c_len = c.len_utf8();
        if escape {
            escape = false;
            byte_pos += c_len;
            continue;
        }
        if c == '\\' && in_str {
            escape = true;
            byte_pos += c_len;
            continue;
        }
        if c == '"' {
            in_str = !in_str;
            byte_pos += c_len;
            continue;
        }
        if in_str {
            byte_pos += c_len;
            continue;
        }
        if c == '{' {
            if depth == 0 {
                start = byte_pos;
            }
            depth += 1;
        } else if c == '}' {
            depth -= 1;
            if depth == 0 {
                results.push(s[start..=byte_pos].to_string());
            }
        }
        byte_pos += c_len;
    }
    results
}

/// Split `key:value,...` pairs at the top level (depth 0).
fn split_json_pairs(s: &str) -> Vec<String> {
    let mut pairs = Vec::new();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;
    let mut start = 0;
    let bytes = s.as_bytes();

    for (i, &b) in bytes.iter().enumerate() {
        if escape {
            escape = false;
            continue;
        }
        if b == b'\\' && in_str {
            escape = true;
            continue;
        }
        if b == b'"' {
            in_str = !in_str;
            continue;
        }
        if in_str {
            continue;
        }
        match b {
            b'{' | b'[' => depth += 1,
            b'}' | b']' => depth -= 1,
            b',' if depth == 0 => {
                let slice = s[start..i].trim();
                if !slice.is_empty() {
                    pairs.push(slice.to_string());
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    let last = s[start..].trim();
    if !last.is_empty() {
        pairs.push(last.to_string());
    }
    pairs
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_to_json_roundtrip() {
        let v = vec![1.0f32, -0.5, 0.0, 3.14];
        let j = floats_to_json_array(&v);
        assert!(j.starts_with('['));
        assert!(j.ends_with(']'));
        assert!(j.contains("1.0") || j.contains("1."));
    }

    #[test]
    fn json_string_encoding() {
        let s = json_string("hello \"world\"");
        assert_eq!(s, r#""hello \"world\"""#);
    }

    #[test]
    fn meta_to_json_object_basic() {
        let mut meta = HashMap::new();
        meta.insert("label".into(), MetaVal::Str("cat".into()));
        meta.insert("n".into(), MetaVal::Int(42));
        let j = meta_to_json_object(&meta);
        assert!(j.starts_with('{'));
        assert!(j.ends_with('}'));
        assert!(j.contains("\"label\""));
        assert!(j.contains("\"cat\""));
        assert!(j.contains("\"n\""));
        assert!(j.contains("42"));
    }

    #[test]
    fn qdrant_id_numeric() {
        assert_eq!(qdrant_id_json("42"), "42");
    }

    #[test]
    fn qdrant_id_string() {
        assert_eq!(qdrant_id_json("my-vec-id"), "\"my-vec-id\"");
    }

    #[test]
    fn parse_count_response_basic() {
        let body = r#"{"result":{"count":17},"status":"ok","time":0.001}"#;
        assert_eq!(parse_count_response(body).unwrap(), 17);
    }

    #[test]
    fn parse_search_response_basic() {
        let body = r#"{"result":[{"id":"abc","score":0.95,"payload":{"label":"dog","n":3}}],"status":"ok","time":0.002}"#;
        let hits = parse_search_response(body).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "abc");
        assert!((hits[0].score - 0.95).abs() < 0.01);
        match hits[0].metadata.get("label") {
            Some(MetaVal::Str(s)) => assert_eq!(s, "dog"),
            _ => panic!("expected string label"),
        }
    }

    #[test]
    fn parse_search_response_numeric_id() {
        let body = r#"{"result":[{"id":7,"score":0.8,"payload":{}}],"status":"ok","time":0.001}"#;
        let hits = parse_search_response(body).unwrap();
        assert_eq!(hits[0].id, "7");
    }

    #[test]
    fn split_json_objects_two_hits() {
        let s = r#"{"id":"a","score":0.9},{"id":"b","score":0.8}"#;
        let objs = split_json_objects(s);
        assert_eq!(objs.len(), 2);
        assert!(objs[0].contains("\"a\""));
        assert!(objs[1].contains("\"b\""));
    }

    #[test]
    fn extract_json_f64_works() {
        let j = r#"{"score":0.95,"other":1}"#;
        assert!((extract_json_f64(j, "score").unwrap() - 0.95).abs() < 1e-9);
    }
}
