//! Native nsmtp standard library — ergonomic SMTP email sending via `lettre`.
//! Object-based config instead of positional `net_smtp_send` arguments.
//!
//! Import with `import "nsmtp"` (or `import "std/nsmtp"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use lettre::message::header::ContentType;
use lettre::message::{Message, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::client::Tls;
use lettre::{SmtpTransport, Transport};
use niao_ast::Span;
use std::collections::HashMap;
use std::rc::Rc;

// codes.rs integration pending — use local constants until wired.
const E2890_NSMTP_ARITY: u32 = 2890;
const E2891_NSMTP_ERROR: u32 = 2891;
const E2892_NSMTP_TYPE: u32 = 2892;

const DEFAULT_PORT: u16 = 587;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn config_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E2892_NSMTP_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E2890_NSMTP_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn config_arg(args: &[ValueRef], span: Span, name: &str) -> NiaoResult<HashMap<String, ValueRef>> {
    match &*args[0].borrow() {
        Value::Object(map) => Ok(map.clone()),
        other => Err(config_err(
            span,
            format!(
                "{name}() expects a config object, got {}",
                other.type_name()
            ),
        )),
    }
}

fn nsmtp_error(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E2891_NSMTP_ERROR, "nsmtp_error", msg.into(), span)
}

fn ok_bool(b: bool) -> ValueRef {
    Value::Bool(b).ref_cell()
}

fn required_string(
    config: &HashMap<String, ValueRef>,
    field: &str,
    span: Span,
) -> NiaoResult<String> {
    match config.get(field) {
        Some(v) => match &*v.borrow() {
            Value::String(s) if !s.is_empty() => Ok(s.clone()),
            Value::String(_) => Err(config_err(span, format!("config.{field} must not be empty"))),
            other => Err(config_err(
                span,
                format!(
                    "config.{field} must be a string, got {}",
                    other.type_name()
                ),
            )),
        },
        None => Err(config_err(span, format!("config: missing field '{field}'"))),
    }
}

fn optional_string(config: &HashMap<String, ValueRef>, field: &str) -> Option<String> {
    config.get(field).and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.clone()),
        Value::Nil => None,
        _ => None,
    })
}

fn optional_port(config: &HashMap<String, ValueRef>, span: Span) -> NiaoResult<u16> {
    match config.get("port") {
        None => Ok(DEFAULT_PORT),
        Some(v) => match &*v.borrow() {
            Value::Nil => Ok(DEFAULT_PORT),
            Value::Int(n) if (0..=65535).contains(n) => Ok(*n as u16),
            Value::Int(_) => Err(config_err(span, "config.port must be 0..=65535")),
            other => Err(config_err(
                span,
                format!("config.port must be an int, got {}", other.type_name()),
            )),
        },
    }
}

fn optional_bool(config: &HashMap<String, ValueRef>, field: &str, default: bool) -> bool {
    match config.get(field) {
        Some(v) => matches!(&*v.borrow(), Value::Bool(b) if *b),
        None => default,
    }
}

fn parse_recipients(config: &HashMap<String, ValueRef>, span: Span) -> NiaoResult<Vec<String>> {
    match config.get("to") {
        Some(v) => match &*v.borrow() {
            Value::String(s) if !s.is_empty() => Ok(vec![s.clone()]),
            Value::String(_) => Err(config_err(span, "config.to must not be empty")),
            Value::Array(items) => {
                if items.is_empty() {
                    return Err(config_err(span, "config.to must not be empty"));
                }
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    match &*item.borrow() {
                        Value::String(s) if !s.is_empty() => out.push(s.clone()),
                        Value::String(_) => {
                            return Err(config_err(
                                span,
                                "config.to array items must not be empty strings",
                            ));
                        }
                        other => {
                            return Err(config_err(
                                span,
                                format!(
                                    "config.to array items must be strings, got {}",
                                    other.type_name()
                                ),
                            ));
                        }
                    }
                }
                Ok(out)
            }
            other => Err(config_err(
                span,
                format!(
                    "config.to must be a string or array of strings, got {}",
                    other.type_name()
                ),
            )),
        },
        None => Err(config_err(span, "config: missing field 'to'")),
    }
}

