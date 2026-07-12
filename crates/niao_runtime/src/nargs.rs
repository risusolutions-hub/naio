//! Native nargs standard library — declarative CLI argument parsing:
//! flags, typed options, positionals, `--key=value`, short bundling (`-abc`),
//! `--` terminator, and generated `--help` text. Std-only, zero deps.
//!
//! Import with `import "nargs"` (or `import "std/nargs"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::collections::HashMap;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// Spec model
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum OptType {
    Str,
    Int,
    Float,
    Bool,
}

impl OptType {
    fn parse(name: &str) -> Option<OptType> {
        match name {
            "string" | "str" => Some(OptType::Str),
            "int" => Some(OptType::Int),
            "float" => Some(OptType::Float),
            "bool" => Some(OptType::Bool),
            _ => None,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            OptType::Str => "string",
            OptType::Int => "int",
            OptType::Float => "float",
            OptType::Bool => "bool",
        }
    }
}

struct FlagSpec {
    name: String,
    short: Option<char>,
    help: String,
}

struct OptSpec {
    name: String,
    short: Option<char>,
    ty: OptType,
    default: Option<Value>,
    required: bool,
    help: String,
}

struct PosSpec {
    name: String,
    required: bool,
    variadic: bool,
    help: String,
}

struct Spec {
    name: String,
    about: String,
    flags: Vec<FlagSpec>,
    options: Vec<OptSpec>,
    positionals: Vec<PosSpec>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::TypeError {
        message: msg.into(),
        line: span.line,
        col: span.col,
    }
}

fn spec_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E2652_NARGS_SPEC, msg.into())
}

fn parse_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E2651_NARGS_PARSE, "nargs_error", msg.into(), span)
}

fn obj_str(map: &HashMap<String, ValueRef>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.clone()),
        _ => None,
    })
}

fn obj_bool(map: &HashMap<String, ValueRef>, key: &str, default: bool) -> bool {
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Bool(b)) => b,
        Some(Value::Int(n)) => n != 0,
        None => default,
        _ => default,
    }
}

fn short_char(map: &HashMap<String, ValueRef>, span: Span) -> NiaoResult<Option<char>> {
    match obj_str(map, "short") {
        Some(s) => {
            let mut chars = s.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Ok(Some(c)),
                _ => Err(spec_err(span, format!("short alias must be one character, got '{s}'"))),
            }
        }
        None => Ok(None),
    }
}

fn spec_list<'a>(
    map: &'a HashMap<String, ValueRef>,
    key: &str,
    span: Span,
) -> NiaoResult<Vec<HashMap<String, ValueRef>>> {
    let Some(v) = map.get(key) else {
        return Ok(Vec::new());
    };
    match &*v.borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match &*item.borrow() {
                    Value::Object(o) => out.push(o.clone()),
                    other => {
                        return Err(spec_err(
                            span,
                            format!("spec.{key} entries must be objects, found {}", other.type_name()),
                        ))
                    }
                }
            }
            Ok(out)
        }
        Value::Nil => Ok(Vec::new()),
        other => Err(spec_err(
            span,
            format!("spec.{key} must be an array, got {}", other.type_name()),
        )),
    }
}

