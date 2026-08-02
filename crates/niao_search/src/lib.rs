//! Hosted search-engine clients for Niao (~elasticsearch, meilisearch).
//!
//! Elasticsearch/OpenSearch, Meilisearch, and Typesense over `niao_http`.

mod client;
mod error;
mod http;
mod ops;
mod query;
mod response;

pub use client::{resolve_cloud_id, Auth, Client, Engine};
pub use error::{SearchError, SearchResult};
pub use http::{execute, HttpResponse, RequestOpts};
pub use ops::{
    bulk, create_index, delete_doc, delete_index, get_doc, index_doc, index_exists, list_indexes,
    raw_request, search, update_doc, SearchOpts,
};
pub use query::{
    encode_params, es_bulk_ndjson, es_query, join_url, meili_filter, prepare_url, ts_filter,
    BulkOp, EsQueryOpts,
};
pub use response::extract_hits;

/// Build a client from common option fields.
pub fn build_client(
    engine: Engine,
    url: Option<String>,
    cloud_id: Option<String>,
    api_key: Option<String>,
    username: Option<String>,
    password: Option<String>,
    bearer: Option<String>,
    key: Option<String>,
    timeout_ms: Option<u64>,
) -> SearchResult<Client> {
    let base = if let Some(cid) = cloud_id {
        resolve_cloud_id(&cid)?
    } else {
        url.ok_or_else(|| SearchError::Config("url (or cloud_id) is required".into()))?
    };
    let auth = match engine {
        Engine::Meilisearch => {
            let k = key.or(api_key).or(bearer).unwrap_or_default();
            if k.is_empty() {
                Auth::None
            } else {
                Auth::Bearer(k)
            }
        }
        Engine::Typesense => {
            let k = api_key.or(key).or(bearer).unwrap_or_default();
            if k.is_empty() {
                Auth::None
            } else {
                Auth::ApiKey(k)
            }
        }
        Engine::Elasticsearch | Engine::OpenSearch => {
            if let Some(k) = api_key {
                Auth::ApiKey(k)
            } else if let Some(t) = bearer {
                Auth::Bearer(t)
            } else if let (Some(u), Some(p)) = (username, password) {
                Auth::Basic {
                    username: u,
                    password: p,
                }
            } else {
                Auth::None
            }
        }
    };
    Ok(Client::new(
        engine,
        base,
        auth,
        timeout_ms.unwrap_or(30_000),
    ))
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use niao_http::{OutgoingResponse, Server};
    use std::thread;

    #[test]
    fn mock_es_search_roundtrip() {
        let server = Server::http("127.0.0.1:0").expect("bind");
        let addr = server.local_addr().unwrap();
        let handle = thread::spawn(move || {
            if let Ok(req) = server.recv() {
                assert!(req.url().contains("_search"));
                let body = r#"{"hits":{"hits":[{"_source":{"title":"Niao"}}]}}"#;
                let _ = req.respond(OutgoingResponse::from_string(body).with_status(200));
            }
        });
        thread::sleep(std::time::Duration::from_millis(20));
        let client = Client::new(
            Engine::Elasticsearch,
            format!("http://{addr}"),
            Auth::None,
            5_000,
        );
        let resp = search(
            &client,
            &SearchOpts {
                index: "docs".into(),
                q: Some("Niao".into()),
                limit: Some(10),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(resp.ok);
        let hits = extract_hits("elasticsearch", &resp.body).unwrap();
        assert_eq!(hits.len(), 1);
        let _ = handle.join();
    }
}
