//! Native nagent standard library — lightweight multi-agent orchestration
//! structure (message logs, local memory, handoffs, round-robin run).
//! No LLM calls — scaffolding only.
//!
//! Import with `import "nagent"` (or `import "std/nagent"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// Wired into niao_errors::codes by central integration.
const E2990_NAGENT_ARITY: u32 = 2990;
const E2991_NAGENT_ERROR: u32 = 2991;
const E2992_NAGENT_TYPE: u32 = 2992;
const E2993_NAGENT_INVALID_HANDLE: u32 = 2993;

// ---------------------------------------------------------------------------
// Agent model
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Message {
    role: String,
    content: String,
}

struct Agent {
    name: String,
    role: String,
    messages: Vec<Message>,
    memory: HashMap<String, ValueRef>,
}

impl Agent {
    fn new(name: String, role: String) -> Self {
        Agent {
            name,
            role,
            messages: Vec::new(),
            memory: HashMap::new(),
        }
    }

    fn push_msg(&mut self, role: impl Into<String>, content: impl Into<String>) {
        self.messages.push(Message {
            role: role.into(),
            content: content.into(),
        });
    }

    fn messages_value(&self) -> ValueRef {
        let items: Vec<ValueRef> = self
            .messages
            .iter()
            .map(|m| {
                let mut map = HashMap::new();
                map.insert("role".to_string(), Value::String(m.role.clone()).ref_cell());
                map.insert(
                    "content".to_string(),
                    Value::String(m.content.clone()).ref_cell(),
                );
                Value::Object(map).ref_cell()
            })
            .collect();
        Value::Array(items).ref_cell()
    }
}

thread_local! {
    static AGENTS: RefCell<HashMap<i64, Agent>> = RefCell::new(HashMap::new());
    static NEXT_HANDLE: RefCell<i64> = const { RefCell::new(1) };
}

fn new_handle() -> i64 {
    NEXT_HANDLE.with(|h| {
        let mut h = h.borrow_mut();
        let id = *h;
        *h += 1;
        id
    })
}

fn with_agent<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut Agent) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    AGENTS.with(|agents| {
        let mut agents = agents.borrow_mut();
        match agents.get_mut(&id) {
            Some(a) => Ok(Ok(f(a))),
            None => Ok(Err(error_value(
                E2993_NAGENT_INVALID_HANDLE,
                "nagent_error",
                format!("invalid or closed agent handle {id}"),
                span,
            ))),
        }
    })
}

