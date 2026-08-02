//! Native nprovider standard library — provider profiles, model aliases,
//! failover chains, and a built-in pricing table for LLM planning.
//!
//! Import with `import "nprovider"` (or `import "std/nprovider"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E3330_NPROVIDER_ARITY: u32 = 3330;
const E3331_NPROVIDER_ERROR: u32 = 3331;
const E3332_NPROVIDER_TYPE: u32 = 3332;

#[derive(Clone, Debug)]
struct Profile {
    provider: String,
    model: String,
    api_base: Option<String>,
    key_env: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ModelPrice {
    in_per_mtok: f64,
    out_per_mtok: f64,
}

#[derive(Clone, Debug)]
struct Chain {
    keys: Vec<String>,
    cursor: usize,
}

thread_local! {
    static PROFILES: RefCell<HashMap<String, Profile>> = RefCell::new(HashMap::new());
    static ALIASES: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
    static CHAINS: RefCell<HashMap<i64, Chain>> = RefCell::new(HashMap::new());
    static NEXT_CHAIN: RefCell<i64> = const { RefCell::new(1) };
    static PRICE_OVERRIDES: RefCell<HashMap<String, ModelPrice>> = RefCell::new(HashMap::new());
}

fn builtin_prices() -> HashMap<&'static str, ModelPrice> {
    HashMap::from([
        (
            "gpt-4o",
            ModelPrice {
                in_per_mtok: 2.5,
                out_per_mtok: 10.0,
            },
        ),
        (
            "gpt-4o-mini",
            ModelPrice {
                in_per_mtok: 0.15,
                out_per_mtok: 0.6,
            },
        ),
        (
            "claude-sonnet",
            ModelPrice {
                in_per_mtok: 3.0,
                out_per_mtok: 15.0,
            },
        ),
        (
            "gemini-pro",
            ModelPrice {
                in_per_mtok: 1.25,
                out_per_mtok: 5.0,
            },
        ),
        (
            "llama-local",
            ModelPrice {
                in_per_mtok: 0.0,
                out_per_mtok: 0.0,
            },
        ),
    ])
}

fn lookup_price(model: &str) -> Option<ModelPrice> {
    PRICE_OVERRIDES.with(|over| {
        if let Some(p) = over.borrow().get(model) {
            return Some(*p);
        }
        builtin_prices().get(model).copied()
    })
}

fn resolve_key(key: &str) -> Option<String> {
    ALIASES.with(|a| a.borrow().get(key).cloned()).or_else(|| {
        PROFILES.with(|p| {
            if p.borrow().contains_key(key) {
                Some(key.to_string())
            } else {
                None
            }
        })
    })
}

fn profile_to_value(name: &str, p: &Profile) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("name".into(), Value::String(name.into()).ref_cell());
    map.insert(
        "provider".into(),
        Value::String(p.provider.clone()).ref_cell(),
    );
    map.insert("model".into(), Value::String(p.model.clone()).ref_cell());
    if let Some(ref base) = p.api_base {
        map.insert("api_base".into(), Value::String(base.clone()).ref_cell());
    }
    if let Some(ref env) = p.key_env {
        map.insert("key_env".into(), Value::String(env.clone()).ref_cell());
    }
    Value::Object(map).ref_cell()
}

