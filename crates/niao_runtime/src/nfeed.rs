//! Native nfeed standard library — RSS / Atom / JSON Feed parse + generate
//! (~feedparser subset).
//!
//! Import with `import "nfeed"` (or `import "std/nfeed"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_feed::{
    build, build_entry, detect_format, detect_version, emit, format_date, is_valid, parse,
    parse_bytes, parse_date, parallel_parse, sanitize_html, strip_html, Category, ContentPart,
    EmitFormat, EmitOptions, Enclosure, FeedDocument, FeedEntry, FeedError, FeedMeta,
    ParseOptions, Person, MAX_BYTES,
};
use niao_parallel::available_threads;
use std::collections::HashMap;
use std::fs;
use std::rc::Rc;

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E4422_NFEED_TYPE, msg.into())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E4420_NFEED_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn nfeed_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E4421_NFEED_ERROR, "nfeed_error", msg.into(), span)
}

fn map_feed_err(span: Span, err: FeedError) -> ValueRef {
    let code = match &err {
        FeedError::Parse(_) | FeedError::InvalidDate(_) | FeedError::UnknownFormat(_) => {
            codes::E4423_NFEED_PARSE
        }
        _ => codes::E4421_NFEED_ERROR,
    };
    error_value(code, "nfeed_error", err.message(), span)
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

fn optional_object_arg(args: &[ValueRef], idx: usize) -> Option<HashMap<String, ValueRef>> {
    if args.len() <= idx {
        return None;
    }
    match &*args[idx].borrow() {
        Value::Object(map) => Some(map.clone()),
        Value::Nil => None,
        _ => None,
    }
}

fn bool_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: bool) -> bool {
    let Some(map) = map else {
        return default;
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Bool(b)) => b,
        Some(Value::Int(n)) => n != 0,
        Some(Value::String(s)) => matches!(s.as_str(), "true" | "1" | "yes" | "on"),
        _ => default,
    }
}

fn string_field(map: &HashMap<String, ValueRef>, key: &str) -> Option<String> {
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::String(s)) => Some(s),
        Some(Value::Int(n)) => Some(n.to_string()),
        Some(Value::Float(f)) => Some(f.to_string()),
        _ => None,
    }
}

fn parse_opts_from_map(map: Option<&HashMap<String, ValueRef>>) -> ParseOptions {
    ParseOptions {
        sanitize: bool_field(map, "sanitize", false),
        relaxed: bool_field(map, "relaxed", false),
        encoding: None,
    }
}

fn emit_opts_from_map(map: Option<&HashMap<String, ValueRef>>) -> EmitOptions {
    let format = map
        .and_then(|m| string_field(m, "format"))
        .and_then(|s| EmitFormat::parse(&s))
        .unwrap_or(EmitFormat::Rss2);
    EmitOptions {
        format,
        pretty: bool_field(map, "pretty", false),
        indent: map
            .and_then(|m| match m.get("indent").map(|v| v.borrow().clone()) {
                Some(Value::Int(n)) if n >= 0 => Some(n as usize),
                _ => None,
            })
            .unwrap_or(0),
    }
}

fn opt_string(v: Option<String>) -> ValueRef {
    match v {
        Some(s) => Value::String(s).ref_cell(),
        None => Value::Nil.ref_cell(),
    }
}

fn person_to_niao(p: &Person) -> Value {
    let mut m = HashMap::new();
    m.insert("name".into(), opt_string(p.name.clone()));
    m.insert("email".into(), opt_string(p.email.clone()));
    m.insert("uri".into(), opt_string(p.uri.clone()));
    Value::Object(m)
}

fn category_to_niao(c: &Category) -> Value {
    let mut m = HashMap::new();
    m.insert("term".into(), Value::String(c.term.clone()).ref_cell());
    m.insert("scheme".into(), opt_string(c.scheme.clone()));
    m.insert("label".into(), opt_string(c.label.clone()));
    Value::Object(m)
}

fn content_to_niao(c: &ContentPart) -> Value {
    let mut m = HashMap::new();
    m.insert("value".into(), Value::String(c.value.clone()).ref_cell());
    m.insert("type".into(), Value::String(c.mime_type.clone()).ref_cell());
    m.insert("language".into(), opt_string(c.language.clone()));
    m.insert("base".into(), opt_string(c.base.clone()));
    Value::Object(m)
}

