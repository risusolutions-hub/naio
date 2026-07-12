//! Native nshell standard library — subprocess execution with captured output,
//! optional shell wrapping, timeouts, and PATH lookup (`which`).
//!
//! Import with `import "nshell"` (or `import "std/nshell"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::rc::Rc;
use std::time::Duration;

// codes.rs integration pending — use local constants until wired.
const E2930_NSHELL_ARITY: u32 = 2930;
const E2931_NSHELL_ERROR: u32 = 2931;
const E2932_NSHELL_TYPE: u32 = 2932;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::TypeError {
        message: msg.into(),
        line: span.line,
        col: span.col,
    }
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E2930_NSHELL_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E2930_NSHELL_ARITY,
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

fn nshell_error(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E2931_NSHELL_ERROR, "nshell_error", msg.into(), span)
}

fn ok_string(s: impl Into<String>) -> ValueRef {
    Value::String(s.into()).ref_cell()
}

fn ok_bool(b: bool) -> ValueRef {
    Value::Bool(b).ref_cell()
}

fn ok_nil() -> ValueRef {
    Value::Nil.ref_cell()
}

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

#[derive(Default)]
struct RunOpts {
    cwd: Option<PathBuf>,
    env: HashMap<String, String>,
    timeout_ms: Option<u64>,
    shell: bool,
}

