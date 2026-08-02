//! Native nsemver standard library — SemVer 2.0 parse, compare, range checks,
//! and version increment. Hand-rolled parser (no external crate).
//!
//! Import with `import "nsemver"` (or `import "std/nsemver"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::collections::HashMap;
use std::rc::Rc;

// Wired in codes.rs by central integration.
const E2900_NSEMVER_ARITY: u32 = 2900;
const E2901_NSEMVER_ERROR: u32 = 2901;
const E2902_NSEMVER_PARSE: u32 = 2902;

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
            E2900_NSEMVER_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
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

fn parse_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E2902_NSEMVER_PARSE, "nsemver_error", msg.into(), span)
}

fn semver_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E2901_NSEMVER_ERROR, "nsemver_error", msg.into(), span)
}

// ---------------------------------------------------------------------------
// SemVer model
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
enum Identifier {
    Numeric(u64),
    Alpha(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SemVer {
    major: u64,
    minor: u64,
    patch: u64,
    pre: Vec<Identifier>,
    build: Vec<Identifier>,
}

impl SemVer {
    fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
            pre: Vec::new(),
            build: Vec::new(),
        }
    }

    fn to_string(&self) -> String {
        let mut out = format!("{}.{}.{}", self.major, self.minor, self.patch);
        if !self.pre.is_empty() {
            out.push('-');
            out.push_str(&join_identifiers(&self.pre));
        }
        if !self.build.is_empty() {
            out.push('+');
            out.push_str(&join_identifiers(&self.build));
        }
        out
    }
}

fn join_identifiers(ids: &[Identifier]) -> String {
    ids.iter()
        .map(|id| match id {
            Identifier::Numeric(n) => n.to_string(),
            Identifier::Alpha(s) => s.clone(),
        })
        .collect::<Vec<_>>()
        .join(".")
}

fn parse_numeric_component(s: &str, label: &str) -> Result<u64, String> {
    if s.is_empty() {
        return Err(format!("missing {label} version component"));
    }
    if s.len() > 1 && s.starts_with('0') {
        return Err(format!(
            "{label} version component must not have leading zeros"
        ));
    }
    s.parse::<u64>()
        .map_err(|_| format!("invalid {label} version component '{s}'"))
}

fn parse_identifiers(s: &str) -> Result<Vec<Identifier>, String> {
    if s.is_empty() {
        return Err("empty identifier".into());
    }
    s.split('.')
        .map(|part| {
            if part.is_empty() {
                return Err("empty identifier".into());
            }
            if part.chars().all(|c| c.is_ascii_digit()) {
                if part.len() > 1 && part.starts_with('0') {
                    return Err(format!(
                        "numeric identifier must not have leading zeros: '{part}'"
                    ));
                }
                let n = part
                    .parse::<u64>()
                    .map_err(|_| format!("invalid numeric identifier '{part}'"))?;
                Ok(Identifier::Numeric(n))
            } else {
                if !part.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                    return Err(format!("invalid identifier '{part}'"));
                }
                Ok(Identifier::Alpha(part.to_string()))
            }
        })
        .collect()
}

fn parse_version(s: &str) -> Result<SemVer, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty version string".into());
    }

    let (core_and_pre, build) = match s.split_once('+') {
        Some((left, build)) => (left, Some(build)),
        None => (s, None),
    };

    let mut dash_parts = core_and_pre.splitn(2, '-');
    let core = dash_parts.next().unwrap_or("");
    let pre = dash_parts.next();

    let mut parts = core.split('.');
    let major_s = parts
        .next()
        .ok_or_else(|| "missing major version component".to_string())?;
    let minor_s = parts
        .next()
        .ok_or_else(|| "missing minor version component".to_string())?;
    let patch_s = parts
        .next()
        .ok_or_else(|| "missing patch version component".to_string())?;
    if parts.next().is_some() {
        return Err("version has too many core components".into());
    }

    let major = parse_numeric_component(major_s, "major")?;
    let minor = parse_numeric_component(minor_s, "minor")?;
    let patch = parse_numeric_component(patch_s, "patch")?;

    let pre = match pre {
        Some(p) if !p.is_empty() => parse_identifiers(p)?,
        _ => Vec::new(),
    };
    let build = match build {
        Some(b) => parse_identifiers(b)?,
        None => Vec::new(),
    };

    Ok(SemVer {
        major,
        minor,
        patch,
        pre,
        build,
    })
}

