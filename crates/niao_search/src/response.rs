//! Normalize search hits across engines.

use niao_json_core::{parse, Value as JsonValue};

/// Extract a uniform list of hit documents from an engine response body.
///
/// - Elasticsearch/OpenSearch: `hits.hits[*]._source` (fallback whole hit)
/// - Meilisearch: `hits` array
/// - Typesense: `hits[*].document` (fallback whole hit)
pub fn extract_hits(engine: &str, body: &str) -> Result<Vec<JsonValue>, String> {
    let v = parse(body).map_err(|e| e.to_string())?;
    match engine {
        "elasticsearch" | "opensearch" => {
            let hits = v
                .get("hits")
                .and_then(|h| h.get("hits"))
                .and_then(|a| a.as_array())
                .ok_or_else(|| "missing hits.hits".to_string())?;
            let mut out = Vec::with_capacity(hits.len());
            for hit in hits {
                if let Some(src) = hit.get("_source") {
                    out.push(src.clone());
                } else {
                    out.push(hit.clone());
                }
            }
            Ok(out)
        }
        "meilisearch" => {
            let hits = v
                .get("hits")
                .and_then(|a| a.as_array())
                .ok_or_else(|| "missing hits".to_string())?;
            Ok(hits.to_vec())
        }
        "typesense" => {
            let hits = v
                .get("hits")
                .and_then(|a| a.as_array())
                .ok_or_else(|| "missing hits".to_string())?;
            let mut out = Vec::with_capacity(hits.len());
            for hit in hits {
                if let Some(doc) = hit.get("document") {
                    out.push(doc.clone());
                } else {
                    out.push(hit.clone());
                }
            }
            Ok(out)
        }
        other => Err(format!("unknown engine for hits(): {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn es_hits() {
        let body = r#"{"hits":{"hits":[{"_id":"1","_source":{"title":"a"}},{"_id":"2","_source":{"title":"b"}}]}}"#;
        let hits = extract_hits("elasticsearch", body).unwrap();
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn meili_hits() {
        let body = r#"{"hits":[{"id":1},{"id":2}],"estimatedTotalHits":2}"#;
        let hits = extract_hits("meilisearch", body).unwrap();
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn ts_hits() {
        let body = r#"{"hits":[{"document":{"id":"1"}},{"document":{"id":"2"}}]}"#;
        let hits = extract_hits("typesense", body).unwrap();
        assert_eq!(hits.len(), 2);
    }
}
