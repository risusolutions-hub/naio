use crate::component::Component;
use crate::error::{IcalError, MAX_BYTES};
use crate::property::{unescape_value, Property};
use crate::unfold::unfold_lines;
use std::collections::HashMap;

/// Parse options for `parse` / `parse_all`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseOptions {
    /// When true, tolerate missing outer BEGIN wrapper (single component).
    pub relaxed: bool,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self { relaxed: false }
    }
}

/// Parse the first top-level component from text.
///
/// >>> use niao_ical::{parse, ParseOptions};
/// >>> let cal = parse("BEGIN:VCALENDAR\nVERSION:2.0\nBEGIN:VEVENT\nSUMMARY:Hi\nEND:VEVENT\nEND:VCALENDAR\n", &ParseOptions::default()).unwrap();
/// >>> cal.name == "VCALENDAR"
/// true
pub fn parse(input: &str, opts: &ParseOptions) -> Result<Component, IcalError> {
    let all = parse_all(input, opts)?;
    all.into_iter().next().ok_or(IcalError::Empty)
}

/// Parse all top-level components (e.g. multiple VCARDs in one file).
pub fn parse_all(input: &str, opts: &ParseOptions) -> Result<Vec<Component>, IcalError> {
    if input.len() > MAX_BYTES {
        return Err(IcalError::TooLarge(input.len()));
    }
    let lines: Vec<(u32, String)> = unfold_lines(input)
        .enumerate()
        .map(|(i, l)| ((i + 1) as u32, l))
        .collect();
    if lines.is_empty() {
        return Err(IcalError::Empty);
    }
    let mut idx = 0usize;
    let mut out = Vec::new();
    while idx < lines.len() {
        let (comp, next) = parse_component(&lines, idx, opts)?;
        out.push(comp);
        idx = next;
    }
    if out.is_empty() {
        return Err(IcalError::Empty);
    }
    Ok(out)
}

fn parse_component(
    lines: &[(u32, String)],
    start: usize,
    opts: &ParseOptions,
) -> Result<(Component, usize), IcalError> {
    let (line_no, first) = &lines[start];
    let name = if let Some(n) = first.strip_prefix("BEGIN:") {
        n.trim().to_ascii_uppercase()
    } else if opts.relaxed {
        // Treat as bare property block inside implicit component — scan ahead.
        return parse_relaxed(lines, start);
    } else {
        return Err(IcalError::InvalidProperty {
            line: *line_no,
            detail: format!("expected BEGIN:, got {first}"),
        });
    };

    let mut comp = Component::new(&name);
    let mut i = start + 1;
    while i < lines.len() {
        let (line_no, line) = &lines[i];
        if line.strip_prefix("BEGIN:").is_some() {
            let (child, next) = parse_component(lines, i, opts)?;
            comp.children.push(child);
            i = next;
            continue;
        }
        if let Some(end_name) = line.strip_prefix("END:") {
            let end = end_name.trim().to_ascii_uppercase();
            if end != comp.name {
                return Err(IcalError::UnbalancedComponent {
                    name: end,
                    line: *line_no,
                });
            }
            return Ok((comp, i + 1));
        }
        comp.properties.push(parse_property_line(line, *line_no)?);
        i += 1;
    }
    Err(IcalError::UnexpectedEnd {
        expected: format!("END:{}", comp.name),
        line: lines.last().map(|l| l.0).unwrap_or(*line_no),
    })
}

fn parse_relaxed(lines: &[(u32, String)], start: usize) -> Result<(Component, usize), IcalError> {
    let mut comp = Component::new("VCARD");
    let mut i = start;
    while i < lines.len() {
        let (line_no, line) = &lines[i];
        if line.starts_with("BEGIN:") {
            break;
        }
        if line.starts_with("END:") {
            return Ok((comp, i + 1));
        }
        comp.properties.push(parse_property_line(line, *line_no)?);
        i += 1;
    }
    Ok((comp, i))
}

fn parse_property_line(line: &str, line_no: u32) -> Result<Property, IcalError> {
    let (left, value_raw) = split_name_value(line).ok_or_else(|| IcalError::InvalidProperty {
        line: line_no,
        detail: "missing ':' separator".into(),
    })?;
    let mut parts = left.split(';');
    let name = parts
        .next()
        .ok_or_else(|| IcalError::InvalidProperty {
            line: line_no,
            detail: "empty property name".into(),
        })?
        .trim()
        .to_ascii_uppercase();
    if name.is_empty() {
        return Err(IcalError::InvalidProperty {
            line: line_no,
            detail: "empty property name".into(),
        });
    }
    let mut params: HashMap<String, Vec<String>> = HashMap::new();
    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((k, v)) = part.split_once('=') {
            params
                .entry(k.trim().to_ascii_uppercase())
                .or_default()
                .push(v.trim().to_string());
        } else {
            params
                .entry(part.to_ascii_uppercase())
                .or_default()
                .push(String::new());
        }
    }
    let value = unescape_value(value_raw.trim());
    Ok(Property {
        name,
        params,
        value,
    })
}

fn split_name_value(line: &str) -> Option<(&str, &str)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b':' {
            return Some((&line[..i], &line[i + 1..]));
        }
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        i += 1;
    }
    None
}

/// Return true when `input` parses as at least one component.
pub fn is_valid(input: &str) -> bool {
    parse(input, &ParseOptions::default()).is_ok()
        || parse_all(input, &ParseOptions::default()).is_ok()
}

/// Parse only VCALENDAR components; errors if first root is not VCALENDAR.
pub fn parse_calendar(input: &str, opts: &ParseOptions) -> Result<Component, IcalError> {
    let cal = parse(input, opts)?;
    if cal.name != "VCALENDAR" {
        return Err(IcalError::InvalidProperty {
            line: 1,
            detail: format!("expected VCALENDAR, got {}", cal.name),
        });
    }
    Ok(cal)
}

/// Parse all VCARD components from a vCard/vcf stream.
pub fn parse_contacts(input: &str, opts: &ParseOptions) -> Result<Vec<Component>, IcalError> {
    let all = parse_all(input, opts)?;
    for c in &all {
        if c.name != "VCARD" {
            return Err(IcalError::InvalidProperty {
                line: 1,
                detail: format!("expected VCARD, got {}", c.name),
            });
        }
    }
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Niao//nical//EN
BEGIN:VEVENT
UID:evt-1
DTSTAMP:20260105T120000Z
DTSTART:20260105T090000Z
DTEND:20260105T093000Z
SUMMARY:Standup
RRULE:FREQ=WEEKLY;BYDAY=MO;COUNT=4
END:VEVENT
END:VCALENDAR
";

    #[test]
    fn parse_event() {
        let cal = parse_calendar(SAMPLE, &ParseOptions::default()).unwrap();
        let ev = cal.children_named("VEVENT");
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].get("SUMMARY").unwrap().value, "Standup");
    }
}
