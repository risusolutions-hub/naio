//! Native nfsm standard library — finite state machines and hierarchical
//! statecharts: states, guards, transitions, hooks (~transitions,
//! python-statemachine subset).
//!
//! Import with `import "nfsm"` (or `import "std/nfsm"`).

use crate::{call_niao_function, error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_fsm::{
    ActiveTransition, FsmEngine, FsmError, FsmSpec, StateDef, TransitionApply, TransitionDef,
    TransitionDest, TransitionSources,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E3504_NFSM_ARITY: u32 = codes::E3504_NFSM_ARITY;
const E3505_NFSM_ERROR: u32 = codes::E3505_NFSM_ERROR;
const E3506_NFSM_TYPE: u32 = codes::E3506_NFSM_TYPE;
const E3507_NFSM_INVALID_HANDLE: u32 = codes::E3507_NFSM_INVALID_HANDLE;

// ---------------------------------------------------------------------------
// Machine store
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct TransitionHooks {
    prepare: Option<ValueRef>,
    guard: Option<ValueRef>,
    guard_unless: bool,
    on: Option<ValueRef>,
}

#[derive(Clone)]
struct MachineHooks {
    before: Option<ValueRef>,
    after: Option<ValueRef>,
    on_prepare: Option<ValueRef>,
    on_transition: Option<ValueRef>,
    on_enter: HashMap<String, ValueRef>,
    on_exit: HashMap<String, ValueRef>,
}

struct MachineInstance {
    engine: FsmEngine,
    context: ValueRef,
    hooks: MachineHooks,
    transition_hooks: Vec<TransitionHooks>,
}

thread_local! {
    static MACHINES: RefCell<HashMap<i64, MachineInstance>> = RefCell::new(HashMap::new());
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

fn register(instance: MachineInstance) -> i64 {
    let id = new_handle();
    MACHINES.with(|m| {
        m.borrow_mut().insert(id, instance);
    });
    id
}

fn with_machine_mut<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut MachineInstance) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    MACHINES.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(inst) => Ok(Ok(f(inst))),
            None => Ok(Err(error_value(
                E3507_NFSM_INVALID_HANDLE,
                "nfsm_error",
                format!("invalid or closed nfsm handle {id}"),
                span,
            ))),
        }
    })
}

fn with_machine<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&MachineInstance) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    MACHINES.with(|m| {
        let m = m.borrow();
        match m.get(&id) {
            Some(inst) => Ok(Ok(f(inst))),
            None => Ok(Err(error_value(
                E3507_NFSM_INVALID_HANDLE,
                "nfsm_error",
                format!("invalid or closed nfsm handle {id}"),
                span,
            ))),
        }
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3506_NFSM_TYPE, msg.into())
}

fn nfsm_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3505_NFSM_ERROR, "nfsm_error", msg.into(), span)
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3504_NFSM_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3504_NFSM_ARITY,
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
                "{name}() expects an int as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn str_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
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

fn bool_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<bool> {
    match &*args[idx].borrow() {
        Value::Bool(b) => Ok(*b),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a bool as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn handle_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    int_arg(args, idx, name, span)
}

fn bool_val(b: bool) -> NiaoResult<ValueRef> {
    Ok(Value::Bool(b).ref_cell())
}

fn nil_val() -> NiaoResult<ValueRef> {
    Ok(Value::Nil.ref_cell())
}

fn is_callable(v: &Value) -> bool {
    matches!(v, Value::Function(_) | Value::NativeFunction(_))
}

fn optional_callable(map: &HashMap<String, ValueRef>, key: &str) -> Option<ValueRef> {
    map.get(key).and_then(|v| {
        if is_callable(&v.borrow()) {
            Some(Rc::clone(v))
        } else {
            None
        }
    })
}

fn invoke_callable(callee: &ValueRef, args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    match &*callee.borrow() {
        Value::NativeFunction(native) => native(args, span),
        Value::Function(_) => call_niao_function(Rc::clone(callee), args, span),
        other => Err(type_err(
            span,
            format!("expected callable, got {}", other.type_name()),
        )),
    }
}

fn invoke_bool(callee: &ValueRef, args: &[ValueRef], span: Span) -> NiaoResult<bool> {
    let out = invoke_callable(callee, args, span)?;
    match &*out.borrow() {
        Value::Bool(b) => Ok(*b),
        Value::Nil => Ok(true),
        other => Err(type_err(
            span,
            format!("guard/prepare must return bool or nil, got {}", other.type_name()),
        )),
    }
}

fn fsm_error_to_value(span: Span, e: FsmError) -> ValueRef {
    nfsm_err(span, e.to_string())
}

// ---------------------------------------------------------------------------
// Spec parsing
// ---------------------------------------------------------------------------

fn string_list(v: &Value, field: &str, span: Span) -> NiaoResult<Vec<String>> {
    match v {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match &*item.borrow() {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!("{field} entries must be strings, got {}", other.type_name()),
                        ));
                    }
                }
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!("{field} must be an array, got {}", other.type_name()),
        )),
    }
}

