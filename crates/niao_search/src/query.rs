//! URL helpers and query/filter builders (offline-testable hot paths).

use crate::error::{SearchError, SearchResult};
use niao_http::{form_urlencode, join as http_join, parse_url};
use niao_json_core::{to_string as json_to_string, Number, Value as JsonValue};
use std::collections::BTreeMap;

/// Join base URL + relative path.
pub fn join_url(base: &str, path: &str) -> SearchResult<String> {
    if path.is_empty() {
        return Ok(base.to_string());
    }
    if path.contains("://") {
        return Ok(path.to_string());
    }
    if base.is_empty() {
        return Ok(path.to_string());
    }
    let base_url = parse_url(base).map_err(|e| SearchError::Url(e.to_string()))?;
    let joined = http_join(&base_url, path).map_err(|e| SearchError::Url(e.to_string()))?;
    Ok(joined.to_string_full())
}

/// Build URL with query parameters (stable key order).
pub fn prepare_url(
    base: &str,
    path: Option<&str>,
    params: &[(String, String)],
) -> SearchResult<String> {
    let mut url = match path {
        Some(p) if !p.is_empty() => join_url(base, p)?,
        _ => base.to_string(),
    };
    if params.is_empty() {
        return Ok(url);
    }
    let mut map = BTreeMap::new();
    for (k, v) in params {
        map.insert(k.clone(), v.clone());
    }
    let qs = map
        .iter()
        .map(|(k, v)| {
            format!(
                "{}={}",
                form_urlencode(k.as_bytes()),
                form_urlencode(v.as_bytes())
            )
        })
        .collect::<Vec<_>>()
        .join("&");
    if url.contains('?') {
        if !url.ends_with('?') && !url.ends_with('&') {
            url.push('&');
        }
        url.push_str(&qs);
    } else {
        url.push('?');
        url.push_str(&qs);
    }
    Ok(url)
}

/// Encode a parameter map to a query string.
pub fn encode_params(pairs: &[(String, String)]) -> String {
    if pairs.is_empty() {
        return String::new();
    }
    let mut map = BTreeMap::new();
    for (k, v) in pairs {
        map.insert(k.clone(), v.clone());
    }
    map.iter()
        .map(|(k, v)| {
            format!(
                "{}={}",
                form_urlencode(k.as_bytes()),
                form_urlencode(v.as_bytes())
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// Build Elasticsearch / OpenSearch query DSL JSON from a compact opts map.
///
/// Supported keys (string values unless noted):
/// - `q` / `query`: multi_match query string across fields (default `*`)
/// - `match`: object of field → text (merged as bool must match clauses)
/// - `term`: object of field → value
/// - `match_all`: bool — force `{"match_all":{}}`
/// - `size` / `from`: integers encoded as JSON numbers in the string map via `size`/`from` keys
/// - `sort`: raw JSON string or already-built array string
/// - `body`: if set, returned as-is (escape hatch)
pub fn es_query(opts: &EsQueryOpts) -> String {
    if let Some(raw) = &opts.body {
        return raw.clone();
    }
    if opts.match_all
        || (opts.q.is_none() && opts.match_fields.is_empty() && opts.term_fields.is_empty())
    {
        let mut root = JsonValue::Object(Default::default());
        if let JsonValue::Object(ref mut m) = root {
            m.insert(
                "query".into(),
                json_obj([("match_all", JsonValue::Object(Default::default()))]),
            );
            if let Some(n) = opts.size {
                m.insert("size".into(), JsonValue::Number(Number::I64(n)));
            }
            if let Some(n) = opts.from {
                m.insert("from".into(), JsonValue::Number(Number::I64(n)));
            }
            if let Some(s) = &opts.sort_json {
                if let Ok(v) = niao_json_core::parse(s) {
                    m.insert("sort".into(), v);
                }
            }
        }
        return json_to_string(&root);
    }

    let mut must = Vec::new();
    if let Some(q) = &opts.q {
        let fields = if opts.fields.is_empty() {
            vec![JsonValue::String("*".into())]
        } else {
            opts.fields
                .iter()
                .map(|f| JsonValue::String(f.clone()))
                .collect()
        };
        must.push(json_obj([(
            "multi_match",
            json_obj([
                ("query", JsonValue::String(q.clone())),
                ("fields", JsonValue::Array(fields)),
            ]),
        )]));
    }
    for (field, text) in &opts.match_fields {
        must.push(json_obj([(
            "match",
            json_obj([(field.as_str(), JsonValue::String(text.clone()))]),
        )]));
    }
    for (field, value) in &opts.term_fields {
        must.push(json_obj([(
            "term",
            json_obj([(field.as_str(), JsonValue::String(value.clone()))]),
        )]));
    }

    let query = if must.len() == 1 {
        must.pop().unwrap()
    } else {
        json_obj([("bool", json_obj([("must", JsonValue::Array(must))]))])
    };

    let mut root_map = niao_json_core::Object::new();
    root_map.insert("query".into(), query);
    if let Some(n) = opts.size {
        root_map.insert("size".into(), JsonValue::Number(Number::I64(n)));
    }
    if let Some(n) = opts.from {
        root_map.insert("from".into(), JsonValue::Number(Number::I64(n)));
    }
    if let Some(s) = &opts.sort_json {
        if let Ok(v) = niao_json_core::parse(s) {
            root_map.insert("sort".into(), v);
        }
    }
    json_to_string(&JsonValue::Object(root_map))
}

#[derive(Debug, Clone, Default)]
pub struct EsQueryOpts {
    pub q: Option<String>,
    pub fields: Vec<String>,
    pub match_fields: Vec<(String, String)>,
    pub term_fields: Vec<(String, String)>,
    pub match_all: bool,
    pub size: Option<i64>,
    pub from: Option<i64>,
    pub sort_json: Option<String>,
    pub body: Option<String>,
}

fn json_obj<'a, const N: usize>(pairs: [(&'a str, JsonValue); N]) -> JsonValue {
    let mut m = niao_json_core::Object::new();
    for (k, v) in pairs {
        m.insert(k.to_string(), v);
    }
    JsonValue::Object(m)
}

/// Build Elasticsearch bulk NDJSON body from operation descriptors.
#[derive(Debug, Clone)]
pub struct BulkOp {
    pub action: String, // index | create | update | delete
    pub index: String,
    pub id: Option<String>,
    pub doc_json: Option<String>,
}

pub fn es_bulk_ndjson(ops: &[BulkOp]) -> SearchResult<String> {
    if ops.is_empty() {
        return Err(SearchError::Config("bulk ops must not be empty".into()));
    }
    let mut out = String::new();
    for op in ops {
        let action = op.action.to_ascii_lowercase();
        if !matches!(action.as_str(), "index" | "create" | "update" | "delete") {
            return Err(SearchError::Config(format!(
                "unsupported bulk action: {}",
                op.action
            )));
        }
        let mut meta = niao_json_core::Object::new();
        let mut inner = niao_json_core::Object::new();
        inner.insert("_index".into(), JsonValue::String(op.index.clone()));
        if let Some(id) = &op.id {
            inner.insert("_id".into(), JsonValue::String(id.clone()));
        }
        meta.insert(action.clone(), JsonValue::Object(inner));
        out.push_str(&json_to_string(&JsonValue::Object(meta)));
        out.push('\n');
        match action.as_str() {
            "delete" => {}
            "update" => {
                let doc = op
                    .doc_json
                    .as_ref()
                    .ok_or_else(|| SearchError::Config("update bulk op needs doc".into()))?;
                let mut wrap = niao_json_core::Object::new();
                let parsed =
                    niao_json_core::parse(doc).map_err(|e| SearchError::Json(e.to_string()))?;
                wrap.insert("doc".into(), parsed);
                out.push_str(&json_to_string(&JsonValue::Object(wrap)));
                out.push('\n');
            }
            _ => {
                let doc = op
                    .doc_json
                    .as_ref()
                    .ok_or_else(|| SearchError::Config("index/create bulk op needs doc".into()))?;
                out.push_str(doc.trim());
                out.push('\n');
            }
        }
    }
    Ok(out)
}

/// Join Meilisearch filter expressions with ` AND `.
pub fn meili_filter(parts: &[String]) -> String {
    parts
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" AND ")
}

