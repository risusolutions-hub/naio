//! Client configuration and engine identity.

use crate::error::{SearchError, SearchResult};
use niao_codec::base64::{decode as b64_decode, Base64Config};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Elasticsearch,
    OpenSearch,
    Meilisearch,
    Typesense,
}

impl Engine {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Elasticsearch => "elasticsearch",
            Self::OpenSearch => "opensearch",
            Self::Meilisearch => "meilisearch",
            Self::Typesense => "typesense",
        }
    }

    pub fn is_es_family(self) -> bool {
        matches!(self, Self::Elasticsearch | Self::OpenSearch)
    }
}

#[derive(Debug, Clone)]
pub enum Auth {
    None,
    ApiKey(String),
    Basic { username: String, password: String },
    Bearer(String),
}

#[derive(Debug, Clone)]
pub struct Client {
    pub engine: Engine,
    pub base_url: String,
    pub auth: Auth,
    pub timeout_ms: u64,
    pub default_headers: HashMap<String, String>,
}

impl Client {
    pub fn new(engine: Engine, base_url: String, auth: Auth, timeout_ms: u64) -> Self {
        Self {
            engine,
            base_url: trim_slash(&base_url),
            auth,
            timeout_ms,
            default_headers: HashMap::new(),
        }
    }
}

fn trim_slash(s: &str) -> String {
    s.trim().trim_end_matches('/').to_string()
}

/// Resolve Elastic Cloud ID (`name:base64(host$es$id$kibana$id)`) to HTTPS URL.
pub fn resolve_cloud_id(cloud_id: &str) -> SearchResult<String> {
    let cloud_id = cloud_id.trim();
    let Some((_, encoded)) = cloud_id.split_once(':') else {
        return Err(SearchError::Config(
            "cloud_id must be 'name:base64…'".into(),
        ));
    };
    let bytes = b64_decode(encoded, Base64Config::STANDARD)
        .or_else(|_| b64_decode(encoded, Base64Config::STANDARD_NO_PAD))
        .map_err(|e| SearchError::Config(format!("cloud_id base64: {e}")))?;
    let decoded =
        String::from_utf8(bytes).map_err(|e| SearchError::Config(format!("cloud_id utf8: {e}")))?;
    let parts: Vec<&str> = decoded.split('$').collect();
    if parts.len() < 2 {
        return Err(SearchError::Config(
            "cloud_id payload must contain host$es_id…".into(),
        ));
    }
    let host = parts[0].trim();
    let es_id = parts[1].trim();
    if host.is_empty() || es_id.is_empty() {
        return Err(SearchError::Config("cloud_id host/es_id empty".into()));
    }
    Ok(format!("https://{es_id}.{host}"))
}

pub fn auth_headers(auth: &Auth) -> Vec<(String, String)> {
    match auth {
        Auth::None => Vec::new(),
        Auth::ApiKey(k) => vec![("authorization".into(), format!("ApiKey {k}"))],
        Auth::Basic { username, password } => {
            let token =
                niao_codec::base64::encode_standard(format!("{username}:{password}").as_bytes());
            vec![("authorization".into(), format!("Basic {token}"))]
        }
        Auth::Bearer(t) => vec![("authorization".into(), format!("Bearer {t}"))],
    }
}

/// Meilisearch uses `Authorization: Bearer <key>`.
pub fn meili_auth_headers(key: &str) -> Vec<(String, String)> {
    if key.is_empty() {
        Vec::new()
    } else {
        vec![("authorization".into(), format!("Bearer {key}"))]
    }
}

/// Typesense uses `X-TYPESENSE-API-KEY`.
pub fn typesense_auth_headers(key: &str) -> Vec<(String, String)> {
    if key.is_empty() {
        Vec::new()
    } else {
        vec![("x-typesense-api-key".into(), key.to_string())]
    }
}