fn new_chain_id() -> i64 {
    NEXT_CHAIN.with(|n| {
        let mut g = n.borrow_mut();
        let id = *g;
        *g += 1;
        id
    })
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3332_NPROVIDER_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3330_NPROVIDER_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(
    args: &[ValueRef],
    min: usize,
    max: usize,
    name: &str,
    span: Span,
) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3330_NPROVIDER_ARITY,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn string_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn num_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<f64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n as f64),
        Value::Float(f) => Ok(*f),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a number as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn int_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an int as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn object_arg(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> NiaoResult<HashMap<String, ValueRef>> {
    match &*args[idx].borrow() {
        Value::Object(map) => Ok(map.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an object as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn string_array_arg(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> NiaoResult<Vec<String>> {
    match &*args[idx].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() expects strings in array; item {} is {}",
                                i + 1,
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a string array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn opt_string_field(map: &HashMap<String, ValueRef>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.clone()),
        Value::Nil => None,
        _ => None,
    })
}

fn nprovider_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3331_NPROVIDER_ERROR, "nprovider_error", msg.into(), span)
}

fn parse_profile_config(
    map: &HashMap<String, ValueRef>,
    span: Span,
) -> NiaoResult<(String, String, Option<String>, Option<String>)> {
    let provider = opt_string_field(map, "provider").ok_or_else(|| {
        type_err(
            span,
            "nprovider.profile() config requires string field 'provider'",
        )
    })?;
    let model = opt_string_field(map, "model").ok_or_else(|| {
        type_err(
            span,
            "nprovider.profile() config requires string field 'model'",
        )
    })?;
    Ok((
        provider,
        model,
        opt_string_field(map, "api_base"),
        opt_string_field(map, "key_env"),
    ))
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn nprovider_profile(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nprovider_profile", span)?;
    let name = string_arg(args, 0, "nprovider_profile", span)?;
    if name.is_empty() {
        return Ok(nprovider_err(
            span,
            "nprovider.profile() name must not be empty",
        ));
    }
    let cfg = object_arg(args, 1, "nprovider_profile", span)?;
    let (provider, model, api_base, key_env) = parse_profile_config(&cfg, span)?;
    PROFILES.with(|p| {
        p.borrow_mut().insert(
            name.clone(),
            Profile {
                provider,
                model,
                api_base,
                key_env,
            },
        );
    });
    Ok(Value::Nil.ref_cell())
}

fn nprovider_alias(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nprovider_alias", span)?;
    let alias = string_arg(args, 0, "nprovider_alias", span)?;
    let target = string_arg(args, 1, "nprovider_alias", span)?;
    if alias.is_empty() || target.is_empty() {
        return Ok(nprovider_err(
            span,
            "nprovider.alias() names must not be empty",
        ));
    }
    ALIASES.with(|a| {
        a.borrow_mut().insert(alias, target);
    });
    Ok(Value::Nil.ref_cell())
}

fn nprovider_resolve(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nprovider_resolve", span)?;
    let key = string_arg(args, 0, "nprovider_resolve", span)?;
    let resolved = match resolve_key(&key) {
        Some(r) => r,
        None => {
            return Ok(nprovider_err(
                span,
                format!("unknown profile or alias '{key}'"),
            ))
        }
    };
    PROFILES.with(|p| {
        let profiles = p.borrow();
        match profiles.get(&resolved) {
            Some(profile) => Ok(profile_to_value(&resolved, profile)),
            None => Ok(nprovider_err(
                span,
                format!("alias '{key}' points to missing profile '{resolved}'"),
            )),
        }
    })
}

fn nprovider_chain(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nprovider_chain", span)?;
    let keys = string_array_arg(args, 0, "nprovider_chain", span)?;
    if keys.is_empty() {
        return Ok(nprovider_err(
            span,
            "nprovider.chain() requires at least one entry",
        ));
    }
    for k in &keys {
        if resolve_key(k).is_none() {
            return Ok(nprovider_err(
                span,
                format!("nprovider.chain() unknown profile or alias '{k}'"),
            ));
        }
    }
    let id = new_chain_id();
    CHAINS.with(|c| {
        c.borrow_mut().insert(id, Chain { keys, cursor: 0 });
    });
    Ok(Value::Int(id).ref_cell())
}

fn nprovider_next(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nprovider_next", span)?;
    let id = int_arg(args, 0, "nprovider_next", span)?;
    let advance = if args.len() > 1 {
        match &*args[1].borrow() {
            Value::Bool(b) => *b,
            other => {
                return Err(type_err(
                    span,
                    format!(
                        "nprovider.next() advance flag expects bool, got {}",
                        other.type_name()
                    ),
                ));
            }
        }
    } else {
        true
    };

    CHAINS.with(|chains| {
        let mut chains = chains.borrow_mut();
        let chain = match chains.get_mut(&id) {
            Some(c) => c,
            None => {
                return Ok(nprovider_err(
                    span,
                    format!("invalid or closed failover chain handle {id}"),
                ));
            }
        };
        let key = chain.keys[chain.cursor % chain.keys.len()].clone();
        let index = chain.cursor;
        if advance {
            chain.cursor = (chain.cursor + 1) % chain.keys.len();
        }
        drop(chains);

        let resolved = resolve_key(&key).unwrap_or(key.clone());
        PROFILES.with(|p| {
            let profiles = p.borrow();
            match profiles.get(&resolved) {
                Some(profile) => {
                    let mut out = HashMap::new();
                    out.insert("chain".into(), Value::Int(id).ref_cell());
                    out.insert("key".into(), Value::String(key).ref_cell());
                    out.insert("profile".into(), profile_to_value(&resolved, profile));
                    out.insert("index".into(), Value::Int(index as i64).ref_cell());
                    Ok(Value::Object(out).ref_cell())
                }
                None => Ok(nprovider_err(
                    span,
                    format!("chain entry '{key}' has no profile"),
                )),
            }
        })
    })
}

fn nprovider_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nprovider_close", span)?;
    let id = int_arg(args, 0, "nprovider_close", span)?;
    let removed = CHAINS.with(|c| c.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

fn nprovider_price(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nprovider_price", span)?;
    let model = string_arg(args, 0, "nprovider_price", span)?;
    let tokens_in = num_arg(args, 1, "nprovider_price", span)?;
    let tokens_out = if args.len() > 2 {
        num_arg(args, 2, "nprovider_price", span)?
    } else {
        0.0
    };
    if tokens_in < 0.0 || tokens_out < 0.0 {
        return Ok(nprovider_err(span, "token counts must be non-negative"));
    }
    let price = match lookup_price(&model) {
        Some(p) => p,
        None => {
            return Ok(nprovider_err(
                span,
                format!("unknown model '{model}' for pricing"),
            ))
        }
    };
    let usd = (tokens_in / 1_000_000.0) * price.in_per_mtok
        + (tokens_out / 1_000_000.0) * price.out_per_mtok;
    Ok(Value::Float(usd).ref_cell())
}

fn nprovider_set_price(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nprovider_set_price", span)?;
    let model = string_arg(args, 0, "nprovider_set_price", span)?;
    let in_rate = num_arg(args, 1, "nprovider_set_price", span)?;
    let out_rate = num_arg(args, 2, "nprovider_set_price", span)?;
    PRICE_OVERRIDES.with(|p| {
        p.borrow_mut().insert(
            model,
            ModelPrice {
                in_per_mtok: in_rate,
                out_per_mtok: out_rate,
            },
        );
    });
    Ok(Value::Nil.ref_cell())
}

fn nprovider_table(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nprovider_table", span)?;
    let mut out = HashMap::new();
    for (model, price) in builtin_prices() {
        let mut row = HashMap::new();
        row.insert(
            "in_per_mtok".into(),
            Value::Float(price.in_per_mtok).ref_cell(),
        );
        row.insert(
            "out_per_mtok".into(),
            Value::Float(price.out_per_mtok).ref_cell(),
        );
        out.insert(model.to_string(), Value::Object(row).ref_cell());
    }
    PRICE_OVERRIDES.with(|over| {
        for (model, price) in over.borrow().iter() {
            let mut row = HashMap::new();
            row.insert(
                "in_per_mtok".into(),
                Value::Float(price.in_per_mtok).ref_cell(),
            );
            row.insert(
                "out_per_mtok".into(),
                Value::Float(price.out_per_mtok).ref_cell(),
            );
            out.insert(model.clone(), Value::Object(row).ref_cell());
        }
    });
    Ok(Value::Object(out).ref_cell())
}

fn nprovider_list(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nprovider_list", span)?;
    let mut out = HashMap::new();
    PROFILES.with(|p| {
        let mut profiles = HashMap::new();
        for (name, profile) in p.borrow().iter() {
            profiles.insert(name.clone(), profile_to_value(name, profile));
        }
        out.insert("profiles".into(), Value::Object(profiles).ref_cell());
    });
    ALIASES.with(|a| {
        let mut aliases = HashMap::new();
        for (k, v) in a.borrow().iter() {
            aliases.insert(k.clone(), Value::String(v.clone()).ref_cell());
        }
        out.insert("aliases".into(), Value::Object(aliases).ref_cell());
    });
    Ok(Value::Object(out).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nprovider_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nprovider_fns![
    ("nprovider_profile", "profile", nprovider_profile),
    ("nprovider_alias", "alias", nprovider_alias),
    ("nprovider_resolve", "resolve", nprovider_resolve),
    ("nprovider_chain", "chain", nprovider_chain),
    ("nprovider_next", "next", nprovider_next),
    ("nprovider_close", "close", nprovider_close),
    ("nprovider_price", "price", nprovider_price),
    ("nprovider_set_price", "set_price", nprovider_set_price),
    ("nprovider_table", "table", nprovider_table),
    ("nprovider_list", "list", nprovider_list),
];

fn all_builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs()
        .into_iter()
        .map(|(flat, _, f)| (flat, f))
        .collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nprovider";
pub const MODULE_PATHS: &[&str] = &["nprovider", "std/nprovider"];

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

    fn s(v: &str) -> ValueRef {
        Value::String(v.into()).ref_cell()
    }

    fn setup_profiles() {
        PROFILES.with(|p| p.borrow_mut().clear());
        ALIASES.with(|a| a.borrow_mut().clear());
        CHAINS.with(|c| c.borrow_mut().clear());
        nprovider_profile(
            &[
                s("openai-main"),
                Value::Object(HashMap::from([
                    ("provider".into(), s("openai")),
                    ("model".into(), s("gpt-4o-mini")),
                    ("key_env".into(), s("OPENAI_API_KEY")),
                ]))
                .ref_cell(),
            ],
            span(),
        )
        .unwrap();
        nprovider_profile(
            &[
                s("anthropic-backup"),
                Value::Object(HashMap::from([
                    ("provider".into(), s("anthropic")),
                    ("model".into(), s("claude-sonnet")),
                ]))
                .ref_cell(),
            ],
            span(),
        )
        .unwrap();
        nprovider_alias(&[s("fast"), s("openai-main")], span()).unwrap();
    }

    #[test]
    fn resolve_and_price() {
        setup_profiles();
        let r = nprovider_resolve(&[s("fast")], span()).unwrap();
        let r_b = r.borrow();
        match &*r_b {
            Value::Object(m) => {
                assert_eq!(
                    match &*m.get("model").unwrap().borrow() {
                        Value::String(x) => x.as_str(),
                        _ => "",
                    },
                    "gpt-4o-mini"
                );
            }
            other => panic!("expected object, got {other:?}"),
        }
        let usd = nprovider_price(
            &[
                s("gpt-4o-mini"),
                Value::Int(1_000_000).ref_cell(),
                Value::Int(0).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let usd_b = usd.borrow();
        match &*usd_b {
            Value::Float(f) => assert!((*f - 0.15).abs() < 1e-9),
            other => panic!("expected float, got {other:?}"),
        }
    }

    #[test]
    fn failover_chain() {
        setup_profiles();
        let chain = nprovider_chain(
            &[Value::Array(vec![s("fast"), s("anthropic-backup")]).ref_cell()],
            span(),
        )
        .unwrap();
        let chain_b = chain.borrow();
        let id = match &*chain_b {
            Value::Int(n) => *n,
            other => panic!("expected int, got {other:?}"),
        };
        let first = nprovider_next(&[Value::Int(id).ref_cell()], span()).unwrap();
        let first_b = first.borrow();
        match &*first_b {
            Value::Object(m) => {
                assert_eq!(
                    match &*m.get("key").unwrap().borrow() {
                        Value::String(x) => x.as_str(),
                        _ => "",
                    },
                    "fast"
                );
            }
            other => panic!("expected object, got {other:?}"),
        }
        let second = nprovider_next(&[Value::Int(id).ref_cell()], span()).unwrap();
        let second_b = second.borrow();
        match &*second_b {
            Value::Object(m) => {
                assert_eq!(
                    match &*m.get("key").unwrap().borrow() {
                        Value::String(x) => x.as_str(),
                        _ => "",
                    },
                    "anthropic-backup"
                );
            }
            other => panic!("expected object, got {other:?}"),
        }
        nprovider_close(&[Value::Int(id).ref_cell()], span()).unwrap();
    }
}
