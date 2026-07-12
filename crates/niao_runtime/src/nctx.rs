//! Native nctx standard library — token estimates, message trim strategies,
//! context budgets, and conversation stats for LLM prompt planning.
//!
//! Import with `import "nctx"` (or `import "std/nctx"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::collections::HashMap;
use std::rc::Rc;

const E3340_NCTX_ARITY: u32 = 3340;
const E3341_NCTX_ERROR: u32 = 3341;
const E3342_NCTX_TYPE: u32 = 3342;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3342_NCTX_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3340_NCTX_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3340_NCTX_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
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

fn messages_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<(String, String)>> {
    match &*args[idx].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::Object(map) => {
                        let role = map
                            .get("role")
                            .and_then(|v| match &*v.borrow() {
                                Value::String(s) => Some(s.clone()),
                                _ => None,
                            })
                            .unwrap_or_else(|| "user".into());
                        let content = map
                            .get("content")
                            .and_then(|v| match &*v.borrow() {
                                Value::String(s) => Some(s.clone()),
                                other => Some(other.to_string()),
                            })
                            .unwrap_or_default();
                        out.push((role, content));
                    }
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() message {} must be {{role, content}} object, got {}",
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
                "{name}() expects a message array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn nctx_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3341_NCTX_ERROR, "nctx_error", msg.into(), span)
}

// ---------------------------------------------------------------------------
// Token estimation (chars/4 heuristic + message overhead)
// ---------------------------------------------------------------------------

fn estimate_text_tokens(text: &str) -> i64 {
    let chars = text.chars().count() as i64;
    std::cmp::max(1, (chars + 3) / 4)
}

fn message_tokens(role: &str, content: &str) -> i64 {
    // Small per-message framing overhead (role label + JSON-ish wrapping).
    estimate_text_tokens(content) + 4 + (role.len() as i64 / 4)
}

fn messages_to_value(messages: &[(String, String)]) -> ValueRef {
    Value::Array(
        messages
            .iter()
            .map(|(role, content)| {
                let mut m = HashMap::new();
                m.insert("role".into(), Value::String(role.clone()).ref_cell());
                m.insert("content".into(), Value::String(content.clone()).ref_cell());
                Value::Object(m).ref_cell()
            })
            .collect(),
    )
    .ref_cell()
}