fn enclosure_to_niao(e: &Enclosure) -> Value {
    let mut m = HashMap::new();
    m.insert("url".into(), Value::String(e.url.clone()).ref_cell());
    m.insert("type".into(), opt_string(e.mime_type.clone()));
    m.insert("length".into(), match e.length {
        Some(n) => Value::Int(n as i64).ref_cell(),
        None => Value::Nil.ref_cell(),
    });
    m.insert("title".into(), opt_string(e.title.clone()));
    Value::Object(m)
}

fn feed_meta_to_niao(meta: &FeedMeta) -> Value {
    let mut m = HashMap::new();
    m.insert("title".into(), opt_string(meta.title.clone()));
    m.insert("link".into(), opt_string(meta.link.clone()));
    m.insert("id".into(), opt_string(meta.id.clone()));
    m.insert("subtitle".into(), opt_string(meta.subtitle.clone()));
    m.insert("description".into(), opt_string(meta.subtitle.clone()));
    m.insert("rights".into(), opt_string(meta.rights.clone()));
    m.insert("language".into(), opt_string(meta.language.clone()));
    m.insert("updated".into(), opt_string(meta.updated.clone()));
    m.insert(
        "updated_ms".into(),
        match meta.updated_ms {
            Some(n) => Value::Int(n).ref_cell(),
            None => Value::Nil.ref_cell(),
        },
    );
    m.insert("published".into(), opt_string(meta.published.clone()));
    m.insert(
        "published_ms".into(),
        match meta.published_ms {
            Some(n) => Value::Int(n).ref_cell(),
            None => Value::Nil.ref_cell(),
        },
    );
    m.insert("generator".into(), opt_string(meta.generator.clone()));
    m.insert("icon".into(), opt_string(meta.icon.clone()));
    m.insert("logo".into(), opt_string(meta.logo.clone()));
    m.insert(
        "ttl".into(),
        match meta.ttl {
            Some(n) => Value::Int(n).ref_cell(),
            None => Value::Nil.ref_cell(),
        },
    );
    let authors: Vec<ValueRef> = meta.authors.iter().map(|p| person_to_niao(p).ref_cell()).collect();
    m.insert("authors".into(), Value::Array(authors).ref_cell());
    let tags: Vec<ValueRef> = meta
        .categories
        .iter()
        .map(|c| category_to_niao(c).ref_cell())
        .collect();
    m.insert("categories".into(), Value::Array(tags).ref_cell());
    Value::Object(m)
}

fn entry_to_niao(e: &FeedEntry) -> Value {
    let mut m = HashMap::new();
    m.insert("id".into(), opt_string(e.id.clone()));
    m.insert("title".into(), opt_string(e.title.clone()));
    m.insert("link".into(), opt_string(e.link.clone()));
    m.insert("summary".into(), opt_string(e.summary.clone()));
    m.insert(
        "summary_detail".into(),
        match &e.summary_detail {
            Some(c) => content_to_niao(c).ref_cell(),
            None => Value::Nil.ref_cell(),
        },
    );
    let content: Vec<ValueRef> = e.content.iter().map(|c| content_to_niao(c).ref_cell()).collect();
    m.insert("content".into(), Value::Array(content).ref_cell());
    m.insert("published".into(), opt_string(e.published.clone()));
    m.insert(
        "published_ms".into(),
        match e.published_ms {
            Some(n) => Value::Int(n).ref_cell(),
            None => Value::Nil.ref_cell(),
        },
    );
    m.insert("updated".into(), opt_string(e.updated.clone()));
    m.insert(
        "updated_ms".into(),
        match e.updated_ms {
            Some(n) => Value::Int(n).ref_cell(),
            None => Value::Nil.ref_cell(),
        },
    );
    m.insert("author".into(), opt_string(e.author.clone()));
    let authors: Vec<ValueRef> = e.authors.iter().map(|p| person_to_niao(p).ref_cell()).collect();
    m.insert("authors".into(), Value::Array(authors).ref_cell());
    let tags: Vec<ValueRef> = e.tags.iter().map(|c| category_to_niao(c).ref_cell()).collect();
    m.insert("tags".into(), Value::Array(tags).ref_cell());
    let enc: Vec<ValueRef> = e
        .enclosures
        .iter()
        .map(|x| enclosure_to_niao(x).ref_cell())
        .collect();
    m.insert("enclosures".into(), Value::Array(enc).ref_cell());
    m.insert("guid".into(), opt_string(e.guid.clone()));
  m.insert(
        "guid_is_permalink".into(),
        match e.guid_is_permalink {
            Some(b) => Value::Bool(b).ref_cell(),
            None => Value::Nil.ref_cell(),
        },
    );
    Value::Object(m)
}

