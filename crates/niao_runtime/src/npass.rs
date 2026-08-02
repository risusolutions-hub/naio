//! Native `npass` standard library — password hashing (argon2id, bcrypt, scrypt)
//! and strength policy checks (~passlib, argon2-cffi, bcrypt subset).
//!
//! Import with `import "npass"` (or `import "std/npass"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_pass::{
    argon2, bcrypt, check_strength, generate, identify, is_common_password, scrypt, Argon2Opts,
    CryptContext, Policy, Scheme, ScryptOpts, VerifyUpdateResult, DEFAULT_ALPHABET, DEFAULT_COST,
    DEFAULT_SCHEME, MAX_COST, MAX_PASSWORD_BYTES, MIN_COST,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E3567_NPASS_ARITY: u32 = codes::E3567_NPASS_ARITY;
const E3568_NPASS_ERROR: u32 = codes::E3568_NPASS_ERROR;
const E3569_NPASS_TYPE: u32 = codes::E3569_NPASS_TYPE;
const E3570_NPASS_INVALID_HANDLE: u32 = codes::E3570_NPASS_INVALID_HANDLE;
const E3571_NPASS_SCHEME: u32 = codes::E3571_NPASS_SCHEME;

enum NpassHandle {
    Context(CryptContext),
    Policy(Policy),
}

thread_local! {
    static HANDLES: RefCell<HashMap<i64, NpassHandle>> = RefCell::new(HashMap::new());
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

fn register(handle: NpassHandle) -> i64 {
    let id = new_handle();
    HANDLES.with(|m| m.borrow_mut().insert(id, handle));
    id
}

fn with_handle<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut NpassHandle) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    HANDLES.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(h) => Ok(Ok(f(h))),
            None => Ok(Err(error_value(
                E3570_NPASS_INVALID_HANDLE,
                "npass_error",
                format!("invalid or closed npass handle {id}"),
                span,
            ))),
        }
    })
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3569_NPASS_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3567_NPASS_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3567_NPASS_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn npass_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3568_NPASS_ERROR, "npass_error", msg.into(), span)
}

fn scheme_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3571_NPASS_SCHEME, "npass_error", msg.into(), span)
}

fn str_val(s: impl Into<String>) -> ValueRef {
    Value::String(s.into()).ref_cell()
}

fn int_val(n: i64) -> ValueRef {
    Value::Int(n).ref_cell()
}

fn bool_val(b: bool) -> ValueRef {
    Value::Bool(b).ref_cell()
}