struct RawStateDef {
    name: String,
    parent: Option<u32>,
    initial_child: Option<u32>,
    is_history: bool,
    is_final: bool,
    parent_name: Option<String>,
    initial_name: Option<String>,
}

fn parse_states(map: &HashMap<String, ValueRef>, span: Span) -> NiaoResult<Vec<RawStateDef>> {
    let states_val = map.get("states").ok_or_else(|| {
        type_err(span, "nfsm spec requires 'states' array")
    })?;
    match &*states_val.borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match &*item.borrow() {
                    Value::String(name) => out.push(RawStateDef {
                        name: name.clone(),
                        parent: None,
                        initial_child: None,
                        is_history: false,
                        is_final: false,
                        parent_name: None,
                        initial_name: None,
                    }),
                    Value::Object(obj) => {
                        let name = obj
                            .get("name")
                            .and_then(|v| match &*v.borrow() {
                                Value::String(s) => Some(s.clone()),
                                _ => None,
                            })
                            .ok_or_else(|| type_err(span, "state object requires 'name'"))?;
                        let parent = obj.get("parent").and_then(|v| match &*v.borrow() {
                            Value::String(s) => Some(s.clone()),
                            _ => None,
                        });
                        let initial = obj.get("initial").and_then(|v| match &*v.borrow() {
                            Value::String(s) => Some(s.clone()),
                            _ => None,
                        });
                        let is_final = obj
                            .get("final")
                            .map(|v| matches!(&*v.borrow(), Value::Bool(true)))
                            .unwrap_or(false);
                        let is_history = obj
                            .get("history")
                            .map(|v| matches!(&*v.borrow(), Value::Bool(true)))
                            .unwrap_or(false);
                        out.push(RawStateDef {
                            name,
                            parent: None,
                            initial_child: None,
                            is_history,
                            is_final,
                            parent_name: parent,
                            initial_name: initial,
                        });
                    }
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "states entries must be string or object, got {}",
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
            format!("states must be an array, got {}", other.type_name()),
        )),
    }
}

fn resolve_state_refs(mut states: Vec<RawStateDef>, span: Span) -> NiaoResult<Vec<StateDef>> {
    let index: HashMap<String, usize> = states
        .iter()
        .enumerate()
        .map(|(i, s)| (s.name.clone(), i))
        .collect();

    for i in 0..states.len() {
        if let Some(pname) = states[i].parent_name.clone() {
            let pid = *index.get(&pname).ok_or_else(|| {
                type_err(span, format!("unknown parent state '{pname}'"))
            })? as u32;
            states[i].parent = Some(pid);
        }
        if let Some(iname) = states[i].initial_name.clone() {
            let cid = *index.get(&iname).ok_or_else(|| {
                type_err(span, format!("unknown initial child '{iname}'"))
            })? as u32;
            states[i].initial_child = Some(cid);
        }
    }

    Ok(states
        .into_iter()
        .map(|s| StateDef {
            name: s.name,
            parent: s.parent,
            initial_child: s.initial_child,
            is_history: s.is_history,
            is_final: s.is_final,
        })
        .collect())
}

fn parse_sources(
    v: &Value,
    state_index: &HashMap<String, u32>,
    span: Span,
) -> NiaoResult<TransitionSources> {
    match v {
        Value::String(s) if s == "*" => Ok(TransitionSources::Any),
        Value::String(s) => {
            let id = *state_index
                .get(s)
                .ok_or_else(|| type_err(span, format!("unknown source state '{s}'")))?;
            Ok(TransitionSources::One(id))
        }
        Value::Array(items) => {
            let mut ids = Vec::with_capacity(items.len());
            for item in items {
                match &*item.borrow() {
                    Value::String(s) => {
                        let id = *state_index
                            .get(s)
                            .ok_or_else(|| type_err(span, format!("unknown source state '{s}'")))?;
                        ids.push(id);
                    }
                    other => {
                        return Err(type_err(
                            span,
                            format!("source list entries must be strings, got {}", other.type_name()),
                        ));
                    }
                }
            }
            Ok(TransitionSources::Many(ids))
        }
        other => Err(type_err(
            span,
            format!("transition source must be string or array, got {}", other.type_name()),
        )),
    }
}