// ---------------------------------------------------------------------------
// Send logic (adapted from net/smtp.rs)
// ---------------------------------------------------------------------------

struct SmtpConfig {
    host: String,
    port: u16,
    from: String,
    to: Vec<String>,
    subject: String,
    body: String,
    html: Option<String>,
    user: Option<String>,
    pass: Option<String>,
    tls: bool,
}

fn parse_config(
    config: &HashMap<String, ValueRef>,
    span: Span,
    require_html: bool,
) -> NiaoResult<SmtpConfig> {
    let host = required_string(config, "host", span)?;
    let from = required_string(config, "from", span)?;
    let to = parse_recipients(config, span)?;
    let subject = required_string(config, "subject", span)?;
    let body = required_string(config, "body", span)?;
    let port = optional_port(config, span)?;
    let user = optional_string(config, "user");
    let pass = optional_string(config, "pass");
    let tls = optional_bool(config, "tls", true);

    let html = if require_html {
        Some(required_string(config, "html", span)?)
    } else {
        match config.get("html") {
            Some(v) => match &*v.borrow() {
                Value::String(s) => Some(s.clone()),
                Value::Nil => None,
                other => {
                    return Err(config_err(
                        span,
                        format!("config.html must be a string, got {}", other.type_name()),
                    ));
                }
            },
            None => None,
        }
    };

    Ok(SmtpConfig {
        host,
        port,
        from,
        to,
        subject,
        body,
        html,
        user,
        pass,
        tls,
    })
}

fn address_err(span: Span, e: lettre::address::AddressError) -> ValueRef {
    nsmtp_error(span, e.to_string())
}

fn build_message(cfg: &SmtpConfig, span: Span) -> Result<Message, ValueRef> {
    let from = cfg
        .from
        .parse()
        .map_err(|e: lettre::address::AddressError| address_err(span, e))?;

    let mut builder = Message::builder().from(from);
    for addr in &cfg.to {
        let mailbox = addr
            .parse()
            .map_err(|e: lettre::address::AddressError| address_err(span, e))?;
        builder = builder.to(mailbox);
    }

    let email = if let Some(html) = &cfg.html {
        builder
            .subject(&cfg.subject)
            .multipart(
                MultiPart::alternative()
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_PLAIN)
                            .body(cfg.body.clone()),
                    )
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_HTML)
                            .body(html.clone()),
                    ),
            )
            .map_err(|e| nsmtp_error(span, e.to_string()))?
    } else {
        builder
            .subject(&cfg.subject)
            .singlepart(
                SinglePart::builder()
                    .header(ContentType::TEXT_PLAIN)
                    .body(cfg.body.clone()),
            )
            .map_err(|e| nsmtp_error(span, e.to_string()))?
    };

    Ok(email)
}

fn build_transport(cfg: &SmtpConfig, span: Span) -> Result<SmtpTransport, ValueRef> {
    let mut builder = SmtpTransport::relay(&cfg.host)
        .map_err(|e| nsmtp_error(span, e.to_string()))?
        .port(cfg.port);

    if !cfg.tls {
        builder = builder.tls(Tls::None);
    }

    if let (Some(user), Some(pass)) = (&cfg.user, &cfg.pass) {
        builder = builder.credentials(Credentials::new(user.clone(), pass.clone()));
    }

    Ok(builder.build())
}