fn build_spec(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Spec> {
    let map = match &*args[idx].borrow() {
        Value::Object(map) => map.clone(),
        other => {
            return Err(type_err(
                span,
                format!("{name}() expects a spec object, got {}", other.type_name()),
            ))
        }
    };
    let mut spec = Spec {
        name: obj_str(&map, "name").unwrap_or_else(|| "program".to_string()),
        about: obj_str(&map, "about").unwrap_or_default(),
        flags: Vec::new(),
        options: Vec::new(),
        positionals: Vec::new(),
    };
    for f in spec_list(&map, "flags", span)? {
        let Some(fname) = obj_str(&f, "name") else {
            return Err(spec_err(span, "flag entry missing 'name'"));
        };
        spec.flags.push(FlagSpec {
            short: short_char(&f, span)?,
            help: obj_str(&f, "help").unwrap_or_default(),
            name: fname,
        });
    }
    for o in spec_list(&map, "options", span)? {
        let Some(oname) = obj_str(&o, "name") else {
            return Err(spec_err(span, "option entry missing 'name'"));
        };
        let ty = match obj_str(&o, "type") {
            Some(t) => OptType::parse(&t)
                .ok_or_else(|| spec_err(span, format!("option '{oname}' has unknown type '{t}'")))?,
            None => OptType::Str,
        };
        let default = o.get("default").map(|v| v.borrow().clone());
        spec.options.push(OptSpec {
            short: short_char(&o, span)?,
            required: obj_bool(&o, "required", false),
            help: obj_str(&o, "help").unwrap_or_default(),
            name: oname,
            ty,
            default,
        });
    }
    let mut seen_variadic = false;
    for p in spec_list(&map, "positionals", span)? {
        let Some(pname) = obj_str(&p, "name") else {
            return Err(spec_err(span, "positional entry missing 'name'"));
        };
        if seen_variadic {
            return Err(spec_err(span, "variadic positional must be last"));
        }
        let variadic = obj_bool(&p, "variadic", false);
        seen_variadic = variadic;
        spec.positionals.push(PosSpec {
            required: obj_bool(&p, "required", false),
            help: obj_str(&p, "help").unwrap_or_default(),
            name: pname,
            variadic,
        });
    }
    // Reject duplicate names/shorts up front.
    let mut seen_names: Vec<&str> = Vec::new();
    let mut seen_shorts: Vec<char> = Vec::new();
    for (n, s) in spec
        .flags
        .iter()
        .map(|f| (f.name.as_str(), f.short))
        .chain(spec.options.iter().map(|o| (o.name.as_str(), o.short)))
    {
        if seen_names.contains(&n) {
            return Err(spec_err(span, format!("duplicate flag/option name '{n}'")));
        }
        seen_names.push(n);
        if let Some(c) = s {
            if seen_shorts.contains(&c) {
                return Err(spec_err(span, format!("duplicate short alias '-{c}'")));
            }
            seen_shorts.push(c);
        }
    }
    Ok(spec)
}

fn argv_list(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<String>> {
    match &*args[idx].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match &*item.borrow() {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!("{name}() argv must contain strings, found {}", other.type_name()),
                        ))
                    }
                }
            }
            Ok(out)
        }
        Value::StringArray(sa) => Ok(sa.dense_vec()),
        other => Err(type_err(
            span,
            format!("{name}() expects an argv array, got {}", other.type_name()),
        )),
    }
}

fn convert_typed(raw: &str, ty: OptType, opt_name: &str, span: Span) -> Result<Value, ValueRef> {
    match ty {
        OptType::Str => Ok(Value::String(raw.to_string())),
        OptType::Int => raw
            .parse::<i64>()
            .map(Value::Int)
            .map_err(|_| parse_err(span, format!("option --{opt_name} expects an int, got '{raw}'"))),
        OptType::Float => raw
            .parse::<f64>()
            .map(Value::Float)
            .map_err(|_| parse_err(span, format!("option --{opt_name} expects a float, got '{raw}'"))),
        OptType::Bool => match raw.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Ok(Value::Bool(true)),
            "false" | "0" | "no" | "off" => Ok(Value::Bool(false)),
            _ => Err(parse_err(
                span,
                format!("option --{opt_name} expects a bool, got '{raw}'"),
            )),
        },
    }
}

// ---------------------------------------------------------------------------
// Help text
// ---------------------------------------------------------------------------

fn help_text(spec: &Spec) -> String {
    let mut out = String::new();
    if spec.about.is_empty() {
        out.push_str(&spec.name);
    } else {
        out.push_str(&format!("{} — {}", spec.name, spec.about));
    }
    out.push_str("\n\nUsage: ");
    out.push_str(&spec.name);
    if !spec.flags.is_empty() || !spec.options.is_empty() {
        out.push_str(" [options]");
    }
    for p in &spec.positionals {
        if p.variadic {
            out.push_str(&format!(" [{}...]", p.name));
        } else if p.required {
            out.push_str(&format!(" <{}>", p.name));
        } else {
            out.push_str(&format!(" [{}]", p.name));
        }
    }
    out.push('\n');
    if !spec.positionals.is_empty() {
        out.push_str("\nArguments:\n");
        for p in &spec.positionals {
            out.push_str(&format!("  {:<20} {}\n", p.name, p.help));
        }
    }
    out.push_str("\nOptions:\n");
    for f in &spec.flags {
        let lhs = match f.short {
            Some(c) => format!("-{c}, --{}", f.name),
            None => format!("    --{}", f.name),
        };
        out.push_str(&format!("  {lhs:<24} {}\n", f.help));
    }
    for o in &spec.options {
        let lhs = match o.short {
            Some(c) => format!("-{c}, --{} <{}>", o.name, o.ty.label()),
            None => format!("    --{} <{}>", o.name, o.ty.label()),
        };
        let mut rhs = o.help.clone();
        if let Some(d) = &o.default {
            if !rhs.is_empty() {
                rhs.push(' ');
            }
            rhs.push_str(&format!("(default: {})", d.to_string()));
        }
        out.push_str(&format!("  {lhs:<24} {rhs}\n"));
    }
    out.push_str(&format!("  {:<24} Show this help\n", "-h, --help"));
    out
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

enum Owner {
    Flag(usize),
    Opt(usize),
}

fn lookup_long<'a>(spec: &'a Spec, name: &str) -> Option<Owner> {
    if let Some(i) = spec.flags.iter().position(|f| f.name == name) {
        return Some(Owner::Flag(i));
    }
    if let Some(i) = spec.options.iter().position(|o| o.name == name) {
        return Some(Owner::Opt(i));
    }
    None
}