fn parse_dest(
    v: &Value,
    state_index: &HashMap<String, u32>,
    span: Span,
) -> NiaoResult<TransitionDest> {
    match v {
        Value::String(s) if s == "=" || s == "same" => Ok(TransitionDest::Same),
        Value::String(s) if s == "internal" || s == "." => Ok(TransitionDest::Internal),
        Value::String(s) if s.starts_with("hist:") => {
            let parent = &s[5..];
            let id = *state_index
                .get(parent)
                .ok_or_else(|| type_err(span, format!("unknown history parent '{parent}'")))?;
            Ok(TransitionDest::HistoryShallow(id))
        }
        Value::String(s) => {
            let id = *state_index
                .get(s)
                .ok_or_else(|| type_err(span, format!("unknown dest state '{s}'")))?;
            Ok(TransitionDest::State(id))
        }
        Value::Nil => Ok(TransitionDest::Internal),
        other => Err(type_err(
            span,
            format!("transition dest must be string, got {}", other.type_name()),
        )),
    }
}

fn parse_transitions(
    map: &HashMap<String, ValueRef>,
    state_index: &HashMap<String, u32>,
    trigger_index: &HashMap<String, u32>,
    span: Span,
) -> NiaoResult<(Vec<TransitionDef>, Vec<TransitionHooks>)> {
    let Some(tr_val) = map.get("transitions") else {
        return Ok((Vec::new(), Vec::new()));
    };
    match &*tr_val.borrow() {
        Value::Array(items) => {
            let mut transitions = Vec::with_capacity(items.len());
            let mut hooks = Vec::with_capacity(items.len());
            for item in items {
                let obj = match &*item.borrow() {
                    Value::Object(o) => o,
                    other => {
                        return Err(type_err(
                            span,
                            format!("transitions entries must be objects, got {}", other.type_name()),
                        ));
                    }
                };
                let trigger_name = obj
                    .get("trigger")
                    .or_else(|| obj.get("event"))
                    .and_then(|v| match &*v.borrow() {
                        Value::String(s) => Some(s.clone()),
                        _ => None,
                    })
                    .ok_or_else(|| type_err(span, "transition requires 'trigger'"))?;
                let trigger = *trigger_index.get(&trigger_name).ok_or_else(|| {
                    type_err(span, format!("unknown trigger '{trigger_name}'"))
                })?;

                let source_val = obj
                    .get("source")
                    .or_else(|| obj.get("from"))
                    .map(|v| v.borrow().clone())
                    .unwrap_or(Value::String("*".into()));
                let sources = parse_sources(&source_val, state_index, span)?;

                let internal = obj
                    .get("internal")
                    .map(|v| matches!(&*v.borrow(), Value::Bool(true)))
                    .unwrap_or(false);
                let dest = if internal {
                    TransitionDest::Internal
                } else {
                    let dest_val = obj
                        .get("dest")
                        .or_else(|| obj.get("to"))
                        .map(|v| v.borrow().clone())
                        .ok_or_else(|| type_err(span, "transition requires 'dest'"))?;
                    parse_dest(&dest_val, state_index, span)?
                };

                let priority = obj
                    .get("priority")
                    .and_then(|v| match &*v.borrow() {
                        Value::Int(n) => Some(*n as i32),
                        _ => None,
                    })
                    .unwrap_or(0);

                transitions.push(TransitionDef {
                    trigger,
                    sources,
                    dest,
                    priority,
                });
                hooks.push(TransitionHooks {
                    prepare: optional_callable(obj, "prepare"),
                    guard: optional_callable(obj, "guard").or_else(|| optional_callable(obj, "unless")),
                    guard_unless: optional_callable(obj, "unless").is_some()
                        && optional_callable(obj, "guard").is_none(),
                    on: optional_callable(obj, "on"),
                });
            }
            Ok((transitions, hooks))
        }
        Value::Nil => Ok((Vec::new(), Vec::new())),
        other => Err(type_err(
            span,
            format!("transitions must be an array, got {}", other.type_name()),
        )),
    }
}