fn compare_identifiers(a: &[Identifier], b: &[Identifier]) -> i32 {
    let max = a.len().max(b.len());
    for i in 0..max {
        match (a.get(i), b.get(i)) {
            (Some(Identifier::Numeric(x)), Some(Identifier::Numeric(y))) => {
                if x < y {
                    return -1;
                }
                if x > y {
                    return 1;
                }
            }
            (Some(Identifier::Numeric(_)), Some(Identifier::Alpha(_))) => return -1,
            (Some(Identifier::Alpha(_)), Some(Identifier::Numeric(_))) => return 1,
            (Some(Identifier::Alpha(x)), Some(Identifier::Alpha(y))) => {
                if x < y {
                    return -1;
                }
                if x > y {
                    return 1;
                }
            }
            (Some(_), None) => return 1,
            (None, Some(_)) => return -1,
            (None, None) => {}
        }
    }
    0
}

fn compare_versions(a: &SemVer, b: &SemVer) -> i32 {
    if a.major != b.major {
        return (a.major > b.major) as i32 - (a.major < b.major) as i32;
    }
    if a.minor != b.minor {
        return (a.minor > b.minor) as i32 - (a.minor < b.minor) as i32;
    }
    if a.patch != b.patch {
        return (a.patch > b.patch) as i32 - (a.patch < b.patch) as i32;
    }
    match (a.pre.is_empty(), b.pre.is_empty()) {
        (true, true) => 0,
        (true, false) => 1,
        (false, true) => -1,
        (false, false) => compare_identifiers(&a.pre, &b.pre),
    }
}

fn compare_str(a: &str, b: &str) -> Result<i32, String> {
    let va = parse_version(a)?;
    let vb = parse_version(b)?;
    Ok(compare_versions(&va, &vb))
}

// ---------------------------------------------------------------------------
// Range matching
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum ComparatorOp {
    Eq,
    Lt,
    Lte,
    Gt,
    Gte,
    Caret,
    Tilde,
}

#[derive(Clone, Debug)]
struct Comparator {
    op: ComparatorOp,
    version: SemVer,
    /// Number of dotted components in the range token (1..=3), used for tilde.
    parts: usize,
}

fn parse_partial_version(s: &str) -> Result<(SemVer, usize), String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty version in range".into());
    }
    let (core, pre) = match s.split_once('-') {
        Some((core, pre)) if !core.is_empty() => (core, Some(pre)),
        _ => (s, None),
    };
    if core.contains('+') {
        return Err("build metadata not allowed in range comparators".into());
    }

    let comps: Vec<&str> = core.split('.').collect();
    if comps.is_empty() || comps.len() > 3 {
        return Err(format!("invalid version '{s}' in range"));
    }
    for c in &comps {
        if c.is_empty() {
            return Err(format!("invalid version '{s}' in range"));
        }
    }

    let major = parse_numeric_component(comps[0], "major")?;
    let minor = if comps.len() > 1 {
        parse_numeric_component(comps[1], "minor")?
    } else {
        0
    };
    let patch = if comps.len() > 2 {
        parse_numeric_component(comps[2], "patch")?
    } else {
        0
    };
    let pre = match pre {
        Some(p) => parse_identifiers(p)?,
        None => Vec::new(),
    };

    Ok((
        SemVer {
            major,
            minor,
            patch,
            pre,
            build: Vec::new(),
        },
        comps.len(),
    ))
}

fn parse_comparator(token: &str) -> Result<Comparator, String> {
    let token = token.trim();
    if token.is_empty() {
        return Err("empty range comparator".into());
    }

    let (op, ver_s) = if let Some(rest) = token.strip_prefix("^") {
        (ComparatorOp::Caret, rest)
    } else if let Some(rest) = token.strip_prefix("~") {
        (ComparatorOp::Tilde, rest)
    } else if let Some(rest) = token.strip_prefix(">=") {
        (ComparatorOp::Gte, rest)
    } else if let Some(rest) = token.strip_prefix("<=") {
        (ComparatorOp::Lte, rest)
    } else if let Some(rest) = token.strip_prefix("!=") {
        return Err("unsupported operator '!='".into());
    } else if let Some(rest) = token.strip_prefix("=") {
        (ComparatorOp::Eq, rest)
    } else if let Some(rest) = token.strip_prefix(">") {
        (ComparatorOp::Gt, rest)
    } else if let Some(rest) = token.strip_prefix("<") {
        (ComparatorOp::Lt, rest)
    } else {
        (ComparatorOp::Eq, token)
    };

    let (version, parts) = parse_partial_version(ver_s)?;
    Ok(Comparator { op, version, parts })
}