fn float_val(n: f64) -> ValueRef {
    Value::Float(n).ref_cell()
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

fn optional_string(args: &[ValueRef], idx: usize) -> Option<String> {
    if args.len() <= idx {
        return None;
    }
    match &*args[idx].borrow() {
        Value::String(s) => Some(s.clone()),
        Value::Nil => None,
        _ => None,
    }
}

fn optional_object(args: &[ValueRef], idx: usize) -> Option<HashMap<String, ValueRef>> {
    if args.len() <= idx {
        return None;
    }
    match &*args[idx].borrow() {
        Value::Object(map) => Some(map.clone()),
        Value::Nil => None,
        _ => None,
    }
}

fn obj_bool(map: Option<&HashMap<String, ValueRef>>, key: &str, default: bool) -> bool {
    map.get(key)
        .and_then(|v| match &*v.borrow() {
            Value::Bool(b) => Some(*b),
            Value::Int(n) => Some(*n != 0),
            _ => None,
        })
        .unwrap_or(default)
}

fn obj_int(map: Option<&HashMap<String, ValueRef>>, key: &str, default: i64) -> i64 {
    map.get(key)
        .and_then(|v| match &*v.borrow() {
            Value::Int(n) => Some(*n),
            _ => None,
        })
        .unwrap_or(default)
}

fn obj_float(map: Option<&HashMap<String, ValueRef>>, key: &str, default: f64) -> f64 {
    map.get(key)
        .and_then(|v| match &*v.borrow() {
            Value::Float(n) => Some(*n),
            Value::Int(n) => Some(*n as f64),
            _ => None,
        })
        .unwrap_or(default)
}

fn string_list(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Vec<String> {
    let Some(map) = map else {
        return Vec::new();
    };
    let Some(v) = map.get(key) else {
        return Vec::new();
    };
    match &*v.borrow() {
        Value::Array(items) => items
            .iter()
            .filter_map(|item| match &*item.borrow() {
                Value::String(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn scheme_list(map: Option<&HashMap<String, ValueRef>>, key: &str, default: &[Scheme]) -> Result<Vec<Scheme>, String> {
    let strings = string_list(map, key);
    if strings.is_empty() {
        return Ok(default.to_vec());
    }
    strings
        .iter()
        .map(|s| Scheme::parse(s).map_err(|e| e.message()))
        .collect()
}

fn parse_scheme(name: &str, span: Span) -> Result<Scheme, ValueRef> {
    Scheme::parse(name).map_err(|e| scheme_err(span, e.message()))
}

fn argon2_opts_from_map(map: Option<&HashMap<String, ValueRef>>) -> Result<Argon2Opts, ValueRef> {
    Argon2Opts::from_map(
        {
            let m = obj_int(map, "memory_kib", -1);
            if m >= 0 { Some(m as u32) } else { None }
        },
        {
            let t = obj_int(map, "time_cost", -1);
            if t >= 0 { Some(t as u32) } else { None }
        },
        {
            let p = obj_int(map, "parallelism", -1);
            if p >= 0 { Some(p as u32) } else { None }
        },
    )
    .map_err(|e| npass_err(Span::dummy(), e.message()))
}

fn scrypt_opts_from_map(map: Option<&HashMap<String, ValueRef>>) -> Result<ScryptOpts, ValueRef> {
    ScryptOpts::from_map(
        {
            let n = obj_int(map, "log_n", -1);
            if n >= 0 { Some(n as u8) } else { None }
        },
        {
            let r = obj_int(map, "r", -1);
            if r >= 0 { Some(r as u32) } else { None }
        },
        {
            let p = obj_int(map, "p", -1);
            if p >= 0 { Some(p as u32) } else { None }
        },
    )
    .map_err(|e| npass_err(Span::dummy(), e.message()))
}

fn context_from_map(map: Option<&HashMap<String, ValueRef>>, span: Span) -> Result<CryptContext, ValueRef> {
    let schemes = scheme_list(map, "schemes", Scheme::ALL)
        .map_err(|e| scheme_err(span, e))?;
    let deprecated = scheme_list(map, "deprecated", &[])
        .map_err(|e| scheme_err(span, e))?;
    let default_name = optional_string_from_map(map, "default", "argon2id");
    let default_scheme = Scheme::parse(&default_name).map_err(|e| scheme_err(span, e.message()))?;
    let argon2 = argon2_opts_from_map(map)?;
    let scrypt = scrypt_opts_from_map(map)?;
    let bcrypt_cost = obj_int(map, "bcrypt_cost", DEFAULT_COST as i64) as u32;
    Ok(CryptContext {
        default_scheme: default_scheme,
        schemes,
        deprecated,
        argon2,
        bcrypt_cost,
        scrypt,
    })
}

fn optional_string_from_map(map: Option<&HashMap<String, ValueRef>>, key: &str, default: &str) -> String {
    map.and_then(|m| m.get(key))
        .and_then(|v| match &*v.borrow() {
            Value::String(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_else(|| default.to_string())
}

fn policy_from_map(map: Option<&HashMap<String, ValueRef>>) -> Policy {
    Policy {
        min_length: obj_int(map, "min_length", 8).max(0) as usize,
        max_length: obj_int(map, "max_length", 128).max(1) as usize,
        min_upper: obj_int(map, "min_upper", 0).max(0) as usize,
        min_lower: obj_int(map, "min_lower", 0).max(0) as usize,
        min_digit: obj_int(map, "min_digit", 0).max(0) as usize,
        min_special: obj_int(map, "min_special", 0).max(0) as usize,
        min_entropy: obj_float(map, "min_entropy", 0.0),
        min_score: obj_int(map, "min_score", 0).clamp(0, 4) as u8,
        forbid_common: obj_bool(map, "forbid_common", true),
        forbid_sequential: obj_bool(map, "forbid_sequential", true),
        forbid_repeated: obj_bool(map, "forbid_repeated", true),
        forbidden: string_list(map, "forbidden"),
    }
}

fn strength_object(report: niao_pass::StrengthReport) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("ok".into(), bool_val(report.ok));
    map.insert("score".into(), int_val(report.score as i64));
    map.insert("entropy".into(), float_val(report.entropy));
    map.insert("length".into(), int_val(report.length as i64));
    let issues: Vec<ValueRef> = report.issues.into_iter().map(str_val).collect();
    map.insert("issues".into(), Value::Array(issues).ref_cell());
    let mut classes = HashMap::new();
    classes.insert("upper".into(), int_val(report.classes.upper as i64));
    classes.insert("lower".into(), int_val(report.classes.lower as i64));
    classes.insert("digit".into(), int_val(report.classes.digit as i64));
    classes.insert("special".into(), int_val(report.classes.special as i64));
    classes.insert("other".into(), int_val(report.classes.other as i64));
    map.insert("classes".into(), Value::Object(classes).ref_cell());
    Value::Object(map).ref_cell()
}

fn verify_update_object(result: VerifyUpdateResult) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("valid".into(), bool_val(result.valid));
    map.insert(
        "new_hash".into(),
        match result.new_hash {
            Some(h) => str_val(h),
            None => Value::Nil.ref_cell(),
        },
    );
    map.insert(
        "scheme".into(),
        match result.scheme {
            Some(s) => str_val(s.as_str()),
            None => Value::Nil.ref_cell(),
        },
    );
    Value::Object(map).ref_cell()
}

fn handle_id_from_arg(args: &[ValueRef], idx: usize, span: Span, name: &str) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Object(map) => match map.get("id") {
            Some(v) => match &*v.borrow() {
                Value::Int(n) => Ok(*n),
                other => Err(type_err(
                    span,
                    format!("{name}() handle id must be int, got {}", other.type_name()),
                )),
            },
            None => Err(type_err(span, format!("{name}() object missing id field"))),
        },
        Value::Int(n) => Ok(*n),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects handle object as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn handle_object(id: i64, kind: &str, methods: HashMap<String, ValueRef>) -> ValueRef {
    let mut map = methods;
    map.insert("id".into(), int_val(id));
    map.insert("kind".into(), str_val(kind));
    Value::Object(map).ref_cell()
}

fn map_pass_err(span: Span, err: niao_pass::PassError) -> ValueRef {
    let code = match &err {
        niao_pass::PassError::UnknownScheme(_) | niao_pass::PassError::UnsupportedScheme(_) => {
            E3571_NPASS_SCHEME
        }
        _ => E3568_NPASS_ERROR,
    };
    error_value(code, "npass_error", err.message(), span)
}

// >>> import "npass"
// >>> type(npass.hash("secret"))
// "string"
fn npass_hash(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "npass_hash", span)?;
    let password = string_arg(args, 0, "npass_hash", span)?;
    let scheme = if let Some(s) = optional_string(args, 1) {
        Some(parse_scheme(&s, span)?)
    } else {
        None
    };
    let ctx = context_from_map(optional_object(args, 2), span)?;
    match ctx.hash(&password, scheme) {
        Ok(h) => Ok(str_val(h)),
        Err(e) => Ok(map_pass_err(span, e)),
    }
}

// >>> npass.verify("secret", npass.bcrypt_hash("secret", 4))
// true
fn npass_verify(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "npass_verify", span)?;
    let password = string_arg(args, 0, "npass_verify", span)?;
    let hash = string_arg(args, 1, "npass_verify", span)?;
    match niao_pass::verify_password(&password, &hash) {
        Ok(ok) => Ok(bool_val(ok)),
        Err(e) => Ok(map_pass_err(span, e)),
    }
}

// >>> npass.identify(npass.bcrypt_hash("x", 4))
// "bcrypt"
fn npass_identify(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "npass_identify", span)?;
    let hash = string_arg(args, 0, "npass_identify", span)?;
    Ok(match identify(&hash) {
        Some(s) => str_val(s.as_str()),
        None => Value::Nil.ref_cell(),
    })
}

// >>> type(npass.needs_update(npass.bcrypt_hash("x", 4), {bcrypt_cost: 12}))
// "bool"
fn npass_needs_update(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "npass_needs_update", span)?;
    let hash = string_arg(args, 0, "npass_needs_update", span)?;
    let ctx = context_from_map(optional_object(args, 1), span)?;
    match ctx.needs_update(&hash) {
        Ok(v) => Ok(bool_val(v)),
        Err(e) => Ok(map_pass_err(span, e)),
    }
}

// >>> npass.verify_and_update("secret", h).valid
// true
fn npass_verify_and_update(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "npass_verify_and_update", span)?;
    let password = string_arg(args, 0, "npass_verify_and_update", span)?;
    let hash = string_arg(args, 1, "npass_verify_and_update", span)?;
    let ctx = context_from_map(optional_object(args, 2), span)?;
    match ctx.verify_and_update(&password, &hash) {
        Ok(r) => Ok(verify_update_object(r)),
        Err(e) => Ok(map_pass_err(span, e)),
    }
}

// >>> type(npass.context().hash("secret"))
// "string"
fn npass_context(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "npass_context", span)?;
    let ctx = context_from_map(optional_object(args, 0), span)?;
    let id = register(NpassHandle::Context(ctx));
    let mut methods = HashMap::new();
    methods.insert("hash".into(), Value::NativeFunction(Rc::new(npass_context_hash)).ref_cell());
    methods.insert("verify".into(), Value::NativeFunction(Rc::new(npass_context_verify)).ref_cell());
    methods.insert(
        "verify_and_update".into(),
        Value::NativeFunction(Rc::new(npass_context_verify_and_update)).ref_cell(),
    );
    methods.insert(
        "needs_update".into(),
        Value::NativeFunction(Rc::new(npass_context_needs_update)).ref_cell(),
    );
    Ok(handle_object(id, "context", methods))
}

fn npass_context_hash(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if args.len() < 2 {
        return Err(RuntimeError::at(
            span,
            E3567_NPASS_ARITY,
            format!("context.hash() expects handle and password, got {}", args.len()),
        ));
    }
    let handle_id = handle_id_from_arg(args, 0, span, "context.hash")?;
    let password = string_arg(args, 1, "context.hash", span)?;
    let scheme = if args.len() >= 3 {
        optional_string(args, 2).map(|s| parse_scheme(&s, span)).transpose()?
    } else {
        None
    };
    match with_handle(handle_id, span, |h| {
        if let NpassHandle::Context(ctx) = h {
            ctx.hash(&password, scheme)
        } else {
            Err(niao_pass::PassError::InvalidParameter("invalid context handle".into()))
        }
    })? {
        Ok(Ok(h)) => Ok(str_val(h)),
        Ok(Err(e)) => Ok(map_pass_err(span, e)),
        Err(e) => Ok(e),
    }
}

fn npass_context_verify(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if args.len() != 3 {
        return Err(RuntimeError::at(
            span,
            E3567_NPASS_ARITY,
            format!("context.verify() expects handle, password, hash; got {}", args.len()),
        ));
    }
    let handle_id = handle_id_from_arg(args, 0, span, "context.verify")?;
    let password = string_arg(args, 1, "context.verify", span)?;
    let hash = string_arg(args, 2, "context.verify", span)?;
    match with_handle(handle_id, span, |h| {
        if let NpassHandle::Context(ctx) = h {
            ctx.verify(&password, &hash)
        } else {
            Err(niao_pass::PassError::InvalidParameter("invalid context handle".into()))
        }
    })? {
        Ok(Ok(v)) => Ok(bool_val(v)),
        Ok(Err(e)) => Ok(map_pass_err(span, e)),
        Err(e) => Ok(e),
    }
}

fn npass_context_verify_and_update(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if args.len() != 3 {
        return Err(RuntimeError::at(
            span,
            E3567_NPASS_ARITY,
            format!(
                "context.verify_and_update() expects handle, password, hash; got {}",
                args.len()
            ),
        ));
    }
    let handle_id = handle_id_from_arg(args, 0, span, "context.verify_and_update")?;
    let password = string_arg(args, 1, "context.verify_and_update", span)?;
    let hash = string_arg(args, 2, "context.verify_and_update", span)?;
    match with_handle(handle_id, span, |h| {
        if let NpassHandle::Context(ctx) = h {
            ctx.verify_and_update(&password, &hash)
        } else {
            Err(niao_pass::PassError::InvalidParameter("invalid context handle".into()))
        }
    })? {
        Ok(Ok(r)) => Ok(verify_update_object(r)),
        Ok(Err(e)) => Ok(map_pass_err(span, e)),
        Err(e) => Ok(e),
    }
}

fn npass_context_needs_update(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if args.len() != 2 {
        return Err(RuntimeError::at(
            span,
            E3567_NPASS_ARITY,
            format!("context.needs_update() expects handle and hash, got {}", args.len()),
        ));
    }
    let handle_id = handle_id_from_arg(args, 0, span, "context.needs_update")?;
    let hash = string_arg(args, 1, "context.needs_update", span)?;
    match with_handle(handle_id, span, |h| {
        if let NpassHandle::Context(ctx) = h {
            ctx.needs_update(&hash)
        } else {
            Err(niao_pass::PassError::InvalidParameter("invalid context handle".into()))
        }
    })? {
        Ok(Ok(v)) => Ok(bool_val(v)),
        Ok(Err(e)) => Ok(map_pass_err(span, e)),
        Err(e) => Ok(e),
    }
}

// >>> type(npass.argon2_hash("secret"))
// "string"
fn npass_argon2_hash(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "npass_argon2_hash", span)?;
    let password = string_arg(args, 0, "npass_argon2_hash", span)?;
    let opts = argon2_opts_from_map(optional_object(args, 1))?;
    match argon2::hash_password(&password, &opts) {
        Ok(h) => Ok(str_val(h)),
        Err(e) => Ok(map_pass_err(span, e)),
    }
}

// >>> type(npass.bcrypt_hash("secret", 4))
// "string"
fn npass_bcrypt_hash(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "npass_bcrypt_hash", span)?;
    let password = string_arg(args, 0, "npass_bcrypt_hash", span)?;
    let cost = if args.len() == 2 {
        int_arg(args, 1, "npass_bcrypt_hash", span)? as u32
    } else {
        DEFAULT_COST
    };
    match bcrypt::hash_password(&password, cost) {
        Ok(h) => Ok(str_val(h)),
        Err(e) => Ok(map_pass_err(span, e)),
    }
}

