//! Engine-specific search / CRUD operations.

use crate::client::{Client, Engine};
use crate::error::{SearchError, SearchResult};
use crate::http::{execute, HttpResponse, RequestOpts};
use crate::query::{es_bulk_ndjson, es_query, BulkOp, EsQueryOpts};
use niao_json_core::{to_string as json_to_string, Object, Value as JsonValue};

#[derive(Debug, Clone, Default)]
pub struct SearchOpts {
    pub index: String,
    pub q: Option<String>,
    pub query_by: Option<String>,
    pub filter: Option<String>,
    pub sort: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub body: Option<String>,
    pub fields: Vec<String>,
}

pub fn search(client: &Client, opts: &SearchOpts) -> SearchResult<HttpResponse> {
    if opts.index.is_empty() {
        return Err(SearchError::Config(
            "search requires index/collection name".into(),
        ));
    }
    match client.engine {
        Engine::Elasticsearch | Engine::OpenSearch => {
            let body = if let Some(b) = &opts.body {
                b.clone()
            } else {
                es_query(&EsQueryOpts {
                    q: opts.q.clone(),
                    fields: opts.fields.clone(),
                    size: opts.limit,
                    from: opts.offset,
                    sort_json: opts.sort.clone(),
                    match_all: opts.q.is_none(),
                    ..Default::default()
                })
            };
            let path = format!("/{}/_search", trim_name(&opts.index));
            execute(
                client,
                "POST",
                &path,
                &RequestOpts {
                    body: Some(body),
                    ..Default::default()
                },
            )
        }
        Engine::Meilisearch => {
            let mut obj = Object::new();
            let q = opts.q.clone().unwrap_or_default();
            obj.insert("q".into(), JsonValue::String(q));
            if let Some(n) = opts.limit {
                obj.insert("limit".into(), JsonValue::int(n));
            }
            if let Some(n) = opts.offset {
                obj.insert("offset".into(), JsonValue::int(n));
            }
            if let Some(f) = &opts.filter {
                obj.insert("filter".into(), JsonValue::String(f.clone()));
            }
            if let Some(s) = &opts.sort {
                // accept JSON array string or comma-separated
                if let Ok(v) = niao_json_core::parse(s) {
                    obj.insert("sort".into(), v);
                } else {
                    let arr: Vec<JsonValue> = s
                        .split(',')
                        .map(|p| JsonValue::String(p.trim().to_string()))
                        .collect();
                    obj.insert("sort".into(), JsonValue::Array(arr));
                }
            }
            let body = if let Some(b) = &opts.body {
                b.clone()
            } else {
                json_to_string(&JsonValue::Object(obj))
            };
            let path = format!("/indexes/{}/search", trim_name(&opts.index));
            execute(
                client,
                "POST",
                &path,
                &RequestOpts {
                    body: Some(body),
                    ..Default::default()
                },
            )
        }
        Engine::Typesense => {
            if let Some(b) = &opts.body {
                let path = format!("/collections/{}/documents/search", trim_name(&opts.index));
                return execute(
                    client,
                    "POST",
                    &path,
                    &RequestOpts {
                        body: Some(b.clone()),
                        ..Default::default()
                    },
                );
            }
            let q = opts.q.clone().unwrap_or_else(|| "*".into());
            let query_by = opts
                .query_by
                .clone()
                .or_else(|| {
                    if opts.fields.is_empty() {
                        None
                    } else {
                        Some(opts.fields.join(","))
                    }
                })
                .unwrap_or_else(|| "*".into());
            let mut params = vec![("q".into(), q), ("query_by".into(), query_by)];
            if let Some(n) = opts.limit {
                params.push(("per_page".into(), n.to_string()));
            }
            if let Some(n) = opts.offset {
                // Typesense uses page; approximate with offset/limit
                let per = opts.limit.unwrap_or(10).max(1);
                let page = (n / per) + 1;
                params.push(("page".into(), page.to_string()));
            }
            if let Some(f) = &opts.filter {
                params.push(("filter_by".into(), f.clone()));
            }
            if let Some(s) = &opts.sort {
                params.push(("sort_by".into(), s.clone()));
            }
            let path = format!("/collections/{}/documents/search", trim_name(&opts.index));
            execute(
                client,
                "GET",
                &path,
                &RequestOpts {
                    body: None,
                    content_type: None,
                    params,
                    ..Default::default()
                },
            )
        }
    }
}