fn caret_upper(v: &SemVer) -> SemVer {
    if v.major > 0 {
        SemVer::new(v.major + 1, 0, 0)
    } else if v.minor > 0 {
        SemVer::new(0, v.minor + 1, 0)
    } else {
        SemVer::new(0, 0, v.patch + 1)
    }
}

fn tilde_upper(v: &SemVer, parts: usize) -> SemVer {
    match parts {
        1 => SemVer::new(v.major + 1, 0, 0),
        _ => SemVer::new(v.major, v.minor + 1, 0),
    }
}

fn satisfies_comparator(version: &SemVer, cmp: &Comparator) -> bool {
    let c = compare_versions(version, &cmp.version);
    match cmp.op {
        ComparatorOp::Eq => c == 0,
        ComparatorOp::Lt => c < 0,
        ComparatorOp::Lte => c <= 0,
        ComparatorOp::Gt => c > 0,
        ComparatorOp::Gte => c >= 0,
        ComparatorOp::Caret => {
            let upper = caret_upper(&cmp.version);
            c >= 0 && compare_versions(version, &upper) < 0
        }
        ComparatorOp::Tilde => {
            let upper = tilde_upper(&cmp.version, cmp.parts);
            c >= 0 && compare_versions(version, &upper) < 0
        }
    }
}

fn satisfies_range(version: &SemVer, range: &str) -> Result<bool, String> {
    let range = range.trim();
    if range.is_empty() {
        return Err("empty range string".into());
    }
    for token in range.split_whitespace() {
        let cmp = parse_comparator(token)?;
        if !satisfies_comparator(version, &cmp) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn satisfies_str(version: &str, range: &str) -> Result<bool, String> {
    let v = parse_version(version)?;
    satisfies_range(&v, range)
}

// ---------------------------------------------------------------------------
// Value builders
// ---------------------------------------------------------------------------

fn version_object(v: &SemVer) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("major".to_string(), Value::Int(v.major as i64).ref_cell());
    map.insert("minor".to_string(), Value::Int(v.minor as i64).ref_cell());
    map.insert("patch".to_string(), Value::Int(v.patch as i64).ref_cell());
    map.insert(
        "pre".to_string(),
        if v.pre.is_empty() {
            Value::String(String::new()).ref_cell()
        } else {
            Value::String(join_identifiers(&v.pre)).ref_cell()
        },
    );
    map.insert(
        "build".to_string(),
        if v.build.is_empty() {
            Value::String(String::new()).ref_cell()
        } else {
            Value::String(join_identifiers(&v.build)).ref_cell()
        },
    );
    Value::Object(map).ref_cell()
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn nsemver_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsemver_parse", span)?;
    let s = string_arg(args, 0, "nsemver_parse", span)?;
    match parse_version(&s) {
        Ok(v) => Ok(version_object(&v)),
        Err(msg) => Ok(parse_err(span, msg)),
    }
}

fn nsemver_compare(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsemver_compare", span)?;
    let a = string_arg(args, 0, "nsemver_compare", span)?;
    let b = string_arg(args, 1, "nsemver_compare", span)?;
    match compare_str(&a, &b) {
        Ok(n) => Ok(Value::Int(n as i64).ref_cell()),
        Err(msg) => Ok(parse_err(span, msg)),
    }
}

fn nsemver_valid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsemver_valid", span)?;
    let s = string_arg(args, 0, "nsemver_valid", span)?;
    Ok(Value::Bool(parse_version(&s).is_ok()).ref_cell())
}

fn nsemver_satisfies(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsemver_satisfies", span)?;
    let version = string_arg(args, 0, "nsemver_satisfies", span)?;
    let range = string_arg(args, 1, "nsemver_satisfies", span)?;
    match parse_version(&version) {
        Ok(v) => match satisfies_range(&v, &range) {
            Ok(ok) => Ok(Value::Bool(ok).ref_cell()),
            Err(msg) => Ok(parse_err(span, msg)),
        },
        Err(_) => Ok(Value::Bool(false).ref_cell()),
    }
}