fn parse_opts(opts: &ValueRef, span: Span) -> NiaoResult<RunOpts> {
    match &*opts.borrow() {
        Value::Object(map) => {
            let mut out = RunOpts::default();
            if let Some(cwd_ref) = map.get("cwd") {
                match &*cwd_ref.borrow() {
                    Value::String(s) => out.cwd = Some(PathBuf::from(s)),
                    other => {
                        return Err(RuntimeError::at(
                            span,
                            E2932_NSHELL_TYPE,
                            format!("opts.cwd must be a string, got {}", other.type_name()),
                        ));
                    }
                }
            }
            if let Some(env_ref) = map.get("env") {
                match &*env_ref.borrow() {
                    Value::Object(env_map) => {
                        for (k, v) in env_map {
                            match &*v.borrow() {
                                Value::String(s) => {
                                    out.env.insert(k.clone(), s.clone());
                                }
                                other => {
                                    return Err(RuntimeError::at(
                                        span,
                                        E2932_NSHELL_TYPE,
                                        format!(
                                            "opts.env values must be strings, got {} for '{k}'",
                                            other.type_name()
                                        ),
                                    ));
                                }
                            }
                        }
                    }
                    other => {
                        return Err(RuntimeError::at(
                            span,
                            E2932_NSHELL_TYPE,
                            format!("opts.env must be an object, got {}", other.type_name()),
                        ));
                    }
                }
            }
            if let Some(t_ref) = map.get("timeout_ms") {
                match &*t_ref.borrow() {
                    Value::Int(n) if *n >= 0 => out.timeout_ms = Some(*n as u64),
                    other => {
                        return Err(RuntimeError::at(
                            span,
                            E2932_NSHELL_TYPE,
                            format!(
                                "opts.timeout_ms must be a non-negative int, got {}",
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            if let Some(s_ref) = map.get("shell") {
                match &*s_ref.borrow() {
                    Value::Bool(b) => out.shell = *b,
                    other => {
                        return Err(RuntimeError::at(
                            span,
                            E2932_NSHELL_TYPE,
                            format!("opts.shell must be a bool, got {}", other.type_name()),
                        ));
                    }
                }
            }
            Ok(out)
        }
        other => Err(RuntimeError::at(
            span,
            E2932_NSHELL_TYPE,
            format!(
                "opts must be an object, got {}",
                other.type_name()
            ),
        )),
    }
}

fn apply_opts(mut cmd: Command, opts: &RunOpts) -> Command {
    if let Some(cwd) = &opts.cwd {
        cmd.current_dir(cwd);
    }
    for (k, v) in &opts.env {
        cmd.env(k, v);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

fn build_command(cmd: &ValueRef, opts: &RunOpts, span: Span) -> NiaoResult<Command> {
    match &*cmd.borrow() {
        Value::String(s) => {
            let base = if opts.shell {
                #[cfg(windows)]
                {
                    let mut c = Command::new("cmd");
                    c.args(["/C", s]);
                    c
                }
                #[cfg(not(windows))]
                {
                    let mut c = Command::new("sh");
                    c.args(["-c", s]);
                    c
                }
            } else {
                Command::new(s)
            };
            Ok(apply_opts(base, opts))
        }
        Value::Array(items) => {
            if items.is_empty() {
                return Err(RuntimeError::at(
                    span,
                    E2932_NSHELL_TYPE,
                    "command array must not be empty",
                ));
            }
            let program = match &*items[0].borrow() {
                Value::String(s) => s.clone(),
                other => {
                    return Err(RuntimeError::at(
                        span,
                        E2932_NSHELL_TYPE,
                        format!(
                            "command array[0] must be a string, got {}",
                            other.type_name()
                        ),
                    ));
                }
            };
            let mut c = Command::new(&program);
            for (i, item) in items.iter().enumerate().skip(1) {
                match &*item.borrow() {
                    Value::String(s) => c.arg(s),
                    other => {
                        return Err(RuntimeError::at(
                            span,
                            E2932_NSHELL_TYPE,
                            format!(
                                "command array[{}] must be a string, got {}",
                                i,
                                other.type_name()
                            ),
                        ));
                    }
                };
            }
            Ok(apply_opts(c, opts))
        }
        other => Err(RuntimeError::at(
            span,
            E2932_NSHELL_TYPE,
            format!(
                "command must be a string or array, got {}",
                other.type_name()
            ),
        )),
    }
}

fn lossy_utf8(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn run_command(mut cmd: Command, timeout_ms: Option<u64>) -> Result<Output, String> {
    if let Some(ms) = timeout_ms {
        if ms == 0 {
            return cmd.output().map_err(|e| e.to_string());
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = cmd.output();
            let _ = tx.send(result);
        });
        match rx.recv_timeout(Duration::from_millis(ms)) {
            Ok(Ok(out)) => Ok(out),
            Ok(Err(e)) => Err(e.to_string()),
            Err(_) => Err(format!("command timed out after {ms}ms")),
        }
    } else {
        cmd.output().map_err(|e| e.to_string())
    }
}

fn result_object(stdout: String, stderr: String, code: i64, ok: bool) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("stdout".to_string(), Value::String(stdout).ref_cell());
    map.insert("stderr".to_string(), Value::String(stderr).ref_cell());
    map.insert("code".to_string(), Value::Int(code).ref_cell());
    map.insert("ok".to_string(), Value::Bool(ok).ref_cell());
    Value::Object(map).ref_cell()
}

fn run_impl(cmd: &ValueRef, opts: &RunOpts, span: Span) -> NiaoResult<ValueRef> {
    let command = build_command(cmd, opts, span)?;
    match run_command(command, opts.timeout_ms) {
        Ok(output) => {
            let code = output.status.code().unwrap_or(-1) as i64;
            Ok(result_object(
                lossy_utf8(&output.stdout),
                lossy_utf8(&output.stderr),
                code,
                output.status.success(),
            ))
        }
        Err(e) => Ok(nshell_error(span, e)),
    }
}

// ---------------------------------------------------------------------------
// PATH lookup
// ---------------------------------------------------------------------------

#[cfg(windows)]
const PATHEXT: &[&str] = &[".EXE", ".CMD", ".BAT", ".COM"];

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

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// nshell_run(cmd, opts?) → {stdout, stderr, code, ok}
fn nshell_run(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nshell_run", span)?;
    let opts = if args.len() == 2 {
        parse_opts(&args[1], span)?
    } else {
        RunOpts::default()
    };
    run_impl(&args[0], &opts, span)
}

/// nshell_run_capture(cmd) → stdout string
fn nshell_run_capture(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nshell_run_capture", span)?;
    let result = run_impl(&args[0], &RunOpts::default(), span)?;
    let is_error = matches!(&*result.borrow(), Value::Error(_));
    if is_error {
        return Ok(result);
    }
    let borrowed = result.borrow();
    match &*borrowed {
        Value::Object(map) => {
            if let Some(stdout) = map.get("stdout") {
                Ok(Rc::clone(stdout))
            } else {
                Ok(Value::String(String::new()).ref_cell())
            }
        }
        other => Ok(other.clone().ref_cell()),
    }
}

/// nshell_which(name) → path string or nil
fn nshell_which(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nshell_which", span)?;
    let name = string_arg(args, 0, "nshell_which", span)?;
    match which_path(&name) {
        Some(p) => Ok(ok_string(p.to_string_lossy().into_owned())),
        None => Ok(ok_nil()),
    }
}

/// nshell_exists(cmd) → bool (whether `which` would succeed)
fn nshell_exists(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nshell_exists", span)?;
    let name = string_arg(args, 0, "nshell_exists", span)?;
    Ok(ok_bool(which_path(&name).is_some()))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nshell_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nshell_fns![
    ("nshell_run", "run", nshell_run),
    ("nshell_run_capture", "run_capture", nshell_run_capture),
    ("nshell_which", "which", nshell_which),
    ("nshell_exists", "exists", nshell_exists),
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

pub const MODULE_NAME: &str = "nshell";
pub const MODULE_PATHS: &[&str] = &["nshell", "std/nshell"];

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

    #[test]
    fn run_shell_echo() {
        let cmd = Value::String("echo hello".into()).ref_cell();
        let mut opts = HashMap::new();
        opts.insert("shell".to_string(), Value::Bool(true).ref_cell());
        let result = nshell_run(&[cmd, Value::Object(opts).ref_cell()], span()).unwrap();
        match &*result.borrow() {
            Value::Object(map) => {
                let stdout = match &*map["stdout"].borrow() {
                    Value::String(s) => s.trim().to_string(),
                    other => panic!("expected stdout string, got {other:?}"),
                };
                assert!(stdout.contains("hello"));
                assert!(matches!(&*map["ok"].borrow(), Value::Bool(true)));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn run_capture_returns_stdout() {
        let cmd = Value::Array(vec![
            Value::String("niao".into()).ref_cell(),
            Value::String("--version".into()).ref_cell(),
        ])
        .ref_cell();
        let via_run = nshell_run(&[Rc::clone(&cmd)], span()).unwrap();
        let via_capture = nshell_run_capture(&[cmd], span()).unwrap();
        if let (Value::Object(run_map), Value::String(cap)) =
            (&*via_run.borrow(), &*via_capture.borrow())
        {
            if matches!(&*run_map["ok"].borrow(), Value::Bool(true)) {
                let run_out = match &*run_map["stdout"].borrow() {
                    Value::String(s) => s.clone(),
                    _ => String::new(),
                };
                assert_eq!(cap, &run_out);
            }
        }
    }

    #[test]
    #[cfg(not(windows))]
    fn which_finds_echo() {
        let name = Value::String("echo".into()).ref_cell();
        let result = nshell_which(&[name], span()).unwrap();
        match &*result.borrow() {
            Value::String(path) => assert!(!path.is_empty()),
            Value::Nil => panic!("expected echo in PATH on unix"),
            other => panic!("expected string or nil, got {other:?}"),
        }
        let exists = nshell_exists(&[Value::String("echo".into()).ref_cell()], span()).unwrap();
        assert!(matches!(&*exists.borrow(), Value::Bool(true)));
    }

    #[test]
    #[cfg(windows)]
    fn which_finds_cmd() {
        let name = Value::String("cmd".into()).ref_cell();
        let result = nshell_which(&[name], span()).unwrap();
        match &*result.borrow() {
            Value::String(path) => {
                let lower = path.to_lowercase();
                assert!(lower.ends_with("cmd.exe"));
            }
            other => panic!("expected cmd.exe path, got {other:?}"),
        }
        let exists = nshell_exists(&[Value::String("cmd".into()).ref_cell()], span()).unwrap();
        assert!(matches!(&*exists.borrow(), Value::Bool(true)));
    }

    #[test]
    fn which_missing_returns_nil() {
        let name = Value::String("niao-definitely-not-a-real-binary-xyz".into()).ref_cell();
        let result = nshell_which(&[name], span()).unwrap();
        assert!(matches!(&*result.borrow(), Value::Nil));
        let exists =
            nshell_exists(&[Value::String("niao-definitely-not-a-real-binary-xyz".into()).ref_cell()], span())
                .unwrap();
        assert!(matches!(&*exists.borrow(), Value::Bool(false)));
    }

    #[test]
    fn arity_errors() {
        let err = nshell_run(&[], span()).unwrap_err();
        assert!(err.to_string().contains("expects 1..=2"));
        let err = nshell_which(&[], span()).unwrap_err();
        assert!(err.to_string().contains("expects 1"));
    }
}