fn agent_name(id: i64) -> Option<String> {
    AGENTS.with(|agents| agents.borrow().get(&id).map(|a| a.name.clone()))
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E2992_NAGENT_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E2990_NAGENT_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E2990_NAGENT_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn int_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an int handle as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
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

fn handle_array_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<i64>> {
    match &*args[idx].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::Int(n) => out.push(*n),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() expects int handles; item {} is {}",
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
                "{name}() expects an array of handles as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn nagent_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E2991_NAGENT_ERROR, "nagent_error", msg.into(), span)
}

fn do_step(agent: &mut Agent, input: &str) -> String {
    agent.push_msg("user", input);
    let reply = format!("acked: {input}");
    agent.push_msg("assistant", &reply);
    reply
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// nagent_new(name, role?)
fn nagent_new(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nagent_new", span)?;
    let name = string_arg(args, 0, "nagent_new", span)?;
    if name.is_empty() {
        return Ok(nagent_err(span, "nagent_new() name must not be empty"));
    }
    let role = if args.len() > 1 {
        string_arg(args, 1, "nagent_new", span)?
    } else {
        String::new()
    };
    let id = new_handle();
    AGENTS.with(|agents| {
        agents.borrow_mut().insert(id, Agent::new(name, role));
    });
    Ok(Value::Int(id).ref_cell())
}

/// nagent_say(h, role_or_name, content)
fn nagent_say(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nagent_say", span)?;
    let id = int_arg(args, 0, "nagent_say", span)?;
    let role = string_arg(args, 1, "nagent_say", span)?;
    let content = string_arg(args, 2, "nagent_say", span)?;
    match with_agent(id, span, |a| a.push_msg(role, content))? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

/// nagent_step(h, input_string) — append user input, return assistant placeholder.
fn nagent_step(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nagent_step", span)?;
    let id = int_arg(args, 0, "nagent_step", span)?;
    let input = string_arg(args, 1, "nagent_step", span)?;
    match with_agent(id, span, |a| do_step(a, &input))? {
        Ok(reply) => Ok(Value::String(reply).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nagent_messages(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nagent_messages", span)?;
    let id = int_arg(args, 0, "nagent_messages", span)?;
    match with_agent(id, span, |a| a.messages_value())? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

fn nagent_remember(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nagent_remember", span)?;
    let id = int_arg(args, 0, "nagent_remember", span)?;
    let key = string_arg(args, 1, "nagent_remember", span)?;
    let val = Rc::clone(&args[2]);
    match with_agent(id, span, |a| {
        a.memory.insert(key, val);
    })? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nagent_recall(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nagent_recall", span)?;
    let id = int_arg(args, 0, "nagent_recall", span)?;
    let key = string_arg(args, 1, "nagent_recall", span)?;
    match with_agent(id, span, |a| a.memory.get(&key).map(Rc::clone))? {
        Ok(Some(v)) => Ok(v),
        Ok(None) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

/// nagent_handoff(from_h, to_h, msg) — append handoff notes to both logs.
fn nagent_handoff(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nagent_handoff", span)?;
    let from_id = int_arg(args, 0, "nagent_handoff", span)?;
    let to_id = int_arg(args, 1, "nagent_handoff", span)?;
    let msg = string_arg(args, 2, "nagent_handoff", span)?;

    let from_name = match agent_name(from_id) {
        Some(n) => n,
        None => {
            return Ok(error_value(
                E2993_NAGENT_INVALID_HANDLE,
                "nagent_error",
                format!("invalid or closed agent handle {from_id}"),
                span,
            ));
        }
    };
    let to_name = match agent_name(to_id) {
        Some(n) => n,
        None => {
            return Ok(error_value(
                E2993_NAGENT_INVALID_HANDLE,
                "nagent_error",
                format!("invalid or closed agent handle {to_id}"),
                span,
            ));
        }
    };

    let out_note = format!("handoff→{to_name}: {msg}");
    let in_note = format!("handoff←{from_name}: {msg}");

    match with_agent(from_id, span, |a| a.push_msg("handoff", out_note))? {
        Ok(()) => {}
        Err(e) => return Ok(e),
    }
    match with_agent(to_id, span, |a| a.push_msg("handoff", in_note))? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nagent_name(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nagent_name", span)?;
    let id = int_arg(args, 0, "nagent_name", span)?;
    match with_agent(id, span, |a| a.name.clone())? {
        Ok(n) => Ok(Value::String(n).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nagent_role(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nagent_role", span)?;
    let id = int_arg(args, 0, "nagent_role", span)?;
    match with_agent(id, span, |a| a.role.clone())? {
        Ok(r) => Ok(Value::String(r).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nagent_clear_messages(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nagent_clear_messages", span)?;
    let id = int_arg(args, 0, "nagent_clear_messages", span)?;
    match with_agent(id, span, |a| a.messages.clear())? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nagent_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nagent_close", span)?;
    let id = int_arg(args, 0, "nagent_close", span)?;
    let removed = AGENTS.with(|agents| agents.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

/// nagent_run(handles_array, kickoff_string, max_steps?)
/// Round-robin: each agent steps with the previous output; returns final string.
fn nagent_run(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nagent_run", span)?;
    let handles = handle_array_arg(args, 0, "nagent_run", span)?;
    let kickoff = string_arg(args, 1, "nagent_run", span)?;
    if handles.is_empty() {
        return Ok(nagent_err(span, "nagent_run() requires at least one agent handle"));
    }
    let max_steps = if args.len() > 2 {
        let n = int_arg(args, 2, "nagent_run", span)?;
        if n < 0 {
            return Ok(nagent_err(span, "nagent_run() max_steps must be >= 0"));
        }
        n as usize
    } else {
        handles.len()
    };

    let mut output = kickoff;
    for i in 0..max_steps {
        let id = handles[i % handles.len()];
        match with_agent(id, span, |a| do_step(a, &output))? {
            Ok(reply) => output = reply,
            Err(e) => return Ok(e),
        }
    }
    Ok(Value::String(output).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nagent_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nagent_fns![
    ("nagent_new", "new", nagent_new),
    ("nagent_say", "say", nagent_say),
    ("nagent_step", "step", nagent_step),
    ("nagent_messages", "messages", nagent_messages),
    ("nagent_remember", "remember", nagent_remember),
    ("nagent_recall", "recall", nagent_recall),
    ("nagent_handoff", "handoff", nagent_handoff),
    ("nagent_name", "name", nagent_name),
    ("nagent_role", "role", nagent_role),
    ("nagent_clear_messages", "clear_messages", nagent_clear_messages),
    ("nagent_close", "close", nagent_close),
    ("nagent_run", "run", nagent_run),
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

pub const MODULE_NAME: &str = "nagent";
pub const MODULE_PATHS: &[&str] = &["nagent", "std/nagent"];

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

    fn i(v: i64) -> ValueRef {
        Value::Int(v).ref_cell()
    }

    fn s(v: &str) -> ValueRef {
        Value::String(v.to_string()).ref_cell()
    }

    fn handle(r: NiaoResult<ValueRef>) -> ValueRef {
        let v = r.unwrap();
        assert!(matches!(&*v.borrow(), Value::Int(_)), "expected handle int");
        v
    }

    fn as_int(v: &ValueRef) -> i64 {
        match &*v.borrow() {
            Value::Int(n) => *n,
            other => panic!("expected int, got {other:?}"),
        }
    }

    fn as_str(v: &ValueRef) -> String {
        match &*v.borrow() {
            Value::String(s) => s.clone(),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn new_step_messages() {
        let h = handle(nagent_new(&[s("alice"), s("researcher")], span()));
        assert_eq!(as_str(&nagent_name(&[h.clone()], span()).unwrap()), "alice");
        assert_eq!(
            as_str(&nagent_role(&[h.clone()], span()).unwrap()),
            "researcher"
        );

        let reply = nagent_step(&[h.clone(), s("hello")], span()).unwrap();
        assert_eq!(as_str(&reply), "acked: hello");

        let msgs = nagent_messages(&[h.clone()], span()).unwrap();
        match &*msgs.borrow() {
            Value::Array(items) => {
                assert_eq!(items.len(), 2);
                match &*items[0].borrow() {
                    Value::Object(m) => {
                        assert_eq!(as_str(m.get("role").unwrap()), "user");
                        assert_eq!(as_str(m.get("content").unwrap()), "hello");
                    }
                    other => panic!("expected object, got {other:?}"),
                }
                match &*items[1].borrow() {
                    Value::Object(m) => {
                        assert_eq!(as_str(m.get("role").unwrap()), "assistant");
                        assert_eq!(as_str(m.get("content").unwrap()), "acked: hello");
                    }
                    other => panic!("expected object, got {other:?}"),
                }
            }
            other => panic!("expected array, got {other:?}"),
        }
        nagent_close(&[h], span()).unwrap();
    }

    #[test]
    fn say_remember_recall_clear() {
        let h = handle(nagent_new(&[s("bot")], span()));
        nagent_say(&[h.clone(), s("system"), s("be brief")], span()).unwrap();
        nagent_remember(&[h.clone(), s("topic"), s("math")], span()).unwrap();
        assert_eq!(
            as_str(&nagent_recall(&[h.clone(), s("topic")], span()).unwrap()),
            "math"
        );
        let miss = nagent_recall(&[h.clone(), s("missing")], span()).unwrap();
        assert!(matches!(&*miss.borrow(), Value::Nil));

        nagent_clear_messages(&[h.clone()], span()).unwrap();
        let msgs = nagent_messages(&[h.clone()], span()).unwrap();
        match &*msgs.borrow() {
            Value::Array(items) => assert!(items.is_empty()),
            other => panic!("expected array, got {other:?}"),
        }
        // memory survives clear_messages
        assert_eq!(
            as_str(&nagent_recall(&[h.clone(), s("topic")], span()).unwrap()),
            "math"
        );
        nagent_close(&[h], span()).unwrap();
    }

    #[test]
    fn handoff_both_logs() {
        let a = handle(nagent_new(&[s("alpha")], span()));
        let b = handle(nagent_new(&[s("beta")], span()));
        nagent_handoff(&[a.clone(), b.clone(), s("take over")], span()).unwrap();

        let am = nagent_messages(&[a.clone()], span()).unwrap();
        let bm = nagent_messages(&[b.clone()], span()).unwrap();
        match (&*am.borrow(), &*bm.borrow()) {
            (Value::Array(aa), Value::Array(bb)) => {
                assert_eq!(aa.len(), 1);
                assert_eq!(bb.len(), 1);
                assert_eq!(
                    as_str(
                        match &*aa[0].borrow() {
                            Value::Object(m) => m.get("content").unwrap(),
                            _ => panic!("obj"),
                        }
                    ),
                    "handoff→beta: take over"
                );
                assert_eq!(
                    as_str(
                        match &*bb[0].borrow() {
                            Value::Object(m) => m.get("content").unwrap(),
                            _ => panic!("obj"),
                        }
                    ),
                    "handoff←alpha: take over"
                );
            }
            _ => panic!("expected arrays"),
        }
        nagent_close(&[a], span()).unwrap();
        nagent_close(&[b], span()).unwrap();
    }

    #[test]
    fn run_round_robin() {
        let a = handle(nagent_new(&[s("a")], span()));
        let b = handle(nagent_new(&[s("b")], span()));
        let arr = Value::Array(vec![a.clone(), b.clone()]).ref_cell();
        let final_out = nagent_run(&[arr, s("start"), i(2)], span()).unwrap();
        // step1: a gets "start" → "acked: start"
        // step2: b gets "acked: start" → "acked: acked: start"
        assert_eq!(as_str(&final_out), "acked: acked: start");
        nagent_close(&[a], span()).unwrap();
        nagent_close(&[b], span()).unwrap();
    }

    #[test]
    fn invalid_handle_error_value() {
        let v = nagent_step(&[i(424_242), s("x")], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
        let closed = handle(nagent_new(&[s("tmp")], span()));
        let id = as_int(&closed);
        nagent_close(&[closed], span()).unwrap();
        let v2 = nagent_name(&[i(id)], span()).unwrap();
        assert!(matches!(&*v2.borrow(), Value::Error(_)));
    }

    #[test]
    fn step_appends_assistant_placeholder() {
        let h = handle(nagent_new(&[s("x")], span()));
        let r1 = as_str(&nagent_step(&[h.clone(), s("one")], span()).unwrap());
        let r2 = as_str(&nagent_step(&[h.clone(), s("two")], span()).unwrap());
        assert_eq!(r1, "acked: one");
        assert_eq!(r2, "acked: two");
        let msgs = nagent_messages(&[h.clone()], span()).unwrap();
        match &*msgs.borrow() {
            Value::Array(items) => {
                assert_eq!(items.len(), 4);
                let last = &items[items.len() - 1];
                match &*last.borrow() {
                    Value::Object(m) => {
                        assert_eq!(as_str(m.get("role").unwrap()), "assistant");
                        assert_eq!(as_str(m.get("content").unwrap()), "acked: two");
                    }
                    other => panic!("expected object, got {other:?}"),
                }
            }
            other => panic!("expected array, got {other:?}"),
        }
        nagent_close(&[h], span()).unwrap();
    }
}
