//! Native nhub standard library — model/dataset downloads, HF Hub cache,
//! resumable direct URLs, checksums (~huggingface-hub subset).
//!
//! Import with `import "nhub"` (or `import "std/nhub"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_hub::{
    cache_dir_from_env, default_cache_dir, download_url, hash_bytes, hash_file, verify_bytes,
    verify_file, DirectOpts, HashAlgo, HubClient, HubConfig, HubError, HubRepo, SnapshotOpts,
    VERSION,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

const E4617: u32 = codes::E4617_NHUB_ARITY;
const E4618: u32 = codes::E4618_NHUB_ERROR;
const E4619: u32 = codes::E4619_NHUB_TYPE;
const E4620: u32 = codes::E4620_NHUB_INVALID_HANDLE;
const E4622: u32 = codes::E4622_NHUB_CHECKSUM;

struct ClientEntry {
    client: HubClient,
}

struct RepoEntry {
    repo: HubRepo,
}

thread_local! {
    static CLIENTS: RefCell<HashMap<i64, ClientEntry>> = RefCell::new(HashMap::new());
    static REPOS: RefCell<HashMap<i64, RepoEntry>> = RefCell::new(HashMap::new());
    static NEXT_ID: RefCell<i64> = const { RefCell::new(1) };
}

fn alloc_id() -> i64 {
    NEXT_ID.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    })
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E4619, msg.into())
}

fn nhub_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E4618, "nhub_error", msg.into(), span)
}

fn checksum_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E4622, "nhub_error", msg.into(), span)
}

fn invalid_client(span: Span, id: i64) -> ValueRef {
    error_value(
        E4620,
        "nhub_error",
        format!("invalid or closed nhub client handle {id}"),
        span,
    )
}

fn invalid_repo(span: Span, id: i64) -> ValueRef {
    error_value(
        E4620,
        "nhub_error",
        format!("invalid or closed nhub repo handle {id}"),
        span,
    )
}

fn map_hub_err(span: Span, e: HubError) -> ValueRef {
    match &e {
        HubError::Checksum { expected, actual } => checksum_err(
            span,
            format!("checksum mismatch: expected {expected}, got {actual}"),
        ),
        _ => nhub_err(span, e.to_string()),
    }
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
            E4617,
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

fn bool_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: bool) -> bool {
    let Some(map) = map else {
        return default;
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Bool(b)) => b,
        Some(Value::Int(n)) => n != 0,
        _ => default,
    }
}

fn string_field(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Option<String> {
    let map = map?;
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::String(s)) => Some(s),
        _ => None,
    }
}

fn int_field(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Option<i64> {
    let map = map?;
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Int(n)) => Some(n),
        _ => None,
    }
}