pub fn index_doc(
    client: &Client,
    index: &str,
    doc_json: &str,
    id: Option<&str>,
) -> SearchResult<HttpResponse> {
    match client.engine {
        Engine::Elasticsearch | Engine::OpenSearch => {
            let path = if let Some(id) = id {
                format!("/{}/_doc/{}", trim_name(index), id)
            } else {
                format!("/{}/_doc", trim_name(index))
            };
            execute(
                client,
                "POST",
                &path,
                &RequestOpts {
                    body: Some(doc_json.to_string()),
                    ..Default::default()
                },
            )
        }
        Engine::Meilisearch => {
            let path = format!("/indexes/{}/documents", trim_name(index));
            // Meili accepts array of docs
            let body = if doc_json.trim_start().starts_with('[') {
                doc_json.to_string()
            } else {
                format!("[{}]", doc_json.trim())
            };
            execute(
                client,
                "POST",
                &path,
                &RequestOpts {
                    body: Some(body),
                    ..Default::default()
                },
            )
        }
        Engine::Typesense => {
            let path = format!("/collections/{}/documents", trim_name(index));
            execute(
                client,
                "POST",
                &path,
                &RequestOpts {
                    body: Some(doc_json.to_string()),
                    ..Default::default()
                },
            )
        }
    }
}

pub fn get_doc(client: &Client, index: &str, id: &str) -> SearchResult<HttpResponse> {
    match client.engine {
        Engine::Elasticsearch | Engine::OpenSearch => {
            let path = format!("/{}/_doc/{}", trim_name(index), id);
            execute(client, "GET", &path, &RequestOpts::default())
        }
        Engine::Meilisearch => {
            let path = format!("/indexes/{}/documents/{}", trim_name(index), id);
            execute(client, "GET", &path, &RequestOpts::default())
        }
        Engine::Typesense => {
            let path = format!("/collections/{}/documents/{}", trim_name(index), id);
            execute(client, "GET", &path, &RequestOpts::default())
        }
    }
}

pub fn delete_doc(client: &Client, index: &str, id: &str) -> SearchResult<HttpResponse> {
    match client.engine {
        Engine::Elasticsearch | Engine::OpenSearch => {
            let path = format!("/{}/_doc/{}", trim_name(index), id);
            execute(client, "DELETE", &path, &RequestOpts::default())
        }
        Engine::Meilisearch => {
            let path = format!("/indexes/{}/documents/{}", trim_name(index), id);
            execute(client, "DELETE", &path, &RequestOpts::default())
        }
        Engine::Typesense => {
            let path = format!("/collections/{}/documents/{}", trim_name(index), id);
            execute(client, "DELETE", &path, &RequestOpts::default())
        }
    }
}

pub fn update_doc(
    client: &Client,
    index: &str,
    id: &str,
    doc_json: &str,
) -> SearchResult<HttpResponse> {
    match client.engine {
        Engine::Elasticsearch | Engine::OpenSearch => {
            let path = format!("/{}/_update/{}", trim_name(index), id);
            let mut wrap = Object::new();
            let parsed =
                niao_json_core::parse(doc_json).map_err(|e| SearchError::Json(e.to_string()))?;
            wrap.insert("doc".into(), parsed);
            execute(
                client,
                "POST",
                &path,
                &RequestOpts {
                    body: Some(json_to_string(&JsonValue::Object(wrap))),
                    ..Default::default()
                },
            )
        }
        Engine::Meilisearch => {
            // Meili upsert via documents endpoint
            index_doc(client, index, doc_json, Some(id))
        }
        Engine::Typesense => {
            let path = format!("/collections/{}/documents/{}", trim_name(index), id);
            execute(
                client,
                "PATCH",
                &path,
                &RequestOpts {
                    body: Some(doc_json.to_string()),
                    ..Default::default()
                },
            )
        }
    }
}

pub fn bulk(client: &Client, ops: &[BulkOp]) -> SearchResult<HttpResponse> {
    match client.engine {
        Engine::Elasticsearch | Engine::OpenSearch => {
            let body = es_bulk_ndjson(ops)?;
            execute(
                client,
                "POST",
                "/_bulk",
                &RequestOpts {
                    body: Some(body),
                    content_type: Some("application/x-ndjson".into()),
                    ..Default::default()
                },
            )
        }
        Engine::Meilisearch => {
            // Group by index — send documents arrays
            if ops.is_empty() {
                return Err(SearchError::Config("bulk ops must not be empty".into()));
            }
            let index = &ops[0].index;
            let mut docs = Vec::new();
            for op in ops {
                if op.index != *index {
                    return Err(SearchError::Config(
                        "meilisearch bulk requires a single index per call".into(),
                    ));
                }
                let doc = op
                    .doc_json
                    .as_ref()
                    .ok_or_else(|| SearchError::Config("meili bulk needs doc".into()))?;
                let v = niao_json_core::parse(doc).map_err(|e| SearchError::Json(e.to_string()))?;
                docs.push(v);
            }
            let body = json_to_string(&JsonValue::Array(docs));
            index_doc(client, index, &body, None)
        }
        Engine::Typesense => {
            if ops.is_empty() {
                return Err(SearchError::Config("bulk ops must not be empty".into()));
            }
            let index = &ops[0].index;
            let mut lines = String::new();
            for op in ops {
                if op.index != *index {
                    return Err(SearchError::Config(
                        "typesense bulk requires a single collection per call".into(),
                    ));
                }
                let doc = op
                    .doc_json
                    .as_ref()
                    .ok_or_else(|| SearchError::Config("typesense bulk needs doc".into()))?;
                lines.push_str(doc.trim());
                lines.push('\n');
            }
            let path = format!(
                "/collections/{}/documents/import?action=upsert",
                trim_name(index)
            );
            execute(
                client,
                "POST",
                &path,
                &RequestOpts {
                    body: Some(lines),
                    content_type: Some("text/plain".into()),
                    ..Default::default()
                },
            )
        }
    }
}