// >>> type(npass.scrypt_hash("secret"))
// "string"
fn npass_scrypt_hash(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "npass_scrypt_hash", span)?;
    let password = string_arg(args, 0, "npass_scrypt_hash", span)?;
    let opts = scrypt_opts_from_map(optional_object(args, 1))?;
    match scrypt::hash_password(&password, &opts) {
        Ok(h) => Ok(str_val(h)),
        Err(e) => Ok(map_pass_err(span, e)),
    }
}

// >>> npass.check("password").ok
// false
fn npass_check(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "npass_check", span)?;
    let password = string_arg(args, 0, "npass_check", span)?;
    let policy = policy_from_map(optional_object(args, 1));
    if password.is_empty() {
        return Ok(map_pass_err(span, niao_pass::PassError::EmptyPassword));
    }
    if password.len() > MAX_PASSWORD_BYTES {
        return Ok(map_pass_err(
            span,
            niao_pass::PassError::PasswordTooLong {
                max: MAX_PASSWORD_BYTES,
            },
        ));
    }
    Ok(strength_object(policy.validate(&password)))
}

// >>> type(npass.policy().validate("Tr0ub4dor&3"))
// "object"
fn npass_policy(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "npass_policy", span)?;
    let policy = policy_from_map(optional_object(args, 0));
    let id = register(NpassHandle::Policy(policy));
    let mut methods = HashMap::new();
    methods.insert("validate".into(), Value::NativeFunction(Rc::new(npass_policy_validate)).ref_cell());
    methods.insert("check".into(), Value::NativeFunction(Rc::new(npass_policy_validate)).ref_cell());
    Ok(handle_object(id, "policy", methods))
}