fn doc_to_niao(doc: &FeedDocument) -> Value {
    let mut m = HashMap::new();
    m.insert("version".into(), Value::String(doc.version.clone()).ref_cell());
    m.insert("bozo".into(), Value::Bool(doc.bozo).ref_cell());
    m.insert(
        "bozo_exception".into(),
        opt_string(doc.bozo_exception.clone()),
    );
    m.insert("encoding".into(), opt_string(doc.encoding.clone()));
    m.insert("feed".into(), feed_meta_to_niao(&doc.feed).ref_cell());
    let entries: Vec<ValueRef> = doc.entries.iter().map(|e| entry_to_niao(e).ref_cell()).collect();
    m.insert("entries".into(), Value::Array(entries).ref_cell());
    Value::Object(m)
}

fn object_field(obj: &HashMap<String, ValueRef>, key: &str) -> Option<String> {
    string_field(obj, key)
}

fn niao_to_entry(v: &ValueRef) -> Result<FeedEntry, String> {
    let obj = match &*v.borrow() {
        Value::Object(m) => m,
        other => return Err(format!("entry must be an object, got {}", other.type_name())),
    };
    let mut fields = HashMap::new();
    for (k, vr) in obj {
        match &*vr.borrow() {
            Value::String(s) => {
                fields.insert(k.clone(), s.clone());
            }
            Value::Int(n) => {
                fields.insert(k.clone(), n.to_string());
            }
            Value::Bool(b) => {
                fields.insert(k.clone(), b.to_string());
            }
            Value::Nil => {}
            _ => {}
        }
    }
    build_entry(&fields)
}

fn niao_to_doc(v: &ValueRef) -> Result<FeedDocument, String> {
    if let Ok(entry_only) = niao_to_entry(v) {
        let mut doc = FeedDocument::new("rss20");
        doc.entries.push(entry_only);
        return Ok(doc);
    }
    let obj = match &*v.borrow() {
        Value::Object(m) => m,
        other => return Err(format!("feed must be an object, got {}", other.type_name())),
    };
    if let Some(version) = object_field(obj, "version") {
        let mut doc = FeedDocument::new(version);
        doc.bozo = matches!(
            obj.get("bozo").map(|v| v.borrow().clone()),
            Some(Value::Bool(true))
        );
        if let Some(feed_obj) = obj.get("feed").and_then(|v| match &*v.borrow() {
            Value::Object(m) => Some(m.clone()),
            _ => None,
        }) {
            doc.feed.title = object_field(&feed_obj, "title");
            doc.feed.link = object_field(&feed_obj, "link");
            doc.feed.id = object_field(&feed_obj, "id");
            doc.feed.subtitle = object_field(&feed_obj, "subtitle")
                .or_else(|| object_field(&feed_obj, "description"));
            doc.feed.language = object_field(&feed_obj, "language");
            doc.feed.rights = object_field(&feed_obj, "rights");
            doc.feed.generator = object_field(&feed_obj, "generator");
            doc.feed.icon = object_field(&feed_obj, "icon");
            doc.feed.logo = object_field(&feed_obj, "logo");
            if let Some(ms) = feed_obj.get("updated_ms").and_then(|v| match &*v.borrow() {
                Value::Int(n) => Some(*n),
                _ => None,
            }) {
                doc.feed.updated_ms = Some(ms);
            }
        }
        if let Some(items) = obj.get("entries").and_then(|v| match &*v.borrow() {
            Value::Array(a) => Some(a.clone()),
            _ => None,
        }) {
            for item in items {
                doc.entries.push(niao_to_entry(&item)?);
            }
        }
        return Ok(doc);
    }
    let mut fields = HashMap::new();
    for (k, vr) in obj {
        if let Value::String(s) = &*vr.borrow() {
            fields.insert(k.clone(), s.clone());
        }
    }
    let mut doc = build(&fields)?;
    if let Some(items) = obj.get("entries").and_then(|v| match &*v.borrow() {
        Value::Array(a) => Some(a.clone()),
        _ => None,
    }) {
        for item in items {
            doc.entries.push(niao_to_entry(&item)?);
        }
    }
    Ok(doc)
}

