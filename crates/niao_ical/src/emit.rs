use crate::component::Component;
use crate::error::{IcalError, MAX_BYTES};
use crate::property::{escape_value, Property};

/// Emit options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmitOptions {
    pub fold_lines: bool,
    pub crlf: bool,
}

impl Default for EmitOptions {
    fn default() -> Self {
        Self {
            fold_lines: true,
            crlf: true,
        }
    }
}

/// Serialize a component tree to iCalendar / vCard text.
///
/// >>> use niao_ical::{Component, emit, EmitOptions};
/// >>> let c = Component::new("VCARD").with_property(niao_ical::Property::new("FN", "Ada"));
/// >>> emit(&c, &EmitOptions::default()).contains("FN:Ada")
/// true
pub fn emit(component: &Component, opts: &EmitOptions) -> Result<String, IcalError> {
    let mut buf = String::new();
    emit_component(component, &mut buf, opts)?;
    if buf.len() > MAX_BYTES {
        return Err(IcalError::TooLarge(buf.len()));
    }
    Ok(buf)
}

/// Serialize multiple root components (e.g. vCard bundle).
pub fn emit_all(components: &[Component], opts: &EmitOptions) -> Result<String, IcalError> {
    let mut buf = String::new();
    for (i, c) in components.iter().enumerate() {
        if i > 0 {
            write_eol(&mut buf, opts);
        }
        emit_component(c, &mut buf, opts)?;
    }
    if buf.len() > MAX_BYTES {
        return Err(IcalError::TooLarge(buf.len()));
    }
    Ok(buf)
}

fn emit_component(c: &Component, buf: &mut String, opts: &EmitOptions) -> Result<(), IcalError> {
    write_line(buf, &format!("BEGIN:{}", c.name), opts);
    for p in &c.properties {
        emit_property(p, buf, opts);
    }
    for child in &c.children {
        emit_component(child, buf, opts)?;
    }
    write_line(buf, &format!("END:{}", c.name), opts);
    Ok(())
}

fn emit_property(p: &Property, buf: &mut String, opts: &EmitOptions) {
    let mut line = String::with_capacity(p.name.len() + p.value.len() + 8);
    line.push_str(&p.name);
    let mut keys: Vec<_> = p.params.keys().collect();
    keys.sort();
    for k in keys {
        for v in &p.params[k] {
            line.push(';');
            line.push_str(k);
            if !v.is_empty() {
                line.push('=');
                line.push_str(v);
            }
        }
    }
    line.push(':');
    line.push_str(&escape_value(&p.value));
    write_line(buf, &line, opts);
}

fn write_line(buf: &mut String, line: &str, opts: &EmitOptions) {
    if opts.fold_lines {
        fold_line(buf, line, opts);
    } else {
        buf.push_str(line);
        write_eol(buf, opts);
    }
}

fn fold_line(buf: &mut String, line: &str, opts: &EmitOptions) {
    const LIMIT: usize = 75;
    let bytes = line.as_bytes();
    if bytes.len() <= LIMIT {
        buf.push_str(line);
        write_eol(buf, opts);
        return;
    }
    let mut start = 0usize;
    while start < bytes.len() {
        let end = if start == 0 {
            LIMIT.min(bytes.len())
        } else {
            (start + LIMIT - 1).min(bytes.len())
        };
        let chunk = &line[start..end];
        if start > 0 {
            buf.push(' ');
        }
        buf.push_str(chunk);
        write_eol(buf, opts);
        start = end;
    }
}

fn write_eol(buf: &mut String, opts: &EmitOptions) {
    if opts.crlf {
        buf.push_str("\r\n");
    } else {
        buf.push('\n');
    }
}