fn parse_machine_hooks(map: &HashMap<String, ValueRef>) -> MachineHooks {
    let empty = HashMap::new();
    let hooks_map = map
        .get("hooks")
        .and_then(|v| match &*v.borrow() {
            Value::Object(o) => Some(o),
            _ => None,
        })
        .unwrap_or(&empty);

    let mut on_enter = HashMap::new();
    let mut on_exit = HashMap::new();
    if let Some(v) = hooks_map.get("on_enter").or_else(|| hooks_map.get("enter")) {
        if let Value::Object(m) = &*v.borrow() {
            for (k, f) in m {
                if is_callable(&f.borrow()) {
                    on_enter.insert(k.clone(), Rc::clone(f));
                }
            }
        }
    }
    if let Some(v) = hooks_map.get("on_exit").or_else(|| hooks_map.get("exit")) {
        if let Value::Object(m) = &*v.borrow() {
            for (k, f) in m {
                if is_callable(&f.borrow()) {
                    on_exit.insert(k.clone(), Rc::clone(f));
                }
            }
        }
    }

    MachineHooks {
        before: optional_callable(hooks_map, "before"),
        after: optional_callable(hooks_map, "after"),
        on_prepare: optional_callable(hooks_map, "on_prepare").or_else(|| optional_callable(hooks_map, "prepare")),
        on_transition: optional_callable(hooks_map, "on_transition")
            .or_else(|| optional_callable(hooks_map, "on")),
        on_enter,
        on_exit,
    }
}

