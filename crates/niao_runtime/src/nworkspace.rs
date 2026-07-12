//! Native nworkspace standard library — workspace manifest, member dependency
//! graph, topological order, and subprocess run.
//!
//! Import with `import "nworkspace"` (or `import "std/nworkspace"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_json_core::toml::parse_to_value;
use niao_json_core::{Number as JNumber, Value as JsonValue};
use std::collections::{HashMap, HashSet, VecDeque};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::rc::Rc;

const E3230_NWORKSPACE_ARITY: u32 = 3230;
const E3231_NWORKSPACE_ERROR: u32 = 3231;
const E3232_NWORKSPACE_TYPE: u32 = 3232;

// ---------------------------------------------------------------------------
// Workspace model
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
struct Member {
    name: String,
    path: PathBuf,
    entry: String,
    depends: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Workspace {
    name: String,
    root: PathBuf,
    manifest: Option<PathBuf>,
    members: Vec<Member>,
}

// ---------------------------------------------------------------------------
// TOML bridge
// ---------------------------------------------------------------------------

fn json_to_value(j: JsonValue) -> Value {
    match j {
        JsonValue::Null => Value::Nil,
        JsonValue::Bool(b) => Value::Bool(b),
        JsonValue::Number(n) => match n {
            JNumber::I64(i) => Value::Int(i),
            JNumber::U64(u) if u <= i64::MAX as u64 => Value::Int(u as i64),
            JNumber::U64(u) => Value::String(u.to_string()),
            JNumber::F64(f) => Value::Float(f),
        },
        JsonValue::String(s) => Value::String(s),
        JsonValue::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(json_to_value(item).ref_cell());
            }
            Value::Array(out)
        }
        JsonValue::Object(map) => {
            let mut out = HashMap::with_capacity(map.len());
            for (k, v) in map.iter() {
                out.insert(k.to_string(), json_to_value(v.clone()).ref_cell());
            }
            Value::Object(out)
        }
    }
}

fn parse_toml_text(text: &str, span: Span) -> NiaoResult<Value> {
    parse_to_value(text)
        .map(json_to_value)
        .map_err(|e| {
            RuntimeError::at(
                span,
                E3231_NWORKSPACE_ERROR,
                format!("workspace parse error: {e}"),
            )
        })
}

fn string_field(map: &HashMap<String, ValueRef>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.clone()),
        _ => None,
    })
}