fn lookup_short(spec: &Spec, c: char) -> Option<Owner> {
    if let Some(i) = spec.flags.iter().position(|f| f.short == Some(c)) {
        return Some(Owner::Flag(i));
    }
    if let Some(i) = spec.options.iter().position(|o| o.short == Some(c)) {
        return Some(Owner::Opt(i));
    }
    None
}

fn parse_impl(spec: &Spec, argv: &[String], span: Span) -> Result<ValueRef, ValueRef> {
    let mut values: HashMap<String, ValueRef> = HashMap::new();
    let mut positional_raw: Vec<String> = Vec::new();
    let mut rest: Vec<String> = Vec::new();
    let mut after_terminator = false;

    // Defaults
    for f in &spec.flags {
        values.insert(f.name.clone(), Value::Bool(false).ref_cell());
    }
    for o in &spec.options {
        let v = o.default.clone().unwrap_or(Value::Nil);
        values.insert(o.name.clone(), v.ref_cell());
    }

    let mut i = 0usize;
    while i < argv.len() {
        let tok = &argv[i];
        if after_terminator {
            rest.push(tok.clone());
            i += 1;
            continue;
        }
        if tok == "--" {
            after_terminator = true;
            i += 1;
            continue;
        }
        if tok == "--help" || tok == "-h" {
            let mut out: HashMap<String, ValueRef> = HashMap::new();
            out.insert("ok".to_string(), Value::Bool(true).ref_cell());
            out.insert("help".to_string(), Value::Bool(true).ref_cell());
            out.insert("text".to_string(), Value::String(help_text(spec)).ref_cell());
            out.insert("values".to_string(), Value::Object(HashMap::new()).ref_cell());
            out.insert("rest".to_string(), Value::Array(Vec::new()).ref_cell());
            return Ok(Value::Object(out).ref_cell());
        }
        if let Some(body) = tok.strip_prefix("--") {
            let (name, inline_value) = match body.split_once('=') {
                Some((n, v)) => (n.to_string(), Some(v.to_string())),
                None => (body.to_string(), None),
            };
            match lookup_long(spec, &name) {
                Some(Owner::Flag(fi)) => {
                    if inline_value.is_some() {
                        return Err(parse_err(span, format!("flag --{name} does not take a value")));
                    }
                    values.insert(spec.flags[fi].name.clone(), Value::Bool(true).ref_cell());
                }
                Some(Owner::Opt(oi)) => {
                    let raw = match inline_value {
                        Some(v) => v,
                        None => {
                            i += 1;
                            match argv.get(i) {
                                Some(v) => v.clone(),
                                None => {
                                    return Err(parse_err(
                                        span,
                                        format!("option --{name} is missing its value"),
                                    ))
                                }
                            }
                        }
                    };
                    let opt = &spec.options[oi];
                    let v = convert_typed(&raw, opt.ty, &opt.name, span)?;
                    values.insert(opt.name.clone(), v.ref_cell());
                }
                None => return Err(parse_err(span, format!("unknown option --{name}"))),
            }
            i += 1;
            continue;
        }
        if tok.len() > 1 && tok.starts_with('-') && !tok[1..].starts_with('-') {
            // short cluster: -v, -abc, -p VALUE (value-taking short must be last)
            let shorts: Vec<char> = tok[1..].chars().collect();
            for (pos, c) in shorts.iter().enumerate() {
                match lookup_short(spec, *c) {
                    Some(Owner::Flag(fi)) => {
                        values.insert(spec.flags[fi].name.clone(), Value::Bool(true).ref_cell());
                    }
                    Some(Owner::Opt(oi)) => {
                        if pos != shorts.len() - 1 {
                            return Err(parse_err(
                                span,
                                format!("short option -{c} takes a value and must be last in '-{}'", tok.trim_start_matches('-')),
                            ));
                        }
                        i += 1;
                        let raw = match argv.get(i) {
                            Some(v) => v.clone(),
                            None => {
                                return Err(parse_err(span, format!("option -{c} is missing its value")))
                            }
                        };
                        let opt = &spec.options[oi];
                        let v = convert_typed(&raw, opt.ty, &opt.name, span)?;
                        values.insert(opt.name.clone(), v.ref_cell());
                    }
                    None => return Err(parse_err(span, format!("unknown option -{c}"))),
                }
            }
            i += 1;
            continue;
        }
        positional_raw.push(tok.clone());
        i += 1;
    }

    // Required options
    for o in &spec.options {
        if o.required {
            let missing = values
                .get(&o.name)
                .map(|v| matches!(&*v.borrow(), Value::Nil))
                .unwrap_or(true);
            if missing {
                return Err(parse_err(span, format!("missing required option --{}", o.name)));
            }
        }
    }

    // Positionals
    let mut pos_iter = positional_raw.into_iter();
    for p in &spec.positionals {
        if p.variadic {
            let remainder: Vec<ValueRef> = pos_iter
                .by_ref()
                .map(|s| Value::String(s).ref_cell())
                .collect();
            if p.required && remainder.is_empty() {
                return Err(parse_err(span, format!("missing required argument <{}>", p.name)));
            }
            values.insert(p.name.clone(), Value::Array(remainder).ref_cell());
        } else {
            match pos_iter.next() {
                Some(v) => {
                    values.insert(p.name.clone(), Value::String(v).ref_cell());
                }
                None => {
                    if p.required {
                        return Err(parse_err(span, format!("missing required argument <{}>", p.name)));
                    }
                    values.insert(p.name.clone(), Value::Nil.ref_cell());
                }
            }
        }
    }
    // Extra positionals not covered by the spec
    rest.extend(pos_iter);

    let mut out: HashMap<String, ValueRef> = HashMap::new();
    out.insert("ok".to_string(), Value::Bool(true).ref_cell());
    out.insert("help".to_string(), Value::Bool(false).ref_cell());
    out.insert("values".to_string(), Value::Object(values).ref_cell());
    out.insert(
        "rest".to_string(),
        Value::Array(rest.into_iter().map(|s| Value::String(s).ref_cell()).collect()).ref_cell(),
    );
    Ok(Value::Object(out).ref_cell())
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn nargs_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if args.len() != 2 {
        return Err(RuntimeError::at(
            span,
            codes::E2650_NARGS_ARITY,
            format!("nargs_parse() expects 2 arguments (spec, argv), got {}", args.len()),
        ));
    }
    let spec = build_spec(args, 0, "nargs_parse", span)?;
    let argv = argv_list(args, 1, "nargs_parse", span)?;
    match parse_impl(&spec, &argv, span) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

fn nargs_parse_env(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if args.len() != 1 {
        return Err(RuntimeError::at(
            span,
            codes::E2650_NARGS_ARITY,
            format!("nargs_parse_env() expects 1 argument (spec), got {}", args.len()),
        ));
    }
    let spec = build_spec(args, 0, "nargs_parse_env", span)?;
    let argv: Vec<String> = std::env::args().skip(1).collect();
    match parse_impl(&spec, &argv, span) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

fn nargs_help(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if args.len() != 1 {
        return Err(RuntimeError::at(
            span,
            codes::E2650_NARGS_ARITY,
            format!("nargs_help() expects 1 argument (spec), got {}", args.len()),
        ));
    }
    let spec = build_spec(args, 0, "nargs_help", span)?;
    Ok(Value::String(help_text(&spec)).ref_cell())
}

fn nargs_argv(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if !args.is_empty() {
        return Err(RuntimeError::at(
            span,
            codes::E2650_NARGS_ARITY,
            "nargs_argv() expects 0 arguments",
        ));
    }
    let items: Vec<ValueRef> = std::env::args()
        .skip(1)
        .map(|s| Value::String(s).ref_cell())
        .collect();
    Ok(Value::Array(items).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nargs_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nargs_fns![
    ("nargs_parse", "parse", nargs_parse),
    ("nargs_parse_env", "parse_env", nargs_parse_env),
    ("nargs_help", "help", nargs_help),
    ("nargs_argv", "argv", nargs_argv),
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

pub const MODULE_NAME: &str = "nargs";
pub const MODULE_PATHS: &[&str] = &["nargs", "std/nargs"];

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

    fn obj(entries: Vec<(&str, Value)>) -> ValueRef {
        let mut map = HashMap::new();
        for (k, v) in entries {
            map.insert(k.to_string(), v.ref_cell());
        }
        Value::Object(map).ref_cell()
    }

    fn arr(items: Vec<Value>) -> Value {
        Value::Array(items.into_iter().map(|v| v.ref_cell()).collect())
    }

    fn sample_spec() -> ValueRef {
        let verbose = match &*obj(vec![
            ("name", Value::String("verbose".into())),
            ("short", Value::String("v".into())),
        ])
        .borrow()
        {
            v => v.clone(),
        };
        let port = match &*obj(vec![
            ("name", Value::String("port".into())),
            ("short", Value::String("p".into())),
            ("type", Value::String("int".into())),
            ("default", Value::Int(8080)),
        ])
        .borrow()
        {
            v => v.clone(),
        };
        let input = match &*obj(vec![
            ("name", Value::String("input".into())),
            ("required", Value::Bool(true)),
        ])
        .borrow()
        {
            v => v.clone(),
        };
        obj(vec![
            ("name", Value::String("demo".into())),
            ("flags", arr(vec![verbose])),
            ("options", arr(vec![port])),
            ("positionals", arr(vec![input])),
        ])
    }

    fn get_value(result: &ValueRef, key: &str) -> Value {
        match &*result.borrow() {
            Value::Object(map) => match &*map.get("values").unwrap().borrow() {
                Value::Object(values) => values.get(key).unwrap().borrow().clone(),
                other => panic!("values not an object: {other:?}"),
            },
            other => panic!("result not an object: {other:?}"),
        }
    }

    fn argv(items: &[&str]) -> ValueRef {
        arr(items.iter().map(|s| Value::String(s.to_string())).collect()).ref_cell()
    }

    #[test]
    fn parses_flags_options_positionals() {
        let r = nargs_parse(&[sample_spec(), argv(&["-v", "--port", "9090", "in.txt"])], span()).unwrap();
        assert!(matches!(get_value(&r, "verbose"), Value::Bool(true)));
        assert!(matches!(get_value(&r, "port"), Value::Int(9090)));
        match get_value(&r, "input") {
            Value::String(s) => assert_eq!(s, "in.txt"),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn defaults_apply() {
        let r = nargs_parse(&[sample_spec(), argv(&["in.txt"])], span()).unwrap();
        assert!(matches!(get_value(&r, "port"), Value::Int(8080)));
        assert!(matches!(get_value(&r, "verbose"), Value::Bool(false)));
    }

    #[test]
    fn inline_equals_value() {
        let r = nargs_parse(&[sample_spec(), argv(&["--port=7000", "in.txt"])], span()).unwrap();
        assert!(matches!(get_value(&r, "port"), Value::Int(7000)));
    }

    #[test]
    fn unknown_option_is_error_value() {
        let r = nargs_parse(&[sample_spec(), argv(&["--bogus"])], span()).unwrap();
        assert!(matches!(&*r.borrow(), Value::Error(_)));
    }

    #[test]
    fn missing_required_positional() {
        let r = nargs_parse(&[sample_spec(), argv(&[])], span()).unwrap();
        assert!(matches!(&*r.borrow(), Value::Error(_)));
    }

    #[test]
    fn bad_int_value_is_error() {
        let r = nargs_parse(&[sample_spec(), argv(&["--port", "abc", "in.txt"])], span()).unwrap();
        assert!(matches!(&*r.borrow(), Value::Error(_)));
    }

    #[test]
    fn help_flag_short_circuits() {
        let r = nargs_parse(&[sample_spec(), argv(&["--help"])], span()).unwrap();
        match &*r.borrow() {
            Value::Object(map) => {
                assert!(matches!(&*map.get("help").unwrap().borrow(), Value::Bool(true)));
                match &*map.get("text").unwrap().borrow() {
                    Value::String(s) => assert!(s.contains("Usage: demo")),
                    other => panic!("expected string, got {other:?}"),
                }
            }
            other => panic!("expected object, got {other:?}"),
        };
    }
}
