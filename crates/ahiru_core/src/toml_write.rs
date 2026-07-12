//! Minimal TOML writer for `AhiruConfig` scaffolding.

use crate::AhiruConfig;

pub fn config_to_toml(cfg: &AhiruConfig) -> String {
    let mut out = String::new();
    out.push_str("[server]\n");
    out.push_str(&format!("host = \"{}\"\n", cfg.server.host));
    out.push_str(&format!("port = {}\n", cfg.server.port));
    out.push_str(&format!("workers = {}\n", cfg.server.workers));
    out.push_str(&format!("body_limit_mb = {}\n", cfg.server.body_limit_mb));
    if let Some(v) = cfg.server.tls_cert.as_ref() {
        out.push_str(&format!("tls_cert = \"{v}\"\n"));
    }
    if let Some(v) = cfg.server.tls_key.as_ref() {
        out.push_str(&format!("tls_key = \"{v}\"\n"));
    }

    for db in &cfg.databases {
        out.push_str("\n[[databases]]\n");
        out.push_str(&format!("name = \"{}\"\n", db.name));
        out.push_str(&format!("driver = \"{}\"\n", db.driver));
        out.push_str(&format!("url = \"{}\"\n", db.url));
        out.push_str(&format!("pool_size = {}\n", db.pool_size));
    }

    for cache in &cfg.caches {
        out.push_str("\n[[caches]]\n");
        out.push_str(&format!("name = \"{}\"\n", cache.name));
        out.push_str(&format!("driver = \"{}\"\n", cache.driver));
        if let Some(url) = cache.url.as_ref() {
            out.push_str(&format!("url = \"{url}\"\n"));
        }
    }

    out.push_str("\n[auth]\n");
    out.push_str(&format!("mode = \"{}\"\n", cfg.auth.mode));
    if let Some(secret) = cfg.auth.jwt_secret.as_ref() {
        out.push_str(&format!("jwt_secret = \"{secret}\"\n"));
    }

    out.push_str("\n[websocket]\n");
    out.push_str(&format!("mode = \"{}\"\n", cfg.websocket.mode));

    out.push_str("\n[security]\n");
    if !cfg.security.cors_origins.is_empty() {
        let items: Vec<String> = cfg
            .security
            .cors_origins
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect();
        out.push_str(&format!("cors_origins = [{}]\n", items.join(", ")));
    }
    out.push_str(&format!(
        "rate_limit_rps = {}\n",
        cfg.security.rate_limit_rps
    ));
    out.push_str(&format!(
        "secure_headers = {}\n",
        cfg.security.secure_headers
    ));

    out.push_str("\n[logging]\n");
    out.push_str(&format!("level = \"{}\"\n", cfg.logging.level));
    out.push_str(&format!("request_id = {}\n", cfg.logging.request_id));
    out.push_str(&format!("json_logs = {}\n", cfg.logging.json_logs));

    out
}