fn string_array_field(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Vec<String> {
    let Some(map) = map else {
        return Vec::new();
    };
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

fn string_map_field(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Vec<(String, String)> {
    let Some(map) = map else {
        return Vec::new();
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Object(obj)) => obj
            .iter()
            .filter_map(|(k, v)| match &*v.borrow() {
                Value::String(s) => Some((k.clone(), s.clone())),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn optional_object(args: &[ValueRef], idx: usize) -> Option<HashMap<String, ValueRef>> {
    if args.len() <= idx {
        return None;
    }
    match &*args[idx].borrow() {
        Value::Object(m) => Some(m.clone()),
        Value::Nil => None,
        _ => None,
    }
}

fn parse_hub_config(map: Option<&HashMap<String, ValueRef>>) -> HubConfig {
    let mut cfg = HubConfig::default();
    if let Some(map) = map {
        if let Some(dir) = string_field(Some(map), "cache_dir") {
            cfg.cache_dir = Some(PathBuf::from(dir));
        }
        cfg.token = string_field(Some(map), "token");
        cfg.endpoint = string_field(Some(map), "endpoint");
        if let Some(n) = int_field(Some(map), "retries") {
            cfg.retries = n.max(0) as usize;
        }
        cfg.progress = bool_field(Some(map), "progress", false);
    }
    cfg
}

fn parse_direct_opts(map: Option<&HashMap<String, ValueRef>>) -> DirectOpts {
    let mut opts = DirectOpts::default();
    if let Some(map) = map {
        if let Some(n) = int_field(Some(map), "timeout_ms") {
            opts.timeout_ms = n.max(1) as u64;
        }
        if let Some(n) = int_field(Some(map), "retries") {
            opts.retries = n.max(0) as usize;
        }
        opts.resume = bool_field(Some(map), "resume", true);
        opts.expected_sha256 = string_field(Some(map), "expected_sha256")
            .or_else(|| string_field(Some(map), "sha256"));
        opts.headers = string_map_field(Some(map), "headers");
    }
    opts
}

fn parse_snapshot_opts(map: Option<&HashMap<String, ValueRef>>) -> SnapshotOpts {
    SnapshotOpts {
        allow_patterns: string_array_field(map, "allow_patterns")
            .into_iter()
            .chain(string_array_field(map, "allow"))
            .collect(),
        ignore_patterns: string_array_field(map, "ignore_patterns")
            .into_iter()
            .chain(string_array_field(map, "ignore"))
            .collect(),
    }
}

fn parse_algo(map: Option<&HashMap<String, ValueRef>>, default: HashAlgo) -> HashAlgo {
    string_field(map, "algo")
        .and_then(|s| HashAlgo::parse(&s))
        .unwrap_or(default)
}

fn result_obj(fields: Vec<(&str, Value)>) -> ValueRef {
    let mut map = HashMap::new();
    for (k, v) in fields {
        map.insert(k.to_string(), v.ref_cell());
    }
    Value::Object(map).ref_cell()
}

fn with_client<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&HubClient) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    CLIENTS.with(|m| match m.borrow().get(&id) {
        Some(entry) => Ok(Ok(f(&entry.client))),
        None => Ok(Err(invalid_client(span, id))),
    })
}

fn with_repo<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&HubRepo) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    REPOS.with(|m| match m.borrow().get(&id) {
        Some(entry) => Ok(Ok(f(&entry.repo))),
        None => Ok(Err(invalid_repo(span, id))),
    })
}

enum RepoKind {
    Model,
    Dataset,
    Space,
}

// >>> import "nhub"; nhub.version() != ""
fn nhub_version(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    Ok(Value::String(VERSION.into()).ref_cell())
}

// >>> import "nhub"; len(nhub.cache_dir()) > 0
fn nhub_cache_dir(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    let p = cache_dir_from_env();
    Ok(Value::String(p.to_string_lossy().into()).ref_cell())
}

// >>> import "nhub"; nhub.default_cache_dir() != ""
fn nhub_default_cache_dir(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    let p = default_cache_dir();
    Ok(Value::String(p.to_string_lossy().into()).ref_cell())
}

// >>> import "nhub"; let c = nhub.client({}); c > 0
fn nhub_client(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nhub_client", span)?;
    let cfg = parse_hub_config(optional_object(args, 0).as_ref());
    match HubClient::new(cfg) {
        Ok(client) => {
            let id = alloc_id();
            CLIENTS.with(|m| {
                m.borrow_mut().insert(id, ClientEntry { client });
            });
            Ok(Value::Int(id).ref_cell())
        }
        Err(e) => Ok(map_hub_err(span, e)),
    }
}

// >>> import "nhub"; let c = nhub.client({}); nhub.close(c)
fn nhub_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nhub_close", span)?;
    let id = int_arg(args, 0, "nhub_close", span)?;
    let removed = CLIENTS.with(|m| m.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

// >>> import "nhub"; let c = nhub.client({}); nhub.token(c) == nil || type(nhub.token(c)) == "string"
fn nhub_token(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nhub_token", span)?;
    if args.is_empty() {
        let tok = HubClient::from_env().ok().and_then(|c| c.token());
        return Ok(match tok {
            Some(s) => Value::String(s).ref_cell(),
            None => Value::Nil.ref_cell(),
        });
    }
    let id = int_arg(args, 0, "nhub_token", span)?;
    match with_client(id, span, |c| c.token())? {
        Ok(tok) => Ok(match tok {
            Some(s) => Value::String(s).ref_cell(),
            None => Value::Nil.ref_cell(),
        }),
        Err(v) => Ok(v),
    }
}

fn open_repo(args: &[ValueRef], span: Span, name: &str, kind: RepoKind) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, name, span)?;
    let client_id = int_arg(args, 0, name, span)?;
    let repo_id = string_arg(args, 1, name, span)?;
    let revision = optional_object(args, 2)
        .as_ref()
        .and_then(|m| string_field(Some(m), "revision"));
    match with_client(client_id, span, |c| match kind {
        RepoKind::Model => c.model(&repo_id, revision.as_deref()),
        RepoKind::Dataset => c.dataset(&repo_id, revision.as_deref()),
        RepoKind::Space => c.space(&repo_id, revision.as_deref()),
    })? {
        Ok(repo) => {
            let id = alloc_id();
            REPOS.with(|m| m.borrow_mut().insert(id, RepoEntry { repo }));
            Ok(Value::Int(id).ref_cell())
        }
        Err(v) => Ok(v),
    }
}

// >>> import "nhub"; let c = nhub.client({}); let r = nhub.model(c, "gpt2"); r > 0
fn nhub_model(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    open_repo(args, span, "nhub_model", RepoKind::Model)
}

// >>> import "nhub"; let c = nhub.client({}); let r = nhub.dataset(c, "squad"); r > 0
fn nhub_dataset(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    open_repo(args, span, "nhub_dataset", RepoKind::Dataset)
}

// >>> import "nhub"; let c = nhub.client({}); nhub.close_repo(nhub.model(c, "gpt2"))
fn nhub_close_repo(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nhub_close_repo", span)?;
    let id = int_arg(args, 0, "nhub_close_repo", span)?;
    let removed = REPOS.with(|m| m.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

// >>> import "nhub"; let c = nhub.client({}); let r = nhub.model(c, "gpt2"); nhub.file_url(r, "config.json").contains("gpt2")
fn nhub_file_url(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "nhub_file_url", span)?;
    let repo_id = int_arg(args, 0, "nhub_file_url", span)?;
    let filename = string_arg(args, 1, "nhub_file_url", span)?;
    match with_repo(repo_id, span, |r| r.file_url(&filename))? {
        Ok(url) => Ok(Value::String(url).ref_cell()),
        Err(v) => Ok(v),
    }
}

// >>> import "nhub"; let c = nhub.client({}); let r = nhub.model(c, "gpt2"); nhub.repo_info(r).sha != nil
fn nhub_repo_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nhub_repo_info", span)?;
    let repo_id = int_arg(args, 0, "nhub_repo_info", span)?;
    REPOS.with(|m| {
        let borrowed = m.borrow();
        let Some(entry) = borrowed.get(&repo_id) else {
            return Ok(invalid_repo(span, repo_id));
        };
        match entry.repo.info() {
            Ok(info) => {
                let files: Vec<ValueRef> = info
                    .siblings
                    .into_iter()
                    .map(|s| Value::String(s.rfilename).ref_cell())
                    .collect();
                Ok(result_obj(vec![
                    ("sha", Value::String(info.sha)),
                    ("files", Value::Array(files)),
                    ("repo_id", Value::String(entry.repo.repo_id().to_string())),
                    ("revision", Value::String(entry.repo.revision().to_string())),
                    ("kind", Value::String(entry.repo.kind_name().to_string())),
                ]))
            }
            Err(e) => Ok(map_hub_err(span, e)),
        }
    })
}

// >>> import "nhub"; let c = nhub.client({}); let r = nhub.model(c, "gpt2"); len(nhub.list_files(r)) >= 0
fn nhub_list_files(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nhub_list_files", span)?;
    let repo_id = int_arg(args, 0, "nhub_list_files", span)?;
    match with_repo(repo_id, span, |r| r.list_files())? {
        Ok(Ok(files)) => {
            let arr = files
                .into_iter()
                .map(|f| Value::String(f).ref_cell())
                .collect();
            Ok(Value::Array(arr).ref_cell())
        }
        Ok(Err(e)) => Ok(map_hub_err(span, e)),
        Err(v) => Ok(v),
    }
}

// >>> import "nhub"; let c = nhub.client({}); let r = nhub.model(c, "gpt2"); nhub.cached(r, "missing.xyz") == nil
fn nhub_cached(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "nhub_cached", span)?;
    let repo_id = int_arg(args, 0, "nhub_cached", span)?;
    let filename = string_arg(args, 1, "nhub_cached", span)?;
    match with_repo(repo_id, span, |r| r.cached_path(&filename))? {
        Ok(path) => Ok(match path {
            Some(p) => Value::String(p.to_string_lossy().into()).ref_cell(),
            None => Value::Nil.ref_cell(),
        }),
        Err(v) => Ok(v),
    }
}