fn npass_policy_validate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if args.len() != 2 {
        return Err(RuntimeError::at(
            span,
            E3567_NPASS_ARITY,
            format!("policy.validate() expects handle and password, got {}", args.len()),
        ));
    }
    let handle_id = handle_id_from_arg(args, 0, span, "policy.validate")?;
    let password = string_arg(args, 1, "policy.validate", span)?;
    match with_handle(handle_id, span, |h| {
        if let NpassHandle::Policy(p) = h {
            Some(p.validate(&password))
        } else {
            None
        }
    })? {
        Ok(Some(r)) => Ok(strength_object(r)),
        Ok(None) => Ok(npass_err(span, "invalid policy handle")),
        Err(e) => Ok(e),
    }
}

// >>> npass.entropy("abc") > 0
// true
fn npass_entropy(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "npass_entropy", span)?;
    let password = string_arg(args, 0, "npass_entropy", span)?;
    match check_strength(&password) {
        Ok(r) => Ok(float_val(r.entropy)),
        Err(e) => Ok(map_pass_err(span, e)),
    }
}

// >>> npass.is_common("password")
// true
fn npass_is_common(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "npass_is_common", span)?;
    let password = string_arg(args, 0, "npass_is_common", span)?;
    Ok(bool_val(is_common_password(&password)))
}