// >>> nfeed.parse("<rss version=\"2.0\"><channel><title>A</title></channel></rss>").feed.title
// => "A"
fn nfeed_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nfeed_parse", span)?;
    let text = string_arg(args, 0, "nfeed_parse", span)?;
    let opts = parse_opts_from_map(optional_object_arg(args, 1).as_ref());
    match parse(&text, &opts) {
        Ok(doc) => Ok(doc_to_niao(&doc).ref_cell()),
        Err(e) => Ok(map_feed_err(span, e)),
    }
}

fn nfeed_parse_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nfeed_parse_bytes", span)?;
    let bytes = match &*args[0].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::Int(n) if (0..=255).contains(n) => out.push(*n as u8),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "nfeed_parse_bytes() byte {} must be 0..255 int, got {}",
                                i,
                                other.type_name()
                            ),
                        ))
                    }
                }
            }
            out
        }
        Value::String(s) => s.as_bytes().to_vec(),
        other => {
            return Err(type_err(
                span,
                format!(
                    "nfeed_parse_bytes() expects byte array or string, got {}",
                    other.type_name()
                ),
            ))
        }
    };
    let opts = parse_opts_from_map(optional_object_arg(args, 1).as_ref());
    match parse_bytes(&bytes, &opts) {
        Ok(doc) => Ok(doc_to_niao(&doc).ref_cell()),
        Err(e) => Ok(map_feed_err(span, e)),
    }
}

fn nfeed_parse_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nfeed_parse_file", span)?;
    let path = string_arg(args, 0, "nfeed_parse_file", span)?;
    let bytes = fs::read(&path).map_err(|e| {
        RuntimeError::at(
            span,
            codes::E4421_NFEED_ERROR,
            format!("nfeed_parse_file: cannot read '{path}': {e}"),
        )
    })?;
    if bytes.len() > MAX_BYTES {
        return Ok(map_feed_err(span, FeedError::TooLarge(bytes.len())));
    }
    let mut file_args = vec![Value::Array(bytes.iter().map(|b| Value::Int(*b as i64).ref_cell()).collect()).ref_cell()];
    if args.len() > 1 {
        file_args.push(args[1].clone());
    }
    nfeed_parse_bytes(&file_args, span)
}

fn nfeed_parse_many(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nfeed_parse_many", span)?;
    let texts = match &*args[0].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "nfeed_parse_many() item {} must be string, got {}",
                                i,
                                other.type_name()
                            ),
                        ))
                    }
                }
            }
            out
        }
        other => {
            return Err(type_err(
                span,
                format!(
                    "nfeed_parse_many() expects string array, got {}",
                    other.type_name()
                ),
            ))
        }
    };
    let opts = parse_opts_from_map(optional_object_arg(args, 1).as_ref());
    let threads = available_threads().max(1);
    let results = parallel_parse(&texts, &opts, threads);
    let arr: Vec<ValueRef> = results
        .into_iter()
        .map(|r| match r {
            Ok(doc) => doc_to_niao(&doc).ref_cell(),
            Err(e) => map_feed_err(span, e),
        })
        .collect();
    Ok(Value::Array(arr).ref_cell())
}

// >>> nfeed.valid("<rss version=\"2.0\"><channel><title>A</title></channel></rss>")
// => true
fn nfeed_valid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nfeed_valid", span)?;
    let text = string_arg(args, 0, "nfeed_valid", span)?;
    Ok(Value::Bool(is_valid(&text)).ref_cell())
}