// >>> import "nhub"; let c = nhub.client({}); let r = nhub.model(c, "gpt2"); nhub.download(r, "config.json").path != nil
fn nhub_download(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nhub_download", span)?;
    let repo_id = int_arg(args, 0, "nhub_download", span)?;
    let filename = string_arg(args, 1, "nhub_download", span)?;
    match with_repo(repo_id, span, |r| r.download(&filename))? {
        Ok(Ok(dl)) => Ok(result_obj(vec![
            ("path", Value::String(dl.path.to_string_lossy().into())),
            ("bytes", Value::Int(dl.bytes as i64)),
            ("cached", Value::Bool(dl.cached)),
        ])),
        Ok(Err(e)) => Ok(map_hub_err(span, e)),
        Err(v) => Ok(v),
    }
}

// >>> import "nhub"; let c = nhub.client({}); let r = nhub.model(c, "gpt2"); nhub.snapshot(r, {allow_patterns: ["*.json"]}).count >= 0
fn nhub_snapshot(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nhub_snapshot", span)?;
    let repo_id = int_arg(args, 0, "nhub_snapshot", span)?;
    let opts = parse_snapshot_opts(optional_object(args, 1).as_ref());
    match with_repo(repo_id, span, |r| r.snapshot_download(&opts))? {
        Ok(Ok(snap)) => {
            let paths: Vec<ValueRef> = snap
                .paths
                .iter()
                .map(|p| Value::String(p.to_string_lossy().into()).ref_cell())
                .collect();
            Ok(result_obj(vec![
                ("paths", Value::Array(paths)),
                ("count", Value::Int(snap.count as i64)),
                ("bytes", Value::Int(snap.bytes as i64)),
            ]))
        }
        Ok(Err(e)) => Ok(map_hub_err(span, e)),
        Err(v) => Ok(v),
    }
}

