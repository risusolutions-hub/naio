//! Native nrepl standard library — subprocess expression evaluation sessions.
//! A `--watch-expr` CLI flag for in-process eval is a roadmap item (see docs/NREPL.md).
//!
//! Import with `import "nrepl"` (or `import "std/nrepl"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::rc::Rc;
use std::time::Duration;

const E3270_NREPL_ARITY: u32 = 3270;
const E3271_NREPL_ERROR: u32 = 3271;
const E3272_NREPL_TYPE: u32 = 3272;

// ---------------------------------------------------------------------------
// Session model
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct EvalRecord {
    expr: String,
    stdout: String,
    stderr: String,
    code: i64,
    ok: bool,
}

struct Session {
    cwd: PathBuf,
    mode: String,
    timeout_ms: Option<u64>,
    history: Vec<EvalRecord>,
}

thread_local! {
    static SESSIONS: RefCell<HashMap<i64, Session>> = RefCell::new(HashMap::new());
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

fn with_session<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut Session) -> Result<T, ValueRef>,
) -> NiaoResult<Result<T, ValueRef>> {
    SESSIONS.with(|sessions| {
        let mut sessions = sessions.borrow_mut();
        match sessions.get_mut(&id) {
            Some(s) => Ok(f(s)),
            None => Ok(Err(invalid_session(span, id))),
        }
    })
}

// ---------------------------------------------------------------------------
// Subprocess helpers
// ---------------------------------------------------------------------------

#[cfg(windows)]
const PATHEXT: &[&str] = &[".COM", ".EXE", ".BAT", ".CMD"];

fn is_executable(path: &Path) -> bool {
    path.is_file()
}