fn nfeed_detect(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nfeed_detect", span)?;
    let text = string_arg(args, 0, "nfeed_detect", span)?;
    Ok(match detect_format(text.as_bytes()) {
        Some(s) => Value::String(s).ref_cell(),
        None => Value::Nil.ref_cell(),
    })
}

fn nfeed_detect_version(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nfeed_detect_version", span)?;
    let text = string_arg(args, 0, "nfeed_detect_version", span)?;
    Ok(match detect_version(text.as_bytes()) {
        Some(s) => Value::String(s).ref_cell(),
        None => Value::Nil.ref_cell(),
    })
}

fn nfeed_emit(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nfeed_emit", span)?;
    let doc = match niao_to_doc(&args[0]) {
        Ok(d) => d,
        Err(msg) => return Ok(nfeed_err(span, msg)),
    };
    let opts = emit_opts_from_map(optional_object_arg(args, 1).as_ref());
    match emit(&doc, &opts) {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(map_feed_err(span, e)),
    }
}

fn nfeed_emit_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nfeed_emit_file", span)?;
    let path = string_arg(args, 0, "nfeed_emit_file", span)?;
    let emit_args = if args.len() > 2 {
        vec![args[1].clone(), args[2].clone()]
    } else {
        vec![args[1].clone()]
    };
    let out = nfeed_emit(&emit_args, span)?;
    if matches!(&*out.borrow(), Value::Error { .. }) {
        return Ok(out);
    }
    let text = match &*out.borrow() {
        Value::String(s) => s.clone(),
        other => {
            return Err(type_err(
                span,
                format!("nfeed_emit_file: expected string from emit, got {}", other.type_name()),
            ))
        }
    };
    fs::write(&path, text).map_err(|e| {
        RuntimeError::at(
            span,
            codes::E4421_NFEED_ERROR,
            format!("nfeed_emit_file: cannot write '{path}': {e}"),
        )
    })?;
    Ok(Value::Bool(true).ref_cell())
}

fn nfeed_build(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nfeed_build", span)?;
    let map = optional_object_arg(args, 0).unwrap_or_default();
    let title = string_field(&map, "title");
    let link = string_field(&map, "link");
    let mut doc = FeedDocument::new(
        string_field(&map, "version").unwrap_or_else(|| "rss20".into()),
    );
    doc.feed.title = title;
    doc.feed.link = link;
    doc.feed.id = string_field(&map, "id");
    doc.feed.subtitle = string_field(&map, "subtitle").or_else(|| string_field(&map, "description"));
    doc.feed.language = string_field(&map, "language");
    doc.feed.rights = string_field(&map, "rights");
    doc.feed.generator = string_field(&map, "generator");
    if let Some(items) = map.get("entries").and_then(|v| match &*v.borrow() {
        Value::Array(a) => Some(a.clone()),
        _ => None,
    }) {
        for item in items {
            match niao_to_entry(&item) {
                Ok(e) => doc.entries.push(e),
                Err(msg) => return Ok(nfeed_err(span, msg)),
            }
        }
    }
    Ok(doc_to_niao(&doc).ref_cell())
}

fn nfeed_build_entry(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nfeed_build_entry", span)?;
    let map = optional_object_arg(args, 0).unwrap_or_default();
    let mut fields = HashMap::new();
    for (k, vr) in &map {
        match &*vr.borrow() {
            Value::String(s) => {
                fields.insert(k.clone(), s.clone());
            }
            Value::Int(n) => {
                fields.insert(k.clone(), n.to_string());
            }
            Value::Bool(b) => {
                fields.insert(k.clone(), b.to_string());
            }
            _ => {}
        }
    }
    match build_entry(&fields) {
        Ok(e) => Ok(entry_to_niao(&e).ref_cell()),
        Err(e) => Ok(map_feed_err(span, e)),
    }
}