// >>> import "nhub"; nhub.download_url("http://127.0.0.1:1/nope", "/tmp/x").code != nil
fn nhub_download_url(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nhub_download_url", span)?;
    let url = string_arg(args, 0, "nhub_download_url", span)?;
    let dest = string_arg(args, 1, "nhub_download_url", span)?;
    let opts = parse_direct_opts(optional_object(args, 2).as_ref());
    match download_url(&url, Path::new(&dest), &opts) {
        Ok(r) => Ok(result_obj(vec![
            ("path", Value::String(r.path.to_string_lossy().into())),
            ("bytes", Value::Int(r.bytes as i64)),
            ("resumed", Value::Bool(r.resumed)),
        ])),
        Err(e) => Ok(map_hub_err(span, e)),
    }
}

fn hash_from_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<u8>> {
    match &*args[idx].borrow() {
        Value::String(path) => {
            let data = std::fs::read(path).map_err(|e| {
                RuntimeError::at(span, codes::E4621_NHUB_IO, format!("read {path}: {e}"))
            })?;
            Ok(data)
        }
        Value::IntArray(bytes) => Ok(bytes.iter().map(|&b| b as u8).collect()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects string path or byte array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

// >>> import "nhub"; nhub.sha256("abc") == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
fn nhub_sha256(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nhub_sha256", span)?;
    match &*args[0].borrow() {
        Value::String(s) => {
            let path = Path::new(s);
            if path.is_file() {
                match hash_file(path, HashAlgo::Sha256) {
                    Ok(h) => Ok(Value::String(h).ref_cell()),
                    Err(e) => Ok(map_hub_err(span, e)),
                }
            } else {
                Ok(Value::String(hash_bytes(s.as_bytes(), HashAlgo::Sha256)).ref_cell())
            }
        }
        _ => {
            let data = hash_from_arg(args, 0, "nhub_sha256", span)?;
            Ok(Value::String(hash_bytes(&data, HashAlgo::Sha256)).ref_cell())
        }
    }
}

// >>> import "nhub"; nhub.verify("abc", "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
fn nhub_verify(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nhub_verify", span)?;
    let expected = string_arg(args, 1, "nhub_verify", span)?;
    let algo = parse_algo(optional_object(args, 2).as_ref(), HashAlgo::Sha256);
    let result = match &*args[0].borrow() {
        Value::String(s) => {
            let path = Path::new(s);
            if path.is_file() {
                verify_file(path, &expected, algo)
            } else {
                verify_bytes(s.as_bytes(), &expected, algo)
            }
        }
        _ => {
            let data = hash_from_arg(args, 0, "nhub_verify", span)?;
            verify_bytes(&data, &expected, algo)
        }
    };
    match result {
        Ok(true) => Ok(Value::Bool(true).ref_cell()),
        Ok(false) => Ok(Value::Bool(false).ref_cell()),
        Err(e) => Ok(map_hub_err(span, e)),
    }
}

fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
    vec![
        ("nhub_version", "version", Rc::new(nhub_version)),
        ("nhub_cache_dir", "cache_dir", Rc::new(nhub_cache_dir)),
        (
            "nhub_default_cache_dir",
            "default_cache_dir",
            Rc::new(nhub_default_cache_dir),
        ),
        ("nhub_client", "client", Rc::new(nhub_client)),
        ("nhub_close", "close", Rc::new(nhub_close)),
        ("nhub_token", "token", Rc::new(nhub_token)),
        ("nhub_model", "model", Rc::new(nhub_model)),
        ("nhub_dataset", "dataset", Rc::new(nhub_dataset)),
        ("nhub_close_repo", "close_repo", Rc::new(nhub_close_repo)),
        ("nhub_file_url", "file_url", Rc::new(nhub_file_url)),
        ("nhub_repo_info", "repo_info", Rc::new(nhub_repo_info)),
        ("nhub_list_files", "list_files", Rc::new(nhub_list_files)),
        ("nhub_cached", "cached", Rc::new(nhub_cached)),
        ("nhub_download", "download", Rc::new(nhub_download)),
        ("nhub_snapshot", "snapshot", Rc::new(nhub_snapshot)),
        (
            "nhub_download_url",
            "download_url",
            Rc::new(nhub_download_url),
        ),
        ("nhub_sha256", "sha256", Rc::new(nhub_sha256)),
        ("nhub_verify", "verify", Rc::new(nhub_verify)),
    ]
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nhub";
pub const MODULE_PATHS: &[&str] = &["nhub", "std/nhub"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs()
        .into_iter()
        .map(|(flat, _, f)| (flat, f))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_doctest() {
        let args = vec![Value::String("abc".into()).ref_cell()];
        let v = nhub_sha256(&args, Span::dummy()).unwrap();
        let owned = v.borrow().clone();
        match owned {
            Value::String(s) => assert_eq!(
                s,
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
            ),
            other => panic!("expected string, got {other:?}"),
        };
    }

    #[test]
    fn cache_dir_doctest() {
        let v = nhub_cache_dir(&[], Span::dummy()).unwrap();
        let owned = v.borrow().clone();
        match owned {
            Value::String(s) => assert!(s.contains("huggingface")),
            other => panic!("expected string, got {other:?}"),
        };
    }

    #[test]
    fn client_lifecycle() {
        let id = nhub_client(&[], Span::dummy()).unwrap();
        let handle = match id.borrow().clone() {
            Value::Int(n) => n,
            _ => panic!("expected handle"),
        };
        let closed = nhub_close(&[Value::Int(handle).ref_cell()], Span::dummy()).unwrap();
        let owned = closed.borrow().clone();
        match owned {
            Value::Bool(true) => {}
            other => panic!("expected true, got {other:?}"),
        };
    }
}
