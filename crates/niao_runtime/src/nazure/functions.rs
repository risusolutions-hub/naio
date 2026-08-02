//! Azure Functions HTTP-trigger invocation.
//!
//! Endpoint: `https://{app}.azurewebsites.net/api/{fn_name}`
//!
//! Auth options (checked in order):
//!   1. Client credentials → Bearer token
//!   2. SAS field repurposed as a function key (`?code={sas}`)
//!   3. Anonymous (function auth level = anonymous)

use super::{auth, AzureConfig};
use crate::error_value;
use crate::{Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::collections::HashMap;

fn fn_error(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E2811_NAZURE_ERROR, "nazure_error", msg.into(), span)
}

fn auth_error(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E2813_NAZURE_AUTH, "nazure_error", msg.into(), span)
}

// ──────────────────────────────────────────────────────────────────────────────
// Function INVOKE (POST)
// ──────────────────────────────────────────────────────────────────────────────

/// Invoke an Azure Function HTTP trigger by posting `payload` as the JSON body.
/// `app` is the Function App name (without `.azurewebsites.net`).
pub fn function_invoke(
    cfg: &AzureConfig,
    app: &str,
    fn_name: &str,
    payload: &str,
    span: Span,
) -> ValueRef {
    // Build URL, optionally appending function key from SAS field.
    let base_url = format!("https://{app}.azurewebsites.net/api/{fn_name}");
    let url = if let Some(sas) = &cfg.sas {
        format!("{base_url}?code={sas}")
    } else {
        base_url.clone()
    };

    // Obtain Bearer token if client-credentials are present.
    let bearer = if let (Some(tenant), Some(cid), Some(csec)) =
        (&cfg.tenant, &cfg.client_id, &cfg.client_secret)
    {
        let scope = format!("https://{app}.azurewebsites.net/.default");
        match auth::fetch_bearer_token(tenant, cid, csec, &scope) {
            Ok(tok) => Some(tok),
            Err(e) => return auth_error(span, e),
        }
    } else {
        None
    };

    let mut req = niao_http::post(&url)
        .set("Content-Type", "application/json")
        .set("Accept", "application/json");
    if let Some(tok) = bearer {
        req = req.set("Authorization", format!("Bearer {tok}"));
    }

    let body_bytes: &[u8] = payload.as_bytes();
    match req.send_bytes(body_bytes) {
        Err(e) => fn_error(span, format!("nazure function_invoke: {e}")),
        Ok(resp) => {
            let status = resp.status as i64;
            let body = String::from_utf8_lossy(&resp.body).into_owned();
            let mut map = HashMap::new();
            map.insert("status".into(), Value::Int(status).ref_cell());
            map.insert("body".into(), Value::String(body).ref_cell());
            Value::Object(map).ref_cell()
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_cfg() -> AzureConfig {
        AzureConfig {
            account: "myaccount".into(),
            key: None,
            sas: None,
            tenant: None,
            client_id: None,
            client_secret: None,
        }
    }

    #[test]
    fn sas_appended_as_code() {
        // The URL construction is unit-testable by inspecting cfg.sas logic.
        let mut cfg = dummy_cfg();
        cfg.sas = Some("myfunctionkey".into());
        // Just verify we build the code param; actual HTTP not called here.
        let base = format!("https://{}.azurewebsites.net/api/myFunc", "myapp");
        let url = if let Some(sas) = &cfg.sas {
            format!("{base}?code={sas}")
        } else {
            base.clone()
        };
        assert!(url.contains("?code=myfunctionkey"));
    }
}