fn string_list(map: &HashMap<String, ValueRef>, key: &str) -> Vec<String> {
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|v| match &*v.borrow() {
                Value::String(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn parse_member_table(
    table: &HashMap<String, ValueRef>,
    root: &Path,
    span: Span,
) -> Result<Member, ValueRef> {
    let name = string_field(table, "name").ok_or_else(|| {
        workspace_err(span, "member table requires string field 'name'")
    })?;
    let rel = string_field(table, "path").ok_or_else(|| {
        workspace_err(span, format!("member '{name}' requires string field 'path'"))
    })?;
    let entry = string_field(table, "entry").unwrap_or_else(|| "main.niao".to_string());
    let depends = string_list(table, "depends");
    Ok(Member {
        name,
        path: root.join(rel),
        entry,
        depends,
    })
}

fn parse_workspace_value(
    value: &Value,
    root: PathBuf,
    manifest: Option<PathBuf>,
    span: Span,
) -> Result<Workspace, ValueRef> {
    let Value::Object(map) = value else {
        return Err(workspace_err(
            span,
            format!(
                "workspace manifest must be a TOML table, got {}",
                value.type_name()
            ),
        ));
    };
    let name = string_field(map, "name").unwrap_or_else(|| "workspace".to_string());
    let members_tables = match map.get("members").map(|v| v.borrow().clone()) {
        Some(Value::Array(items)) => items,
        _ => {
            return Err(workspace_err(
                span,
                "workspace manifest requires [[members]] array",
            ));
        }
    };
    let mut members = Vec::new();
    for item in members_tables {
        let Value::Object(table) = item.borrow().clone() else {
            return Err(workspace_err(span, "each member must be a table/object"));
        };
        members.push(parse_member_table(&table, &root, span)?);
    }
    if members.is_empty() {
        return Err(workspace_err(span, "workspace must declare at least one member"));
    }
    let names: HashSet<&str> = members.iter().map(|m| m.name.as_str()).collect();
    for m in &members {
        for dep in &m.depends {
            if !names.contains(dep.as_str()) {
                return Err(workspace_err(
                    span,
                    format!("member '{}' depends on unknown member '{dep}'", m.name),
                ));
            }
        }
    }
    Ok(Workspace {
        name,
        root,
        manifest,
        members,
    })
}

fn workspace_to_value(ws: &Workspace) -> Value {
    let members: Vec<ValueRef> = ws
        .members
        .iter()
        .map(|m| {
            let mut map = HashMap::new();
            map.insert("name".to_string(), Value::String(m.name.clone()).ref_cell());
            map.insert(
                "path".to_string(),
                Value::String(m.path.to_string_lossy().into_owned()).ref_cell(),
            );
            map.insert("entry".to_string(), Value::String(m.entry.clone()).ref_cell());
            let deps: Vec<ValueRef> = m
                .depends
                .iter()
                .map(|d| Value::String(d.clone()).ref_cell())
                .collect();
            map.insert("depends".to_string(), Value::Array(deps).ref_cell());
            Value::Object(map).ref_cell()
        })
        .collect();
    let mut map = HashMap::new();
    map.insert("name".to_string(), Value::String(ws.name.clone()).ref_cell());
    map.insert(
        "root".to_string(),
        Value::String(ws.root.to_string_lossy().into_owned()).ref_cell(),
    );
    if let Some(manifest) = &ws.manifest {
        map.insert(
            "manifest".to_string(),
            Value::String(manifest.to_string_lossy().into_owned()).ref_cell(),
        );
    } else {
        map.insert("manifest".to_string(), Value::Nil.ref_cell());
    }
    map.insert("members".to_string(), Value::Array(members).ref_cell());
    Value::Object(map)
}

fn workspace_from_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> Result<Workspace, ValueRef> {
    match &*args[idx].borrow() {
        Value::Object(map) => {
            let root = string_field(map, "root")
                .map(PathBuf::from)
                .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
            let manifest = string_field(map, "manifest").map(PathBuf::from);
            parse_workspace_value(&Value::Object(map.clone()), root, manifest, span)
        }
        other => Err(workspace_err(
            span,
            format!(
                "{name}() expects a workspace object as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn topo_order(ws: &Workspace, span: Span) -> Result<Vec<String>, ValueRef> {
    let mut indegree: HashMap<&str, usize> = HashMap::new();
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for m in &ws.members {
        indegree.insert(m.name.as_str(), m.depends.len());
        for dep in &m.depends {
            adj.entry(dep.as_str()).or_default().push(m.name.as_str());
        }
    }
    let mut queue: VecDeque<&str> = ws
        .members
        .iter()
        .filter(|m| m.depends.is_empty())
        .map(|m| m.name.as_str())
        .collect();
    queue.make_contiguous().sort();
    let mut out = Vec::new();
    while let Some(name) = queue.pop_front() {
        out.push(name.to_string());
        if let Some(children) = adj.get(name) {
            let mut next: Vec<&str> = Vec::new();
            for child in children {
                let entry = indegree.get_mut(child).unwrap();
                *entry -= 1;
                if *entry == 0 {
                    next.push(*child);
                }
            }
            next.sort();
            for child in next {
                queue.push_back(child);
            }
        }
    }
    if out.len() != ws.members.len() {
        return Err(workspace_err(span, "workspace member graph has a cycle"));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Subprocess run
// ---------------------------------------------------------------------------

#[cfg(windows)]
const PATHEXT: &[&str] = &[".COM", ".EXE", ".BAT", ".CMD"];

fn which_path(name: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        let direct = dir.join(name);
        if direct.is_file() {
            return Some(direct);
        }
        #[cfg(windows)]
        {
            for ext in PATHEXT {
                let candidate = dir.join(format!("{name}{ext}"));
                if candidate.is_file() {
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

fn run_result(stdout: String, stderr: String, code: i64, ok: bool) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("stdout".to_string(), Value::String(stdout).ref_cell());
    map.insert("stderr".to_string(), Value::String(stderr).ref_cell());
    map.insert("code".to_string(), Value::Int(code).ref_cell());
    map.insert("ok".to_string(), Value::Bool(ok).ref_cell());
    Value::Object(map).ref_cell()
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3232_NWORKSPACE_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3230_NWORKSPACE_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3230_NWORKSPACE_ARITY,
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

fn workspace_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3231_NWORKSPACE_ERROR, "nworkspace_error", msg.into(), span)
}

fn find_member<'a>(ws: &'a Workspace, name: &str, span: Span) -> Result<&'a Member, ValueRef> {
    ws.members
        .iter()
        .find(|m| m.name == name)
        .ok_or_else(|| workspace_err(span, format!("unknown workspace member '{name}'")))
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn nworkspace_load(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nworkspace_load", span)?;
    let path = string_arg(args, 0, "nworkspace_load", span)?;
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            return Ok(workspace_err(
                span,
                format!("nworkspace_load() failed to read '{path}': {e}"),
            ));
        }
    };
    let value = match parse_toml_text(&text, span) {
        Ok(v) => v,
        Err(e) => return Err(e),
    };
    let manifest = PathBuf::from(&path);
    let root = manifest
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    match parse_workspace_value(&value, root, Some(manifest), span) {
        Ok(ws) => Ok(workspace_to_value(&ws).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nworkspace_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nworkspace_parse", span)?;
    let text = string_arg(args, 0, "nworkspace_parse", span)?;
    let value = match parse_toml_text(&text, span) {
        Ok(v) => v,
        Err(e) => return Err(e),
    };
    let root = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    match parse_workspace_value(&value, root, None, span) {
        Ok(ws) => Ok(workspace_to_value(&ws).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nworkspace_members(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nworkspace_members", span)?;
    match workspace_from_arg(args, 0, "nworkspace_members", span) {
        Ok(ws) => {
            let members: Vec<ValueRef> = ws
                .members
                .iter()
                .map(|m| {
                    let mut map = HashMap::new();
                    map.insert("name".to_string(), Value::String(m.name.clone()).ref_cell());
                    map.insert(
                        "path".to_string(),
                        Value::String(m.path.to_string_lossy().into_owned()).ref_cell(),
                    );
                    map.insert(
                        "entry".to_string(),
                        Value::String(m.entry.clone()).ref_cell(),
                    );
                    let deps: Vec<ValueRef> = m
                        .depends
                        .iter()
                        .map(|d| Value::String(d.clone()).ref_cell())
                        .collect();
                    map.insert("depends".to_string(), Value::Array(deps).ref_cell());
                    Value::Object(map).ref_cell()
                })
                .collect();
            Ok(Value::Array(members).ref_cell())
        }
        Err(e) => Ok(e),
    }
}

fn nworkspace_graph(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nworkspace_graph", span)?;
    match workspace_from_arg(args, 0, "nworkspace_graph", span) {
        Ok(ws) => {
            let mut map = HashMap::new();
            for m in &ws.members {
                let deps: Vec<ValueRef> = m
                    .depends
                    .iter()
                    .map(|d| Value::String(d.clone()).ref_cell())
                    .collect();
                map.insert(m.name.clone(), Value::Array(deps).ref_cell());
            }
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(e),
    }
}

fn nworkspace_order(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nworkspace_order", span)?;
    match workspace_from_arg(args, 0, "nworkspace_order", span) {
        Ok(ws) => match topo_order(&ws, span) {
            Ok(names) => {
                let items: Vec<ValueRef> = names
                    .into_iter()
                    .map(|n| Value::String(n).ref_cell())
                    .collect();
                Ok(Value::Array(items).ref_cell())
            }
            Err(e) => Ok(e),
        },
        Err(e) => Ok(e),
    }
}

fn nworkspace_member_path(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nworkspace_member_path", span)?;
    let member = string_arg(args, 1, "nworkspace_member_path", span)?;
    match workspace_from_arg(args, 0, "nworkspace_member_path", span) {
        Ok(ws) => match find_member(&ws, &member, span) {
            Ok(m) => Ok(Value::String(m.path.to_string_lossy().into_owned()).ref_cell()),
            Err(e) => Ok(e),
        },
        Err(e) => Ok(e),
    }
}

fn nworkspace_run(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nworkspace_run", span)?;
    let member_name = string_arg(args, 1, "nworkspace_run", span)?;
    let mode = if args.len() == 3 {
        match &*args[2].borrow() {
            Value::String(s) if s == "interp" || s == "vm" => s.clone(),
            other => {
                return Err(type_err(
                    span,
                    format!(
                        "nworkspace_run() mode must be \"interp\" or \"vm\", got {}",
                        other.type_name()
                    ),
                ));
            }
        }
    } else {
        "interp".to_string()
    };
    let ws = match workspace_from_arg(args, 0, "nworkspace_run", span) {
        Ok(ws) => ws,
        Err(e) => return Ok(e),
    };
    let member = match find_member(&ws, &member_name, span) {
        Ok(m) => m,
        Err(e) => return Ok(e),
    };
    let entry = member.path.join(&member.entry);
    if !entry.exists() {
        return Ok(workspace_err(
            span,
            format!(
                "entry file '{}' not found for member '{}'",
                entry.display(),
                member.name
            ),
        ));
    }
    let binary = match find_niao_binary() {
        Some(p) => p,
        None => {
            return Ok(workspace_err(
                span,
                "nworkspace_run() could not find niao binary (set NIAO_BIN or add niao to PATH)",
            ));
        }
    };
    let mut cmd = Command::new(&binary);
    cmd.arg("run")
        .arg(&entry)
        .arg("--mode")
        .arg(&mode)
        .current_dir(&member.path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let output = match cmd.output() {
        Ok(o) => o,
        Err(e) => {
            return Ok(workspace_err(
                span,
                format!("nworkspace_run() failed to spawn niao: {e}"),
            ));
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout).trim_end().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim_end().to_string();
    let code = output.status.code().unwrap_or(-1) as i64;
    Ok(run_result(stdout, stderr, code, output.status.success()))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nworkspace_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nworkspace_fns![
    ("nworkspace_load", "load", nworkspace_load),
    ("nworkspace_parse", "parse", nworkspace_parse),
    ("nworkspace_members", "members", nworkspace_members),
    ("nworkspace_graph", "graph", nworkspace_graph),
    ("nworkspace_order", "order", nworkspace_order),
    ("nworkspace_member_path", "member_path", nworkspace_member_path),
    ("nworkspace_run", "run", nworkspace_run),
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

pub const MODULE_NAME: &str = "nworkspace";
pub const MODULE_PATHS: &[&str] = &["nworkspace", "std/nworkspace"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;
    use std::io::Write;

    fn span() -> Span {
        Span::dummy()
    }

    fn s(v: &str) -> ValueRef {
        Value::String(v.to_string()).ref_cell()
    }

    const SAMPLE: &str = r#"
name = "demo-workspace"

[[members]]
name = "core"
path = "packages/core"
entry = "main.niao"

[[members]]
name = "app"
path = "packages/app"
entry = "main.niao"
depends = ["core"]
"#;

    #[test]
    fn parse_members_graph_order() {
        let ws_ref = nworkspace_parse(&[s(SAMPLE)], span()).unwrap();
        let ws_val = ws_ref.borrow().clone();
        let members = nworkspace_members(&[ws_ref.clone()], span()).unwrap();
        match &*members.borrow() {
            Value::Array(items) => assert_eq!(items.len(), 2),
            other => panic!("expected array, got {other:?}"),
        }
        let graph = nworkspace_graph(&[ws_ref.clone()], span()).unwrap();
        match &*graph.borrow() {
            Value::Object(map) => {
                assert!(map.contains_key("core"));
                assert!(map.contains_key("app"));
            }
            other => panic!("expected object, got {other:?}"),
        }
        let order = nworkspace_order(&[ws_ref], span()).unwrap();
        match &*order.borrow() {
            Value::Array(items) => {
                let names: Vec<String> = items
                    .iter()
                    .filter_map(|v| match &*v.borrow() {
                        Value::String(s) => Some(s.clone()),
                        _ => None,
                    })
                    .collect();
                assert_eq!(names, vec!["core".to_string(), "app".to_string()]);
            }
            other => panic!("expected array, got {other:?}"),
        }
        let _ = ws_val;
    }

    #[test]
    fn cycle_is_catchable_error() {
        let text = r#"
name = "cyclic"
[[members]]
name = "a"
path = "a"
depends = ["b"]
[[members]]
name = "b"
path = "b"
depends = ["a"]
"#;
        let ws = nworkspace_parse(&[s(text)], span()).unwrap();
        let v = nworkspace_order(&[ws], span()).unwrap();
        assert!(matches!(
            &*v.borrow(),
            Value::Error(e) if e.code == E3231_NWORKSPACE_ERROR
        ));
    }

    #[test]
    fn load_from_file_roundtrip() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "nworkspace_test_{}_{}.toml",
            std::process::id(),
            "manifest"
        ));
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(SAMPLE.as_bytes()).unwrap();
        let ws = nworkspace_load(&[s(path.to_str().unwrap())], span()).unwrap();
        match &*ws.borrow() {
            Value::Object(map) => {
                assert!(matches!(
                    &*map["name"].borrow(),
                    Value::String(n) if n == "demo-workspace"
                ));
                assert!(matches!(&*map["manifest"].borrow(), Value::String(_)));
            }
            other => panic!("expected object, got {other:?}"),
        }
        let _ = fs::remove_file(path);
    }

    #[test]
    fn namespace_has_expected_methods() {
        match namespace() {
            Value::Object(map) => {
                for key in [
                    "load",
                    "parse",
                    "members",
                    "graph",
                    "order",
                    "member_path",
                    "run",
                ] {
                    assert!(map.contains_key(key), "missing {key}");
                }
            }
            other => panic!("expected object, got {other:?}"),
        }
        assert_eq!(builtins().len(), 7);
    }
}