// >>> len(npass.generate(16))
// 16
fn npass_generate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 2, "npass_generate", span)?;
    let length = if args.is_empty() {
        16usize
    } else {
        let n = int_arg(args, 0, "npass_generate", span)?;
        if n <= 0 {
            return Ok(npass_err(span, "length must be > 0"));
        }
        n as usize
    };
    let alphabet = optional_string(args, 1);
    match generate(length, alphabet.as_deref()) {
        Ok(p) => Ok(str_val(p)),
        Err(e) => Ok(map_pass_err(span, e)),
    }
}

macro_rules! npass_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

npass_fns![
    ("npass_hash", "hash", npass_hash),
    ("npass_verify", "verify", npass_verify),
    ("npass_identify", "identify", npass_identify),
    ("npass_needs_update", "needs_update", npass_needs_update),
    ("npass_verify_and_update", "verify_and_update", npass_verify_and_update),
    ("npass_context", "context", npass_context),
    ("npass_argon2_hash", "argon2_hash", npass_argon2_hash),
    ("npass_bcrypt_hash", "bcrypt_hash", npass_bcrypt_hash),
    ("npass_scrypt_hash", "scrypt_hash", npass_scrypt_hash),
    ("npass_check", "check", npass_check),
    ("npass_policy", "policy", npass_policy),
    ("npass_entropy", "entropy", npass_entropy),
    ("npass_is_common", "is_common", npass_is_common),
    ("npass_generate", "generate", npass_generate),
];