fn send_impl(cfg: &SmtpConfig, span: Span) -> ValueRef {
    let email = match build_message(cfg, span) {
        Ok(m) => m,
        Err(err) => return err,
    };
    let mailer = match build_transport(cfg, span) {
        Ok(t) => t,
        Err(err) => return err,
    };
    match mailer.send(&email) {
        Ok(_) => ok_bool(true),
        Err(e) => nsmtp_error(span, e.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// nsmtp_send(config) → true or catchable nsmtp_error
fn nsmtp_send(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsmtp_send", span)?;
    let config = config_arg(args, span, "nsmtp_send")?;
    let cfg = parse_config(&config, span, false)?;
    Ok(send_impl(&cfg, span))
}

/// nsmtp_send_html(config) → true or catchable nsmtp_error (multipart plain + html)
fn nsmtp_send_html(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsmtp_send_html", span)?;
    let config = config_arg(args, span, "nsmtp_send_html")?;
    let cfg = parse_config(&config, span, true)?;
    Ok(send_impl(&cfg, span))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nsmtp_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nsmtp_fns![
    ("nsmtp_send", "send", nsmtp_send),
    ("nsmtp_send_html", "send_html", nsmtp_send_html),
];

fn all_builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nsmtp";
pub const MODULE_PATHS: &[&str] = &["nsmtp", "std/nsmtp"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    fn base_config() -> HashMap<String, ValueRef> {
        let mut cfg = HashMap::new();
        cfg.insert("host".to_string(), Value::String("smtp.example.com".into()).ref_cell());
        cfg.insert("from".to_string(), Value::String("from@example.com".into()).ref_cell());
        cfg.insert("to".to_string(), Value::String("to@example.com".into()).ref_cell());
        cfg.insert("subject".to_string(), Value::String("Hello".into()).ref_cell());
        cfg.insert("body".to_string(), Value::String("Plain text".into()).ref_cell());
        cfg
    }

    #[test]
    fn missing_host() {
        let mut cfg = base_config();
        cfg.remove("host");
        let err = nsmtp_send(&[Value::Object(cfg).ref_cell()], span()).unwrap_err();
        assert!(err.to_string().contains("missing field 'host'"));
    }

    #[test]
    fn missing_from() {
        let mut cfg = base_config();
        cfg.remove("from");
        let err = nsmtp_send(&[Value::Object(cfg).ref_cell()], span()).unwrap_err();
        assert!(err.to_string().contains("missing field 'from'"));
    }

    #[test]
    fn missing_to() {
        let mut cfg = base_config();
        cfg.remove("to");
        let err = nsmtp_send(&[Value::Object(cfg).ref_cell()], span()).unwrap_err();
        assert!(err.to_string().contains("missing field 'to'"));
    }

    #[test]
    fn to_array_parsing() {
        let mut cfg = base_config();
        cfg.insert(
            "to".to_string(),
            Value::Array(vec![
                Value::String("a@example.com".into()).ref_cell(),
                Value::String("b@example.com".into()).ref_cell(),
            ])
            .ref_cell(),
        );
        let parsed = parse_config(&cfg, span(), false).unwrap();
        assert_eq!(parsed.to, vec!["a@example.com", "b@example.com"]);
    }

    #[test]
    fn default_port_and_tls() {
        let cfg = base_config();
        let parsed = parse_config(&cfg, span(), false).unwrap();
        assert_eq!(parsed.port, DEFAULT_PORT);
        assert!(parsed.tls);
    }

    #[test]
    fn send_html_requires_html_field() {
        let cfg = base_config();
        let err = nsmtp_send_html(&[Value::Object(cfg).ref_cell()], span()).unwrap_err();
        assert!(err.to_string().contains("missing field 'html'"));
    }

    #[test]
    fn arity_errors() {
        let cfg = base_config();
        let err = nsmtp_send(&[Value::Object(cfg.clone()).ref_cell(), Value::Nil.ref_cell()], span())
            .unwrap_err();
        assert!(err.to_string().contains("expects 1 argument"));
        let err = nsmtp_send(&[], span()).unwrap_err();
        assert!(err.to_string().contains("expects 1 argument"));
    }

    #[test]
    fn config_must_be_object() {
        let err = nsmtp_send(&[Value::String("not-object".into()).ref_cell()], span()).unwrap_err();
        assert!(err.to_string().contains("expects a config object"));
    }
}