/// Join Typesense `filter_by` clauses with ` && `.
pub fn ts_filter(parts: &[String]) -> String {
    parts
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" && ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_and_params() {
        let u = join_url("http://localhost:9200/", "_search").unwrap();
        assert!(u.contains("9200"));
        assert!(u.ends_with("_search") || u.contains("/_search"));
        let qs = encode_params(&[("q".into(), "a b".into()), ("page".into(), "1".into())]);
        assert!(qs.contains("q="));
        assert!(qs.contains("page=1"));
        assert!(encode_params(&[]).is_empty());
    }

    #[test]
    fn es_query_match_all_and_q() {
        let s = es_query(&EsQueryOpts {
            match_all: true,
            size: Some(10),
            ..Default::default()
        });
        assert!(s.contains("match_all"));
        assert!(s.contains("\"size\":10") || s.contains("\"size\": 10"));

        let s2 = es_query(&EsQueryOpts {
            q: Some("niao".into()),
            fields: vec!["title".into()],
            size: Some(5),
            ..Default::default()
        });
        assert!(s2.contains("multi_match"));
        assert!(s2.contains("niao"));
    }

    #[test]
    fn bulk_ndjson_round() {
        let body = es_bulk_ndjson(&[BulkOp {
            action: "index".into(),
            index: "docs".into(),
            id: Some("1".into()),
            doc_json: Some(r#"{"title":"hi"}"#.into()),
        }])
        .unwrap();
        assert!(body.contains(r#""_index":"docs""#) || body.contains("\"_index\": \"docs\""));
        assert!(body.contains(r#"{"title":"hi"}"#));
        assert!(es_bulk_ndjson(&[]).is_err());
    }

    #[test]
    fn filters() {
        assert_eq!(
            meili_filter(&["genre = action".into(), "year > 2000".into()]),
            "genre = action AND year > 2000"
        );
        assert_eq!(
            ts_filter(&["year:>2000".into(), "in_stock:true".into()]),
            "year:>2000 && in_stock:true"
        );
        assert_eq!(meili_filter(&[]), "");
    }
}