fn nsemver_inc(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsemver_inc", span)?;
    let s = string_arg(args, 0, "nsemver_inc", span)?;
    let part = string_arg(args, 1, "nsemver_inc", span)?;
    let mut v = match parse_version(&s) {
        Ok(v) => v,
        Err(msg) => return Ok(parse_err(span, msg)),
    };
    match part.as_str() {
        "major" => {
            v.major += 1;
            v.minor = 0;
            v.patch = 0;
        }
        "minor" => {
            v.minor += 1;
            v.patch = 0;
        }
        "patch" => v.patch += 1,
        other => {
            return Ok(semver_err(
                span,
                format!("nsemver_inc() part must be 'major', 'minor', or 'patch', got '{other}'"),
            ));
        }
    }
    v.pre.clear();
    v.build.clear();
    Ok(Value::String(v.to_string()).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nsemver_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nsemver_fns![
    ("nsemver_parse", "parse", nsemver_parse),
    ("nsemver_compare", "compare", nsemver_compare),
    ("nsemver_valid", "valid", nsemver_valid),
    ("nsemver_satisfies", "satisfies", nsemver_satisfies),
    ("nsemver_inc", "inc", nsemver_inc),
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

pub const MODULE_NAME: &str = "nsemver";
pub const MODULE_PATHS: &[&str] = &["nsemver", "std/nsemver"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_versions() {
        let v = parse_version("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert!(v.pre.is_empty());
        assert!(v.build.is_empty());

        let v = parse_version("1.2.3-alpha.1+build.2").unwrap();
        assert_eq!(join_identifiers(&v.pre), "alpha.1");
        assert_eq!(join_identifiers(&v.build), "build.2");

        let v = parse_version("1.2.3-alpha.1").unwrap();
        assert_eq!(v.patch, 3);
        assert_eq!(join_identifiers(&v.pre), "alpha.1");
    }

    #[test]
    fn parse_rejects_invalid() {
        assert!(parse_version("").is_err());
        assert!(parse_version("1.2").is_err());
        assert!(parse_version("01.2.3").is_err());
        assert!(parse_version("1.02.3").is_err());
        assert!(parse_version("1.2.3-").is_err());
    }

    #[test]
    fn compare_ordering() {
        assert_eq!(compare_str("1.0.0", "2.0.0").unwrap(), -1);
        assert_eq!(compare_str("2.0.0", "1.0.0").unwrap(), 1);
        assert_eq!(compare_str("1.2.3", "1.2.3").unwrap(), 0);
        assert_eq!(compare_str("1.2.3-alpha", "1.2.3").unwrap(), -1);
        assert_eq!(compare_str("1.2.3", "1.2.4").unwrap(), -1);
        assert_eq!(compare_str("1.10.0", "1.2.0").unwrap(), 1);
        assert_eq!(compare_str("1.2.3-alpha", "1.2.3-beta").unwrap(), -1);
        assert_eq!(compare_str("1.2.3-alpha.1", "1.2.3-alpha.2").unwrap(), -1);
    }

    #[test]
    fn caret_ranges() {
        assert!(satisfies_str("1.2.3", "^1.2.3").unwrap());
        assert!(satisfies_str("1.9.9", "^1.2.3").unwrap());
        assert!(!satisfies_str("2.0.0", "^1.2.3").unwrap());
        assert!(!satisfies_str("1.2.2", "^1.2.3").unwrap());
        assert!(satisfies_str("0.2.3", "^0.2.3").unwrap());
        assert!(!satisfies_str("0.3.0", "^0.2.3").unwrap());
        assert!(satisfies_str("0.0.3", "^0.0.3").unwrap());
        assert!(!satisfies_str("0.0.4", "^0.0.3").unwrap());
        assert!(satisfies_str("1.5.0", "^1").unwrap());
        assert!(!satisfies_str("2.0.0", "^1").unwrap());
    }

    #[test]
    fn inc_bumps_and_clears_metadata() {
        assert_eq!(
            nsemver_inc(
                &[
                    Value::String("1.2.3-alpha+meta".into()).ref_cell(),
                    Value::String("patch".into()).ref_cell()
                ],
                Span::dummy()
            )
            .unwrap()
            .borrow()
            .to_string(),
            "1.2.4"
        );
        assert_eq!(
            nsemver_inc(
                &[
                    Value::String("1.2.3".into()).ref_cell(),
                    Value::String("minor".into()).ref_cell()
                ],
                Span::dummy()
            )
            .unwrap()
            .borrow()
            .to_string(),
            "1.3.0"
        );
        assert_eq!(
            nsemver_inc(
                &[
                    Value::String("1.2.3".into()).ref_cell(),
                    Value::String("major".into()).ref_cell()
                ],
                Span::dummy()
            )
            .unwrap()
            .borrow()
            .to_string(),
            "2.0.0"
        );
    }
}