fn build_spec(map: &HashMap<String, ValueRef>, span: Span) -> NiaoResult<(FsmSpec, Vec<TransitionHooks>, MachineHooks)> {
    let initial = map
        .get("initial")
        .and_then(|v| match &*v.borrow() {
            Value::String(s) => Some(s.clone()),
            _ => None,
        })
        .ok_or_else(|| type_err(span, "nfsm spec requires 'initial' state name"))?;

    let raw_states = parse_states(map, span)?;
    let states = resolve_state_refs(raw_states, span)?;

    let state_index: HashMap<String, u32> = states
        .iter()
        .enumerate()
        .map(|(i, s)| (s.name.clone(), i as u32))
        .collect();

    // Mark finals from top-level `final` array
    let mut states = states;
    if let Some(fv) = map.get("final").or_else(|| map.get("finals")) {
        if let Ok(names) = string_list(&fv.borrow(), "final", span) {
            for name in names {
                if let Some(&id) = state_index.get(&name) {
                    states[id as usize].is_final = true;
                }
            }
        }
    }

    let triggers = if let Some(tv) = map.get("triggers").or_else(|| map.get("events")) {
        string_list(&tv.borrow(), "triggers", span)?
    } else {
        // Infer triggers from transitions
        Vec::new()
    };

    let trigger_index: HashMap<String, u32> = if triggers.is_empty() {
        HashMap::new()
    } else {
        triggers
            .iter()
            .enumerate()
            .map(|(i, t)| (t.clone(), i as u32))
            .collect()
    };

    let (mut transitions, mut tr_hooks) = parse_transitions(map, &state_index, &trigger_index, span)?;

    // If triggers were empty, collect from transitions
    let triggers = if triggers.is_empty() {
        let mut names = Vec::new();
        let mut ti: HashMap<String, u32> = HashMap::new();
        if let Some(tr_val) = map.get("transitions") {
            if let Value::Array(items) = &*tr_val.borrow() {
                for item in items {
                    if let Value::Object(obj) = &*item.borrow() {
                        if let Some(v) = obj.get("trigger").or_else(|| obj.get("event")) {
                            if let Value::String(s) = &*v.borrow() {
                                if !ti.contains_key(s) {
                                    ti.insert(s.clone(), names.len() as u32);
                                    names.push(s.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
        // Re-parse transitions with inferred trigger index
        let (tr2, h2) = parse_transitions(map, &state_index, &ti, span)?;
        transitions = tr2;
        tr_hooks = h2;
        names
    } else {
        triggers
    };

    if triggers.is_empty() {
        return Err(type_err(span, "nfsm spec requires 'triggers' or transitions with trigger names"));
    }

    let hooks = parse_machine_hooks(map);
    let spec = FsmSpec::build(states, triggers, transitions, &initial)
        .map_err(|e| type_err(span, e))?;
    Ok((spec, tr_hooks, hooks))
}

fn event_object(
    handle: i64,
    trigger: &str,
    source: &str,
    dest: &str,
    transition_idx: usize,
) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("machine".to_string(), Value::Int(handle).ref_cell());
    map.insert("trigger".to_string(), Value::String(trigger.into()).ref_cell());
    map.insert("event".to_string(), Value::String(trigger.into()).ref_cell());
    map.insert("source".to_string(), Value::String(source.into()).ref_cell());
    map.insert("dest".to_string(), Value::String(dest.into()).ref_cell());
    map.insert(
        "transition".to_string(),
        Value::Int(transition_idx as i64).ref_cell(),
    );
    Value::Object(map).ref_cell()
}

fn state_name(spec: &FsmSpec, id: u32) -> String {
    spec.state_name(id).unwrap_or("?").to_string()
}

fn run_send(handle: i64, trigger_name: &str, span: Span) -> NiaoResult<ValueRef> {
    let trigger_id = with_machine(handle, span, |m| {
        m.engine
            .spec()
            .trigger_id(trigger_name)
            .ok_or_else(|| nfsm_err(span, format!("unknown trigger '{trigger_name}'")))
    })??;

    let candidates = with_machine(handle, span, |m| {
        m.engine
            .candidates(trigger_id)
            .map_err(|e| fsm_error_to_value(span, e))
    })??;

    if candidates.is_empty() {
        return nil_val();
    }

    for cand in candidates {
        let tr_hooks = with_machine(handle, span, |m| {
            m.transition_hooks
                .get(cand.index)
                .cloned()
                .unwrap_or(TransitionHooks {
                    prepare: None,
                    guard: None,
                    guard_unless: false,
                    on: None,
                })
        })??;

        let spec = with_machine(handle, span, |m| m.engine.spec().clone())??;
        let src = state_name(&spec, cand.from_leaf);
        let dst = state_name(&spec, cand.to_leaf);
        let trig = trigger_name.to_string();
        let event = event_object(handle, &trig, &src, &dst, cand.index);

        // Machine-level prepare
        let machine_hooks = with_machine(handle, span, |m| MachineHooks {
            before: m.hooks.before.clone(),
            after: m.hooks.after.clone(),
            on_prepare: m.hooks.on_prepare.clone(),
            on_transition: m.hooks.on_transition.clone(),
            on_enter: m.hooks.on_enter.clone(),
            on_exit: m.hooks.on_exit.clone(),
        })??;

        let ctx = with_machine(handle, span, |m| Rc::clone(&m.context))??;
        let call_args = |callee: &ValueRef| -> NiaoResult<bool> {
            invoke_bool(callee, &[Rc::clone(&ctx), Rc::clone(&event)], span)
        };

        if let Some(ref prep) = machine_hooks.on_prepare {
            if !call_args(prep)? {
                continue;
            }
        }
        if let Some(ref prep) = tr_hooks.prepare {
            if !call_args(prep)? {
                continue;
            }
        }
        if let Some(ref guard) = tr_hooks.guard {
            let mut ok = call_args(guard)?;
            if tr_hooks.guard_unless {
                ok = !ok;
            }
            if !ok {
                continue;
            }
        }
        if let Some(ref before) = machine_hooks.before {
            if !call_args(before)? {
                continue;
            }
        }

        // Apply transition and run exit/enter hooks
        let apply = with_machine_mut(handle, span, |m| {
            m.engine
                .apply(cand.index)
                .map_err(|e| fsm_error_to_value(span, e))
        })??;

        let spec = with_machine(handle, span, |m| m.engine.spec().clone())??;
        for sid in &apply.exited {
            let name = state_name(&spec, *sid);
            if let Some(hook) = machine_hooks.on_exit.get(&name) {
                let _ = invoke_callable(hook, &[Rc::clone(&ctx), Rc::clone(&event)], span)?;
            }
        }
        if let Some(ref on) = tr_hooks.on {
            let _ = invoke_callable(on, &[Rc::clone(&ctx), Rc::clone(&event)], span)?;
        }
        if let Some(ref on) = machine_hooks.on_transition {
            let _ = invoke_callable(on, &[Rc::clone(&ctx), Rc::clone(&event)], span)?;
        }
        for sid in &apply.entered {
            let name = state_name(&spec, *sid);
            if let Some(hook) = machine_hooks.on_enter.get(&name) {
                let _ = invoke_callable(hook, &[Rc::clone(&ctx), Rc::clone(&event)], span)?;
            }
        }
        if let Some(ref after) = machine_hooks.after {
            let _ = invoke_callable(after, &[Rc::clone(&ctx), Rc::clone(&event)], span)?;
        }

        let mut result = HashMap::new();
        result.insert("ok".to_string(), Value::Bool(true).ref_cell());
        result.insert(
            "source".to_string(),
            Value::String(src).ref_cell(),
        );
        result.insert(
            "dest".to_string(),
            Value::String(state_name(&spec, apply.to_leaf)).ref_cell(),
        );
        result.insert(
            "trigger".to_string(),
            Value::String(trig).ref_cell(),
        );
        return Ok(Value::Object(result).ref_cell());
    }

    Ok(nfsm_err(
        span,
        format!("no transition for trigger '{trigger_name}' (guards rejected all candidates)"),
    ))
}

// ---------------------------------------------------------------------------
// Public builtins
// ---------------------------------------------------------------------------

// >>> let h = nfsm_new({initial: "idle", states: ["idle", "on"], triggers: ["go"], transitions: [{trigger: "go", source: "idle", dest: "on"}]})
// => handle int
fn nfsm_new(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfsm_new", span)?;
    let map = match &*args[0].borrow() {
        Value::Object(m) => m,
        other => {
            return Err(type_err(
                span,
                format!("nfsm_new() expects a spec object, got {}", other.type_name()),
            ));
        }
    };
    let (spec, tr_hooks, hooks) = build_spec(map, span)?;
    let mut engine = FsmEngine::new(spec).map_err(|e| type_err(span, e.to_string()))?;
    if let Some(v) = map.get("ignore_invalid") {
        if let Value::Bool(b) = &*v.borrow() {
            engine.set_ignore_invalid(*b);
        }
    }
    let context = map
        .get("context")
        .or_else(|| map.get("model"))
        .cloned()
        .unwrap_or_else(|| Value::Object(HashMap::new()).ref_cell());
    let id = register(MachineInstance {
        engine,
        context,
        hooks,
        transition_hooks: tr_hooks,
    });
    Ok(Value::Int(id).ref_cell())
}

// >>> nfsm_close(h)
// => true
fn nfsm_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfsm_close", span)?;
    let id = handle_arg(args, 0, "nfsm_close", span)?;
    let removed = MACHINES.with(|m| m.borrow_mut().remove(&id).is_some());
    bool_val(removed)
}

// >>> nfsm_send(h, "go")
// => {ok, source, dest, trigger}
fn nfsm_send(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nfsm_send", span)?;
    let id = handle_arg(args, 0, "nfsm_send", span)?;
    let trigger = str_arg(args, 1, "nfsm_send", span)?;
    run_send(id, &trigger, span)
}

// >>> nfsm_trigger(h, "go")
// => same as send
fn nfsm_trigger(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    nfsm_send(args, span)
}

// >>> nfsm_state(h)
// => "idle"
fn nfsm_state(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfsm_state", span)?;
    let id = handle_arg(args, 0, "nfsm_state", span)?;
    match with_machine(id, span, |m| {
        state_name(m.engine.spec(), m.engine.current())
    })? {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(e),
    }
}

// >>> nfsm_states(h)
// => ["work", "busy"]  // leaf-to-root active hierarchy
fn nfsm_states(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfsm_states", span)?;
    let id = handle_arg(args, 0, "nfsm_states", span)?;
    match with_machine(id, span, |m| {
        m.engine
            .current_states()
            .into_iter()
            .map(|s| Value::String(state_name(m.engine.spec(), s)).ref_cell())
            .collect::<Vec<_>>()
    })? {
        Ok(items) => Ok(Value::Array(items).ref_cell()),
        Err(e) => Ok(e),
    }
}

// >>> nfsm_is_state(h, "idle")
// => true
fn nfsm_is_state(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nfsm_is_state", span)?;
    let id = handle_arg(args, 0, "nfsm_is_state", span)?;
    let name = str_arg(args, 1, "nfsm_is_state", span)?;
    match with_machine(id, span, |m| {
        m.engine
            .current_states()
            .iter()
            .any(|s| m.engine.spec().state_name(*s) == Some(name.as_str()))
    })? {
        Ok(b) => bool_val(b),
        Err(e) => Ok(e),
    }
}

// >>> nfsm_is_final(h)
// => false
fn nfsm_is_final(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfsm_is_final", span)?;
    let id = handle_arg(args, 0, "nfsm_is_final", span)?;
    match with_machine(id, span, |m| m.engine.is_final())? {
        Ok(b) => bool_val(b),
        Err(e) => Ok(e),
    }
}

// >>> nfsm_can(h, "go")
// => true
fn nfsm_can(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nfsm_can", span)?;
    let id = handle_arg(args, 0, "nfsm_can", span)?;
    let trigger = str_arg(args, 1, "nfsm_can", span)?;
    match with_machine(id, span, |m| {
        m.engine
            .spec()
            .trigger_id(&trigger)
            .and_then(|tid| m.engine.candidates(tid).ok())
            .map(|c| !c.is_empty())
            .unwrap_or(false)
    })? {
        Ok(b) => bool_val(b),
        Err(e) => Ok(e),
    }
}

// >>> nfsm_triggers(h)
// => ["go", "stop"]
fn nfsm_triggers(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfsm_triggers", span)?;
    let id = handle_arg(args, 0, "nfsm_triggers", span)?;
    match with_machine(id, span, |m| {
        m.engine
            .available_triggers()
            .into_iter()
            .filter_map(|tid| {
                m.engine
                    .spec()
                    .trigger_name(tid)
                    .map(|s| Value::String(s.to_string()).ref_cell())
            })
            .collect::<Vec<_>>()
    })? {
        Ok(items) => Ok(Value::Array(items).ref_cell()),
        Err(e) => Ok(e),
    }
}

// >>> nfsm_events(h) — alias for triggers
fn nfsm_events(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    nfsm_triggers(args, span)
}

// >>> nfsm_reset(h)
// => true
fn nfsm_reset(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfsm_reset", span)?;
    let id = handle_arg(args, 0, "nfsm_reset", span)?;
    match with_machine_mut(id, span, |m| {
        m.engine.reset();
        true
    })? {
        Ok(b) => bool_val(b),
        Err(e) => Ok(e),
    }
}

// >>> nfsm_context(h)
// => object
fn nfsm_context(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfsm_context", span)?;
    let id = handle_arg(args, 0, "nfsm_context", span)?;
    match with_machine(id, span, |m| Rc::clone(&m.context))? {
        Ok(ctx) => Ok(ctx),
        Err(e) => Ok(e),
    }
}

// >>> nfsm_set_context(h, ctx)
// => ctx
fn nfsm_set_context(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nfsm_set_context", span)?;
    let id = handle_arg(args, 0, "nfsm_set_context", span)?;
    let ctx = Rc::clone(&args[1]);
    match with_machine_mut(id, span, |m| {
        m.context = Rc::clone(&ctx);
        Rc::clone(&ctx)
    })? {
        Ok(c) => Ok(c),
        Err(e) => Ok(e),
    }
}

// >>> nfsm_history(h)
// => [{trigger, source, dest}, ...]
fn nfsm_history(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfsm_history", span)?;
    let id = handle_arg(args, 0, "nfsm_history", span)?;
    match with_machine(id, span, |m| {
        m.engine
            .history()
            .iter()
            .map(|rec| {
                let mut obj = HashMap::new();
                obj.insert(
                    "trigger".to_string(),
                    Value::String(
                        m.engine
                            .spec()
                            .trigger_name(rec.trigger)
                            .unwrap_or("?")
                            .into(),
                    )
                    .ref_cell(),
                );
                obj.insert(
                    "source".to_string(),
                    Value::String(state_name(m.engine.spec(), rec.from)).ref_cell(),
                );
                obj.insert(
                    "dest".to_string(),
                    Value::String(state_name(m.engine.spec(), rec.to)).ref_cell(),
                );
                Value::Object(obj).ref_cell()
            })
            .collect::<Vec<_>>()
    })? {
        Ok(items) => Ok(Value::Array(items).ref_cell()),
        Err(e) => Ok(e),
    }
}

// >>> nfsm_clear_history(h)
// => true
fn nfsm_clear_history(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfsm_clear_history", span)?;
    let id = handle_arg(args, 0, "nfsm_clear_history", span)?;
    match with_machine_mut(id, span, |m| {
        m.engine.clear_history();
        true
    })? {
        Ok(b) => bool_val(b),
        Err(e) => Ok(e),
    }
}

// >>> nfsm_count(h)
// => 3
fn nfsm_count(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfsm_count", span)?;
    let id = handle_arg(args, 0, "nfsm_count", span)?;
    match with_machine(id, span, |m| m.engine.transition_count())? {
        Ok(n) => Ok(Value::Int(n as i64).ref_cell()),
        Err(e) => Ok(e),
    }
}

// >>> nfsm_ignore_invalid(h, true)
// => true
fn nfsm_ignore_invalid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nfsm_ignore_invalid", span)?;
    let id = handle_arg(args, 0, "nfsm_ignore_invalid", span)?;
    let v = bool_arg(args, 1, "nfsm_ignore_invalid", span)?;
    match with_machine_mut(id, span, |m| {
        m.engine.set_ignore_invalid(v);
        true
    })? {
        Ok(b) => bool_val(b),
        Err(e) => Ok(e),
    }
}

// >>> nfsm_dot(h)
// => "digraph fsm { ... }"
fn nfsm_dot(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfsm_dot", span)?;
    let id = handle_arg(args, 0, "nfsm_dot", span)?;
    match with_machine(id, span, |m| m.engine.to_dot())? {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(e),
    }
}

// >>> nfsm_info(h)
// => {state, states, triggers, final, count}
fn nfsm_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfsm_info", span)?;
    let id = handle_arg(args, 0, "nfsm_info", span)?;
    match with_machine(id, span, |m| {
        let spec = m.engine.spec();
        let mut map = HashMap::new();
        map.insert(
            "state".to_string(),
            Value::String(state_name(spec, m.engine.current())).ref_cell(),
        );
        map.insert(
            "states".to_string(),
            Value::Array(
                m.engine
                    .current_states()
                    .into_iter()
                    .map(|s| Value::String(state_name(spec, s)).ref_cell())
                    .collect(),
            )
            .ref_cell(),
        );
        map.insert(
            "triggers".to_string(),
            Value::Array(
                m.engine
                    .available_triggers()
                    .into_iter()
                    .filter_map(|tid| {
                        spec.trigger_name(tid)
                            .map(|s| Value::String(s.to_string()).ref_cell())
                    })
                    .collect(),
            )
            .ref_cell(),
        );
        map.insert(
            "final".to_string(),
            Value::Bool(m.engine.is_final()).ref_cell(),
        );
        map.insert(
            "count".to_string(),
            Value::Int(m.engine.transition_count() as i64).ref_cell(),
        );
        map
    })? {
        Ok(map) => Ok(Value::Object(map).ref_cell()),
        Err(e) => Ok(e),
    }
}

// >>> nfsm_validate(spec)
// => true | nfsm_error
fn nfsm_validate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfsm_validate", span)?;
    let map = match &*args[0].borrow() {
        Value::Object(m) => m,
        other => {
            return Err(type_err(
                span,
                format!("nfsm_validate() expects a spec object, got {}", other.type_name()),
            ));
        }
    };
    match build_spec(map, span) {
        Ok(_) => bool_val(true),
        Err(e) => Ok(nfsm_err(span, e.to_string())),
    }
}

// ---------------------------------------------------------------------------
// Module exports
// ---------------------------------------------------------------------------

macro_rules! nfsm_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nfsm_fns![
    ("nfsm_new", "new", nfsm_new),
    ("nfsm_close", "close", nfsm_close),
    ("nfsm_send", "send", nfsm_send),
    ("nfsm_trigger", "trigger", nfsm_trigger),
    ("nfsm_state", "state", nfsm_state),
    ("nfsm_states", "states", nfsm_states),
    ("nfsm_is_state", "is_state", nfsm_is_state),
    ("nfsm_is_final", "is_final", nfsm_is_final),
    ("nfsm_can", "can", nfsm_can),
    ("nfsm_triggers", "triggers", nfsm_triggers),
    ("nfsm_events", "events", nfsm_events),
    ("nfsm_reset", "reset", nfsm_reset),
    ("nfsm_context", "context", nfsm_context),
    ("nfsm_set_context", "set_context", nfsm_set_context),
    ("nfsm_history", "history", nfsm_history),
    ("nfsm_clear_history", "clear_history", nfsm_clear_history),
    ("nfsm_count", "count", nfsm_count),
    ("nfsm_ignore_invalid", "ignore_invalid", nfsm_ignore_invalid),
    ("nfsm_dot", "dot", nfsm_dot),
    ("nfsm_info", "info", nfsm_info),
    ("nfsm_validate", "validate", nfsm_validate),
];

pub const MODULE_NAME: &str = "nfsm";
pub const MODULE_PATHS: &[&str] = &["nfsm", "std/nfsm"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    fn spec_obj() -> ValueRef {
        let mut tr = HashMap::new();
        tr.insert("trigger".to_string(), Value::String("go".into()).ref_cell());
        tr.insert("source".to_string(), Value::String("idle".into()).ref_cell());
        tr.insert("dest".to_string(), Value::String("on".into()).ref_cell());
        let mut spec = HashMap::new();
        spec.insert("initial".to_string(), Value::String("idle".into()).ref_cell());
        spec.insert(
            "states".to_string(),
            Value::Array(vec![
                Value::String("idle".into()).ref_cell(),
                Value::String("on".into()).ref_cell(),
            ])
            .ref_cell(),
        );
        spec.insert(
            "triggers".to_string(),
            Value::Array(vec![Value::String("go".into()).ref_cell()]).ref_cell(),
        );
        spec.insert(
            "transitions".to_string(),
            Value::Array(vec![Value::Object(tr).ref_cell()]).ref_cell(),
        );
        Value::Object(spec).ref_cell()
    }

    #[test]
    fn new_send_state() {
        let h = nfsm_new(&[spec_obj()], span()).unwrap();
        let id = match &*h.borrow() {
            Value::Int(n) => *n,
            _ => panic!("expected handle"),
        };
        let r = nfsm_send(&[h, Value::String("go".into()).ref_cell()], span()).unwrap();
        match &*r.borrow() {
            Value::Object(m) => {
                assert!(matches!(&*m.get("ok").unwrap().borrow(), Value::Bool(true)));
            }
            other => panic!("expected object result, got {other:?}"),
        }
        let st = nfsm_state(&[Value::Int(id).ref_cell()], span()).unwrap();
        assert_eq!(*st.borrow(), Value::String("on".into()));
        nfsm_close(&[Value::Int(id).ref_cell()], span()).unwrap();
    }
}