fn nfeed_entries(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nfeed_entries", span)?;
    match &*args[0].borrow() {
        Value::Object(m) => {
            if let Some(e) = m.get("entries") {
                return Ok(e.clone());
            }
            Ok(Value::Array(Vec::new()).ref_cell())
        }
        other => Err(type_err(
            span,
            format!("nfeed_entries() expects feed object, got {}", other.type_name()),
        )),
    }
}

fn nfeed_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "nfeed_get", span)?;
    let name = string_arg(args, 1, "nfeed_get", span)?;
    let key = name.to_ascii_lowercase();
    match &*args[0].borrow() {
        Value::Object(m) => {
            if let Some(v) = m.get(&key) {
                return Ok(v.clone());
            }
            if key == "description" {
                if let Some(v) = m.get("subtitle") {
                    return Ok(v.clone());
                }
            }
            Ok(Value::Nil.ref_cell())
        }
        other => Err(type_err(
            span,
            format!("nfeed_get() expects object, got {}", other.type_name()),
        )),
    }
}

fn nfeed_strip_html(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nfeed_strip_html", span)?;
    let html = string_arg(args, 0, "nfeed_strip_html", span)?;
    Ok(Value::String(strip_html(&html)).ref_cell())
}

fn nfeed_sanitize_html(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nfeed_sanitize_html", span)?;
    let html = string_arg(args, 0, "nfeed_sanitize_html", span)?;
    match sanitize_html(&html, None) {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(nfeed_err(span, e.message().to_string())),
    }
}

fn nfeed_parse_date(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nfeed_parse_date", span)?;
    let raw = string_arg(args, 0, "nfeed_parse_date", span)?;
    match parse_date(&raw) {
        Ok(d) => {
            let mut m = HashMap::new();
            m.insert("raw".into(), Value::String(d.raw).ref_cell());
            m.insert("iso".into(), Value::String(d.iso).ref_cell());
            m.insert("unix_ms".into(), Value::Int(d.unix_ms).ref_cell());
            Ok(Value::Object(m).ref_cell())
        }
        Err(e) => Ok(map_feed_err(span, e)),
    }
}

fn nfeed_format_date(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nfeed_format_date", span)?;
    let ms = match &*args[0].borrow() {
        Value::Int(n) => *n,
        other => {
            return Err(type_err(
                span,
                format!("nfeed_format_date() expects int unix_ms, got {}", other.type_name()),
            ))
        }
    };
    Ok(Value::String(format_date(ms)).ref_cell())
}

macro_rules! nfeed_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nfeed_fns![
    ("nfeed_parse", "parse", nfeed_parse),
    ("nfeed_parse_bytes", "parse_bytes", nfeed_parse_bytes),
    ("nfeed_parse_file", "parse_file", nfeed_parse_file),
    ("nfeed_parse_many", "parse_many", nfeed_parse_many),
    ("nfeed_valid", "valid", nfeed_valid),
    ("nfeed_detect", "detect", nfeed_detect),
    ("nfeed_detect_version", "detect_version", nfeed_detect_version),
    ("nfeed_emit", "emit", nfeed_emit),
    ("nfeed_emit_file", "emit_file", nfeed_emit_file),
    ("nfeed_build", "build", nfeed_build),
    ("nfeed_build_entry", "build_entry", nfeed_build_entry),
    ("nfeed_entries", "entries", nfeed_entries),
    ("nfeed_get", "get", nfeed_get),
    ("nfeed_strip_html", "strip_html", nfeed_strip_html),
    ("nfeed_sanitize_html", "sanitize_html", nfeed_sanitize_html),
    ("nfeed_parse_date", "parse_date", nfeed_parse_date),
    ("nfeed_format_date", "format_date", nfeed_format_date),
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

pub const MODULE_NAME: &str = "nfeed";
pub const MODULE_PATHS: &[&str] = &["nfeed", "std/nfeed"];

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
    fn parse_rss() {
        let xml = "<rss version=\"2.0\"><channel><title>T</title><item><title>E</title></item></channel></rss>";
        let args = [Value::String(xml.into()).ref_cell()];
        let v = nfeed_parse(&args, span()).unwrap();
        match &*v.borrow() {
            Value::Object(m) => {
                assert!(m.contains_key("entries"));
            }
            other => panic!("{other:?}"),
        }
    }
}