fn which_path(name: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        let direct = dir.join(name);
        if is_executable(&direct) {
            return Some(direct);
        }
        #[cfg(windows)]
        {
            for ext in PATHEXT {
                let candidate = dir.join(format!("{name}{ext}"));
                if is_executable(&candidate) {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

fn niao_binary_name() -> &'static str {
    if cfg!(windows) {
        "niao.exe"
    } else {
        "niao"
    }
}

fn find_niao_binary() -> Option<PathBuf> {
    if let Ok(exe) = env::current_exe() {
        if exe.file_name().and_then(|n| n.to_str()) == Some(niao_binary_name()) {
            return Some(exe);
        }
        if let Some(parent) = exe.parent() {
            let sibling = parent.join(niao_binary_name());
            if sibling.exists() {
                return Some(sibling);
            }
        }
    }
    if let Ok(custom) = env::var("NIAO_BIN") {
        let p = PathBuf::from(custom);
        if p.exists() {
            return Some(p);
        }
    }
    which_path(niao_binary_name())
}

fn wrap_expr(expr: &str) -> String {
    format!(
        "// nrepl generated\nfn main() {{\n    print({expr})\n}}\n"
    )
}

fn run_niao(
    binary: &Path,
    script: &Path,
    cwd: &Path,
    mode: &str,
    timeout_ms: Option<u64>,
) -> Result<Output, String> {
    let mut cmd = Command::new(binary);
    cmd.arg("run")
        .arg(script)
        .arg("--mode")
        .arg(mode)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to spawn niao: {e}"))?;
    if let Some(ms) = timeout_ms {
        let started = std::time::Instant::now();
        loop {
            if let Ok(Some(status)) = child.try_wait() {
                let output = child
                    .wait_with_output()
                    .map_err(|e| format!("wait failed: {e}"))?;
                let _ = status;
                return Ok(output);
            }
            if started.elapsed() >= Duration::from_millis(ms) {
                let _ = child.kill();
                return Err(format!("eval timed out after {ms} ms"));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    child
        .wait_with_output()
        .map_err(|e| format!("wait failed: {e}"))
}

fn eval_result_object(record: &EvalRecord) -> Value {
    let mut map = HashMap::new();
    map.insert("expr".to_string(), Value::String(record.expr.clone()).ref_cell());
    map.insert(
        "stdout".to_string(),
        Value::String(record.stdout.clone()).ref_cell(),
    );
    map.insert(
        "stderr".to_string(),
        Value::String(record.stderr.clone()).ref_cell(),
    );
    map.insert("code".to_string(), Value::Int(record.code).ref_cell());
    map.insert("ok".to_string(), Value::Bool(record.ok).ref_cell());
    Value::Object(map)
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3272_NREPL_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3270_NREPL_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3270_NREPL_ARITY,
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

fn repl_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3271_NREPL_ERROR, "nrepl_error", msg.into(), span)
}

fn invalid_session(span: Span, id: i64) -> ValueRef {
    error_value(
        E3271_NREPL_ERROR,
        "nrepl_error",
        format!("invalid or closed nrepl session {id}"),
        span,
    )
}

#[derive(Default)]
struct StartOpts {
    cwd: PathBuf,
    mode: String,
    timeout_ms: Option<u64>,
}

fn parse_start_opts(opts: &ValueRef, span: Span) -> NiaoResult<StartOpts> {
    match &*opts.borrow() {
        Value::Object(map) => {
            let mut out = StartOpts {
                cwd: env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
                mode: "interp".to_string(),
                timeout_ms: None,
            };
            if let Some(cwd_ref) = map.get("cwd") {
                match &*cwd_ref.borrow() {
                    Value::String(s) => out.cwd = PathBuf::from(s),
                    other => {
                        return Err(type_err(
                            span,
                            format!("opts.cwd must be a string, got {}", other.type_name()),
                        ));
                    }
                }
            }
            if let Some(mode_ref) = map.get("mode") {
                match &*mode_ref.borrow() {
                    Value::String(s) if s == "interp" || s == "vm" => out.mode = s.clone(),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "opts.mode must be \"interp\" or \"vm\", got {}",
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            if let Some(t_ref) = map.get("timeout_ms") {
                match &*t_ref.borrow() {
                    Value::Int(n) if *n >= 0 => out.timeout_ms = Some(*n as u64),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "opts.timeout_ms must be a non-negative int, got {}",
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
            format!("opts must be an object, got {}", other.type_name()),
        )),
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn nrepl_start(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nrepl_start", span)?;
    let opts = if args.is_empty() {
        StartOpts {
            cwd: env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            mode: "interp".to_string(),
            timeout_ms: None,
        }
    } else {
        parse_start_opts(&args[0], span)?
    };
    let id = new_handle();
    SESSIONS.with(|s| {
        s.borrow_mut().insert(
            id,
            Session {
                cwd: opts.cwd,
                mode: opts.mode,
                timeout_ms: opts.timeout_ms,
                history: Vec::new(),
            },
        );
    });
    Ok(Value::Int(id).ref_cell())
}

fn nrepl_eval(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nrepl_eval", span)?;
    let id = int_arg(args, 0, "nrepl_eval", span)?;
    let expr = string_arg(args, 1, "nrepl_eval", span)?;
    if expr.trim().is_empty() {
        return Ok(repl_err(span, "nrepl_eval() expression must be non-empty"));
    }
    let binary = match find_niao_binary() {
        Some(p) => p,
        None => {
            return Ok(repl_err(
                span,
                "nrepl_eval() could not find niao binary (set NIAO_BIN or add niao to PATH)",
            ));
        }
    };

    match with_session(id, span, |session| {
        let mut script_path = env::temp_dir();
        script_path.push(format!(
            "nrepl_{}_{}.niao",
            std::process::id(),
            session.history.len()
        ));
        let source = wrap_expr(&expr);
        if let Err(e) = fs::File::create(&script_path).and_then(|mut f| f.write_all(source.as_bytes()))
        {
            return Err(repl_err(
                span,
                format!("nrepl_eval() failed to write temp script: {e}"),
            ));
        }

        let output = match run_niao(
            &binary,
            &script_path,
            &session.cwd,
            &session.mode,
            session.timeout_ms,
        ) {
            Ok(o) => o,
            Err(msg) => {
                let _ = fs::remove_file(&script_path);
                return Err(repl_err(span, msg));
            }
        };
        let _ = fs::remove_file(&script_path);

        let stdout = String::from_utf8_lossy(&output.stdout).trim_end().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim_end().to_string();
        let code = output.status.code().unwrap_or(-1) as i64;
        let ok = output.status.success();
        let record = EvalRecord {
            expr: expr.clone(),
            stdout,
            stderr,
            code,
            ok,
        };
        let result = eval_result_object(&record);
        session.history.push(record);
        Ok(result.ref_cell())
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

fn nrepl_history(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrepl_history", span)?;
    let id = int_arg(args, 0, "nrepl_history", span)?;
    match with_session(id, span, |session| -> Result<Value, ValueRef> {
        let items: Vec<ValueRef> = session
            .history
            .iter()
            .map(|r| eval_result_object(r).ref_cell())
            .collect();
        Ok(Value::Array(items))
    })? {
        Ok(v) => Ok(v.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nrepl_cwd(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrepl_cwd", span)?;
    let id = int_arg(args, 0, "nrepl_cwd", span)?;
    match with_session(id, span, |session| Ok(session.cwd.clone()))? {
        Ok(p) => Ok(Value::String(p.to_string_lossy().into_owned()).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nrepl_len(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrepl_len", span)?;
    let id = int_arg(args, 0, "nrepl_len", span)?;
    match with_session(id, span, |session| Ok(session.history.len() as i64))? {
        Ok(n) => Ok(Value::Int(n).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nrepl_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrepl_close", span)?;
    let id = int_arg(args, 0, "nrepl_close", span)?;
    let removed = SESSIONS.with(|s| s.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

fn nrepl_available(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nrepl_available", span)?;
    Ok(Value::Bool(find_niao_binary().is_some()).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nrepl_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nrepl_fns![
    ("nrepl_start", "start", nrepl_start),
    ("nrepl_eval", "eval", nrepl_eval),
    ("nrepl_history", "history", nrepl_history),
    ("nrepl_cwd", "cwd", nrepl_cwd),
    ("nrepl_len", "len", nrepl_len),
    ("nrepl_close", "close", nrepl_close),
    ("nrepl_available", "available", nrepl_available),
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

pub const MODULE_NAME: &str = "nrepl";
pub const MODULE_PATHS: &[&str] = &["nrepl", "std/nrepl"];

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
        assert!(matches!(&*v.borrow(), Value::Int(_)));
        v
    }

    #[test]
    fn start_close_and_len() {
        let h = handle(nrepl_start(&[], span()));
        assert!(matches!(
            &*nrepl_len(&[h.clone()], span()).unwrap().borrow(),
            Value::Int(0)
        ));
        assert!(matches!(
            &*nrepl_close(&[h.clone()], span()).unwrap().borrow(),
            Value::Bool(true)
        ));
        let err = nrepl_len(&[h], span()).unwrap();
        assert!(matches!(
            &*err.borrow(),
            Value::Error(e) if e.code == E3271_NREPL_ERROR
        ));
    }

    #[test]
    fn eval_when_niao_available() {
        if !find_niao_binary().is_some() {
            return;
        }
        let h = handle(nrepl_start(&[], span()));
        let result = nrepl_eval(&[h.clone(), s("1 + 2")], span()).unwrap();
        match &*result.borrow() {
            Value::Object(map) => {
                assert!(matches!(&*map["ok"].borrow(), Value::Bool(true)));
                let stdout = match &*map["stdout"].borrow() {
                    Value::String(s) => s.trim().to_string(),
                    other => panic!("expected stdout, got {other:?}"),
                };
                assert_eq!(stdout, "3");
            }
            other => panic!("expected object, got {other:?}"),
        }
        assert!(matches!(
            &*nrepl_len(&[h.clone()], span()).unwrap().borrow(),
            Value::Int(1)
        ));
        let hist = nrepl_history(&[h.clone()], span()).unwrap();
        match &*hist.borrow() {
            Value::Array(items) => assert_eq!(items.len(), 1),
            other => panic!("expected array, got {other:?}"),
        }
        nrepl_close(&[h], span()).unwrap();
    }

    #[test]
    fn empty_expr_is_catchable_error() {
        let h = handle(nrepl_start(&[], span()));
        let v = nrepl_eval(&[h.clone(), s("   ")], span()).unwrap();
        match &*v.borrow() {
            Value::Error(e) => assert_eq!(e.code, E3271_NREPL_ERROR),
            other => panic!("expected error, got {other:?}"),
        }
        nrepl_close(&[h], span()).unwrap();
    }

    #[test]
    fn arity_and_type_errors() {
        let err = nrepl_eval(&[s("not-int"), s("1")], span()).unwrap_err();
        assert_eq!(err.code(), E3272_NREPL_TYPE);
        let err = nrepl_start(&[i(1), i(2)], span()).unwrap_err();
        assert_eq!(err.code(), E3270_NREPL_ARITY);
    }

    #[test]
    fn namespace_has_expected_methods() {
        match namespace() {
            Value::Object(map) => {
                for key in ["start", "eval", "history", "cwd", "len", "close", "available"] {
                    assert!(map.contains_key(key), "missing {key}");
                }
            }
            other => panic!("expected object, got {other:?}"),
        }
        assert_eq!(builtins().len(), 7);
    }
}