pub const MODULE_NAME: &str = "npass";
pub const MODULE_PATHS: &[&str] = &["npass", "std/npass"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    map.insert("DEFAULT_SCHEME".into(), str_val(DEFAULT_SCHEME.as_str()));
    map.insert("DEFAULT_BCRYPT_COST".into(), int_val(DEFAULT_COST as i64));
    map.insert("MIN_BCRYPT_COST".into(), int_val(MIN_COST as i64));
    map.insert("MAX_BCRYPT_COST".into(), int_val(MAX_COST as i64));
    map.insert("MAX_PASSWORD_BYTES".into(), int_val(MAX_PASSWORD_BYTES as i64));
    map.insert("DEFAULT_ALPHABET".into(), str_val(DEFAULT_ALPHABET));
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::dummy()
    }

    #[test]
    fn bcrypt_roundtrip() {
        let h = npass_bcrypt_hash(
            &[Value::String("secret".into()).ref_cell(), Value::Int(4).ref_cell()],
            span(),
        )
        .unwrap();
        let ok = npass_verify(
            &[
                Value::String("secret".into()).ref_cell(),
                h,
            ],
            span(),
        )
        .unwrap();
        match &*ok.borrow() {
            Value::Bool(true) => {}
            other => panic!("expected true, got {other:?}"),
        }
    }
}