pub fn create_index(
    client: &Client,
    name: &str,
    settings_json: Option<&str>,
) -> SearchResult<HttpResponse> {
    match client.engine {
        Engine::Elasticsearch | Engine::OpenSearch => {
            let path = format!("/{}", trim_name(name));
            execute(
                client,
                "PUT",
                &path,
                &RequestOpts {
                    body: Some(settings_json.unwrap_or("{}").to_string()),
                    ..Default::default()
                },
            )
        }
        Engine::Meilisearch => {
            let mut obj = Object::new();
            obj.insert("uid".into(), JsonValue::String(trim_name(name).to_string()));
            if let Some(s) = settings_json {
                if let Ok(JsonValue::Object(extra)) = niao_json_core::parse(s) {
                    for (k, v) in extra.iter() {
                        obj.insert(k.to_string(), v.clone());
                    }
                }
            }
            execute(
                client,
                "POST",
                "/indexes",
                &RequestOpts {
                    body: Some(json_to_string(&JsonValue::Object(obj))),
                    ..Default::default()
                },
            )
        }
        Engine::Typesense => {
            let body = if let Some(s) = settings_json {
                s.to_string()
            } else {
                let mut obj = Object::new();
                obj.insert(
                    "name".into(),
                    JsonValue::String(trim_name(name).to_string()),
                );
                obj.insert(
                    "fields".into(),
                    JsonValue::Array(vec![json_obj(&[
                        ("name", JsonValue::String("id".into())),
                        ("type", JsonValue::String("string".into())),
                    ])]),
                );
                json_to_string(&JsonValue::Object(obj))
            };
            execute(
                client,
                "POST",
                "/collections",
                &RequestOpts {
                    body: Some(body),
                    ..Default::default()
                },
            )
        }
    }
}

pub fn delete_index(client: &Client, name: &str) -> SearchResult<HttpResponse> {
    match client.engine {
        Engine::Elasticsearch | Engine::OpenSearch => {
            let path = format!("/{}", trim_name(name));
            execute(client, "DELETE", &path, &RequestOpts::default())
        }
        Engine::Meilisearch => {
            let path = format!("/indexes/{}", trim_name(name));
            execute(client, "DELETE", &path, &RequestOpts::default())
        }
        Engine::Typesense => {
            let path = format!("/collections/{}", trim_name(name));
            execute(client, "DELETE", &path, &RequestOpts::default())
        }
    }
}

pub fn list_indexes(client: &Client) -> SearchResult<HttpResponse> {
    match client.engine {
        Engine::Elasticsearch | Engine::OpenSearch => execute(
            client,
            "GET",
            "/_cat/indices?format=json",
            &RequestOpts::default(),
        ),
        Engine::Meilisearch => execute(client, "GET", "/indexes", &RequestOpts::default()),
        Engine::Typesense => execute(client, "GET", "/collections", &RequestOpts::default()),
    }
}

pub fn index_exists(client: &Client, name: &str) -> SearchResult<bool> {
    let resp = match client.engine {
        Engine::Elasticsearch | Engine::OpenSearch => execute(
            client,
            "HEAD",
            &format!("/{}", trim_name(name)),
            &RequestOpts {
                content_type: None,
                ..Default::default()
            },
        )?,
        Engine::Meilisearch => execute(
            client,
            "GET",
            &format!("/indexes/{}", trim_name(name)),
            &RequestOpts::default(),
        )?,
        Engine::Typesense => execute(
            client,
            "GET",
            &format!("/collections/{}", trim_name(name)),
            &RequestOpts::default(),
        )?,
    };
    Ok(resp.status == 200)
}

pub fn raw_request(
    client: &Client,
    method: &str,
    path: &str,
    body: Option<String>,
    params: Vec<(String, String)>,
) -> SearchResult<HttpResponse> {
    execute(
        client,
        method,
        path,
        &RequestOpts {
            body,
            params,
            content_type: Some("application/json".into()),
            ..Default::default()
        },
    )
}

fn trim_name(name: &str) -> &str {
    name.trim().trim_matches('/')
}

fn json_obj(pairs: &[(&str, JsonValue)]) -> JsonValue {
    let mut m = Object::new();
    for (k, v) in pairs {
        m.insert((*k).to_string(), v.clone());
    }
    JsonValue::Object(m)
}