fn trim_messages(
    messages: &[(String, String)],
    budget: i64,
    strategy: &str,
) -> Result<Vec<(String, String)>, String> {
    if budget <= 0 {
        return Ok(Vec::new());
    }
    let total: i64 = messages
        .iter()
        .map(|(r, c)| message_tokens(r, c))
        .sum();
    if total <= budget {
        return Ok(messages.to_vec());
    }

    match strategy {
        "head" => {
            let mut kept = Vec::new();
            let mut used = 0i64;
            for (role, content) in messages {
                let cost = message_tokens(role, content);
                if used + cost > budget {
                    break;
                }
                used += cost;
                kept.push((role.clone(), content.clone()));
            }
            Ok(kept)
        }
        "middle" => {
            if messages.is_empty() {
                return Ok(Vec::new());
            }
            let mut left = 0usize;
            let mut right = messages.len();
            let mut best = vec![messages[0].clone()];
            while left <= right {
                let take_left = left;
                let take_right = messages.len().saturating_sub(right);
                let mut candidate = Vec::new();
                candidate.extend(messages.iter().take(take_left).cloned());
                if take_right > 0 {
                    candidate.extend(messages.iter().skip(messages.len() - take_right).cloned());
                }
                let cost: i64 = candidate
                    .iter()
                    .map(|(r, c)| message_tokens(r, c))
                    .sum();
                if cost <= budget {
                    best = candidate;
                    left += 1;
                    if right > 0 {
                        right -= 1;
                    }
                } else {
                    break;
                }
            }
            Ok(best)
        }
        "system" => {
            let systems: Vec<_> = messages
                .iter()
                .filter(|(r, _)| r == "system")
                .cloned()
                .collect();
            let mut used: i64 = systems
                .iter()
                .map(|(r, c)| message_tokens(r, c))
                .sum();
            let mut rest: Vec<_> = messages
                .iter()
                .filter(|(r, _)| r != "system")
                .cloned()
                .collect();
            let mut kept = systems;
            while !rest.is_empty() {
                let idx = rest.len() - 1;
                let (role, content) = rest[idx].clone();
                let cost = message_tokens(&role, &content);
                if used + cost > budget {
                    rest.pop();
                } else {
                    used += cost;
                    kept.push((role, content));
                    rest.pop();
                }
            }
            Ok(kept)
        }
        "tail" | _ => {
            let mut kept_rev = Vec::new();
            let mut used = 0i64;
            for (role, content) in messages.iter().rev() {
                let cost = message_tokens(role, content);
                if used + cost > budget {
                    continue;
                }
                used += cost;
                kept_rev.push((role.clone(), content.clone()));
            }
            kept_rev.reverse();
            Ok(kept_rev)
        }
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn nctx_estimate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nctx_estimate", span)?;
    let text = string_arg(args, 0, "nctx_estimate", span)?;
    Ok(Value::Int(estimate_text_tokens(&text)).ref_cell())
}

fn nctx_estimate_messages(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nctx_estimate_messages", span)?;
    let messages = messages_arg(args, 0, "nctx_estimate_messages", span)?;
    let total: i64 = messages
        .iter()
        .map(|(r, c)| message_tokens(r, c))
        .sum();
    Ok(Value::Int(total).ref_cell())
}

fn nctx_trim(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nctx_trim", span)?;
    let messages = messages_arg(args, 0, "nctx_trim", span)?;
    let budget = int_arg(args, 1, "nctx_trim", span)?;
    if budget < 0 {
        return Ok(nctx_err(span, "nctx.trim() budget must be >= 0"));
    }
    let strategy = if args.len() > 2 {
        string_arg(args, 2, "nctx_trim", span)?
    } else {
        "tail".into()
    };
    match trim_messages(&messages, budget, &strategy) {
        Ok(out) => Ok(messages_to_value(&out)),
        Err(e) => Ok(nctx_err(span, e)),
    }
}

fn nctx_stats(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nctx_stats", span)?;
    let messages = messages_arg(args, 0, "nctx_stats", span)?;
    let mut roles: HashMap<String, i64> = HashMap::new();
    let mut chars = 0i64;
    let mut tokens = 0i64;
    for (role, content) in &messages {
        *roles.entry(role.clone()).or_insert(0) += 1;
        chars += content.chars().count() as i64;
        tokens += message_tokens(role, content);
    }
    let mut role_map = HashMap::new();
    for (k, v) in roles {
        role_map.insert(k, Value::Int(v).ref_cell());
    }
    let mut out = HashMap::new();
    out.insert("messages".into(), Value::Int(messages.len() as i64).ref_cell());
    out.insert("chars".into(), Value::Int(chars).ref_cell());
    out.insert("tokens".into(), Value::Int(tokens).ref_cell());
    out.insert("roles".into(), Value::Object(role_map).ref_cell());
    Ok(Value::Object(out).ref_cell())
}

fn nctx_budget(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nctx_budget", span)?;
    let max = int_arg(args, 0, "nctx_budget", span)?;
    if max < 0 {
        return Ok(nctx_err(span, "nctx.budget() max must be >= 0"));
    }
    let reserve = if args.len() > 1 {
        let r = int_arg(args, 1, "nctx_budget", span)?;
        if r < 0 {
            return Ok(nctx_err(span, "nctx.budget() reserve must be >= 0"));
        }
        if r > max {
            return Ok(nctx_err(span, "nctx.budget() reserve cannot exceed max"));
        }
        r
    } else {
        0
    };
    let mut out = HashMap::new();
    out.insert("max".into(), Value::Int(max).ref_cell());
    out.insert("reserve".into(), Value::Int(reserve).ref_cell());
    out.insert("available".into(), Value::Int(max - reserve).ref_cell());
    out.insert("used".into(), Value::Int(0).ref_cell());
    Ok(Value::Object(out).ref_cell())
}

fn nctx_fits(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nctx_fits", span)?;
    let messages = messages_arg(args, 0, "nctx_fits", span)?;
    let budget_obj = match &*args[1].borrow() {
        Value::Object(map) => map.clone(),
        other => {
            return Err(type_err(
                span,
                format!(
                    "nctx.fits() expects budget object as argument 2, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    let available = budget_obj
        .get("available")
        .and_then(|v| match &*v.borrow() {
            Value::Int(n) => Some(*n),
            _ => None,
        })
        .ok_or_else(|| type_err(span, "nctx.fits() budget missing int field 'available'"))?;
    let tokens: i64 = messages
        .iter()
        .map(|(r, c)| message_tokens(r, c))
        .sum();
    Ok(Value::Bool(tokens <= available).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nctx_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nctx_fns![
    ("nctx_estimate", "estimate", nctx_estimate),
    ("nctx_estimate_messages", "estimate_messages", nctx_estimate_messages),
    ("nctx_trim", "trim", nctx_trim),
    ("nctx_stats", "stats", nctx_stats),
    ("nctx_budget", "budget", nctx_budget),
    ("nctx_fits", "fits", nctx_fits),
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

pub const MODULE_NAME: &str = "nctx";
pub const MODULE_PATHS: &[&str] = &["nctx", "std/nctx"];

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

    fn msg(role: &str, content: &str) -> ValueRef {
        let mut m = HashMap::new();
        m.insert("role".into(), Value::String(role.into()).ref_cell());
        m.insert("content".into(), Value::String(content.into()).ref_cell());
        Value::Object(m).ref_cell()
    }

    #[test]
    fn estimate_heuristic() {
        let t = nctx_estimate(&[Value::String("hello world".into()).ref_cell()], span()).unwrap();
        assert!(matches!(&*t.borrow(), Value::Int(n) if *n >= 2));
    }

    #[test]
    fn trim_tail_keeps_recent() {
        let messages = Value::Array(vec![
            msg("user", "one two three four"),
            msg("assistant", "reply one"),
            msg("user", "latest question here"),
        ])
        .ref_cell();
        let trimmed = nctx_trim(
            &[messages, Value::Int(12).ref_cell(), Value::String("tail".into()).ref_cell()],
            span(),
        )
        .unwrap();
        let trimmed_b = trimmed.borrow();
        match &*trimmed_b {
            Value::Array(items) => assert!(!items.is_empty()),
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[test]
    fn stats_and_budget() {
        let messages = Value::Array(vec![
            msg("system", "be helpful"),
            msg("user", "hi"),
            msg("assistant", "hello"),
        ])
        .ref_cell();
        let stats = nctx_stats(&[messages.clone()], span()).unwrap();
        let stats_b = stats.borrow();
        match &*stats_b {
            Value::Object(m) => {
                assert_eq!(
                    match &*m.get("messages").unwrap().borrow() {
                        Value::Int(n) => *n,
                        _ => 0,
                    },
                    3
                );
            }
            other => panic!("expected object, got {other:?}"),
        }
        let b = nctx_budget(&[Value::Int(8192).ref_cell(), Value::Int(512).ref_cell()], span()).unwrap();
        let fits = nctx_fits(&[messages, b], span()).unwrap();
        assert!(matches!(&*fits.borrow(), Value::Bool(true)));
    }
}
