//! naws SSM Parameter Store operations: get.

use super::{aws_error, get_config, ok_string, AwsResult};
use crate::{Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;

use super::sigv4::{now_amz, sign, SignInput};

/// `naws.ssm_get(config_id, name, decrypt?) → string`
///
/// Fetches a parameter from AWS Systems Manager Parameter Store.
/// `decrypt` (default `true`) decrypts `SecureString` parameters.
pub fn ssm_get(args: &[ValueRef], span: Span) -> AwsResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(crate::RuntimeError::at(
            span,
            codes::E2800_NAWS_ARITY,
            "naws_ssm_get() expects 2-3 arguments: config, name, decrypt?",
        ));
    }
    let config_id = super::int_arg(args, 0, "naws_ssm_get", span)?;
    let cfg = get_config(config_id, span)?;
    let name = super::str_arg(args, 1, "naws_ssm_get", span)?;
    let decrypt = if args.len() > 2 {
        match &*args[2].borrow() {
            crate::Value::Bool(b) => *b,
            _ => true,
        }
    } else {
        true
    };

    let host = format!("ssm.{}.amazonaws.com", cfg.region);
    let endpoint = format!("https://{}/", host);
    let decrypt_str = if decrypt { "true" } else { "false" };
    let body_str = format!(
        "{{\"Name\":\"{}\",\"WithDecryption\":{}}}",
        super::json_escape(&name),
        decrypt_str
    );
    let body_bytes = body_str.as_bytes();
    let (amz_dt, amz_d) = now_amz();

    let ct = "application/x-amz-json-1.1";
    let extra = [("content-type", ct), ("x-amz-target", "AmazonSSM.GetParameter")];
    let inp = SignInput {
        method: "POST",
        host: &host,
        path: "/",
        query: "",
        region: &cfg.region,
        service: "ssm",
        access_key: &cfg.access_key,
        secret_key: &cfg.secret_key,
        session_token: cfg.session_token.as_deref(),
        body: body_bytes,
        amz_datetime: &amz_dt,
        amz_date: &amz_d,
        extra_headers: &extra,
    };
    let signed = sign(&inp);

    let mut builder = niao_http::post(&endpoint);
    for (k, v) in &signed.headers {
        builder = builder.set(k.clone(), v.clone());
    }
    builder = builder.set("Content-Type", ct);
    builder = builder.set("X-Amz-Target", "AmazonSSM.GetParameter");

    match builder.send_string(&body_str) {
        Ok(resp) => {
            let status = resp.status as i64;
            let body_out = String::from_utf8_lossy(&resp.body).into_owned();
            if status >= 400 {
                return Ok(aws_error(
                    codes::E2801_NAWS_ERROR,
                    "naws_ssm_error",
                    body_out,
                    span,
                ));
            }
            // Parse: {"Parameter": {"Value": "..."}}
            match serde_json::from_str::<serde_json::Value>(&body_out) {
                Ok(json) => {
                    let val = json
                        .get("Parameter")
                        .and_then(|p| p.get("Value"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    Ok(ok_string(val))
                }
                Err(e) => Ok(aws_error(
                    codes::E2801_NAWS_ERROR,
                    "naws_ssm_error",
                    format!("JSON parse error: {e}"),
                    span,
                )),
            }
        }
        Err(e) => Ok(aws_error(
            codes::E2801_NAWS_ERROR,
            "naws_ssm_error",
            e.to_string(),
            span,
        )),
    }
}
