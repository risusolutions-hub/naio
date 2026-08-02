//! XPath subset evaluator (~xml.etree.ElementTree XPath).

use crate::dom::Element;
use crate::error::XmlError;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
    Child(TagTest),
    Descendant(TagTest),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TagTest {
    Name(String),
    Any,
    Ns { uri: String, local: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Predicate {
    AttrExists(String),
    AttrEquals(String, String),
    AttrNotEquals(String, String),
    Index(i64),
    Last,
    TextEquals(String),
    HasChild(TagTest),
}

#[derive(Debug, Clone)]
struct Path {
    absolute: bool,
    steps: Vec<(Step, Vec<Predicate>)>,
}

fn parse_tag(token: &str) -> Result<TagTest, XmlError> {
    if token == "*" {
        return Ok(TagTest::Any);
    }
    if let Some(rest) = token.strip_prefix('{') {
        let (uri, after) = rest
            .split_once('}')
            .ok_or_else(|| XmlError::XPath(format!("bad namespace tag: {token}")))?;
        if after.is_empty() {
            return Ok(TagTest::Ns {
                uri: uri.to_string(),
                local: None,
            });
        }
        return Ok(TagTest::Ns {
            uri: uri.to_string(),
            local: Some(after.to_string()),
        });
    }
    Ok(TagTest::Name(token.to_string()))
}

fn parse_predicate(inner: &str) -> Result<Predicate, XmlError> {
    let inner = inner.trim();
    if inner == "last()" {
        return Ok(Predicate::Last);
    }
    if let Some(rest) = inner.strip_prefix('@') {
        if let Some((k, v)) = rest.split_once('=') {
            let key = k.trim();
            let val = v.trim().trim_matches(|c| c == '"' || c == '\'');
            if rest.contains("!=") {
                let key = rest
                    .split("!=")
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_start_matches('@');
                let val = rest
                    .split("!=")
                    .nth(1)
                    .unwrap_or("")
                    .trim()
                    .trim_matches(|c| c == '"' || c == '\'');
                return Ok(Predicate::AttrNotEquals(key.to_string(), val.to_string()));
            }
            return Ok(Predicate::AttrEquals(key.to_string(), val.to_string()));
        }
        return Ok(Predicate::AttrExists(rest.to_string()));
    }
    if let Some(pos) = inner.strip_prefix("position()=") {
        let n: i64 = pos
            .parse()
            .map_err(|_| XmlError::XPath(format!("bad position predicate: {inner}")))?;
        return Ok(Predicate::Index(n));
    }
    if let Ok(n) = inner.parse::<i64>() {
        return Ok(Predicate::Index(n));
    }
    if inner.contains('=') && !inner.starts_with('@') {
        let val = inner
            .split_once('=')
            .map(|(_, v)| v.trim().trim_matches(|c| c == '"' || c == '\''))
            .unwrap_or("");
        return Ok(Predicate::TextEquals(val.to_string()));
    }
    Ok(Predicate::HasChild(parse_tag(inner)?))
}

fn split_steps(path: &str) -> Result<Path, XmlError> {
    let mut s = path.trim();
    let absolute = s.starts_with('/');
    if absolute {
        s = s.trim_start_matches('/');
    }
    if s.is_empty() && absolute {
        return Ok(Path {
            absolute: true,
            steps: vec![],
        });
    }

    let mut steps = Vec::new();
    let mut rest = s;
    while !rest.is_empty() {
        let desc = rest.starts_with("//") || rest.starts_with(".//");
        if rest.starts_with(".//") {
            rest = &rest[3..];
        } else if rest.starts_with("//") {
            rest = &rest[2..];
        }
        let _step_kind = if desc { Step::Descendant } else { Step::Child };

        let end = rest.find(['/', '[']).unwrap_or(rest.len());
        let tag_token = &rest[..end];
        if tag_token.is_empty() && desc {
            return Err(XmlError::XPath("empty step in path".into()));
        }
        let tag = if tag_token.is_empty() {
            TagTest::Any
        } else {
            parse_tag(tag_token)?
        };
        rest = &rest[end..];

        let mut preds = Vec::new();
        while rest.starts_with('[') {
            let close = rest
                .find(']')
                .ok_or_else(|| XmlError::XPath("unclosed predicate".into()))?;
            let pred = parse_predicate(&rest[1..close])?;
            preds.push(pred);
            rest = &rest[close + 1..];
        }

        steps.push((
            if desc {
                Step::Descendant(tag)
            } else {
                Step::Child(tag)
            },
            preds,
        ));

        if rest.starts_with('/') && !rest.starts_with("//") {
            rest = &rest[1..];
        }
    }

    Ok(Path { absolute, steps })
}

fn tag_matches(test: &TagTest, el: &Element) -> bool {
    match test {
        TagTest::Any => true,
        TagTest::Name(n) => &el.tag == n || el.qname() == *n,
        TagTest::Ns { uri, local } => {
            el.namespace.as_deref() == Some(uri.as_str())
                && local.as_ref().map(|l| &el.tag == l).unwrap_or(true)
        }
    }
}

fn matches_predicates(el: &Element, preds: &[Predicate], siblings: &[&Element]) -> bool {
    for p in preds {
        match p {
            Predicate::AttrExists(k) => {
                if el.get_attr(k).is_none() {
                    return false;
                }
            }
            Predicate::AttrEquals(k, v) => {
                if el.get_attr(k) != Some(v.as_str()) {
                    return false;
                }
            }
            Predicate::AttrNotEquals(k, v) => {
                if el.get_attr(k) == Some(v.as_str()) {
                    return false;
                }
            }
            Predicate::Index(n) => {
                let idx = if *n < 0 {
                    siblings.len() as i64 + *n
                } else {
                    *n - 1
                };
                if idx < 0 || idx as usize >= siblings.len() {
                    return false;
                }
                if !std::ptr::eq(siblings[idx as usize], el) {
                    return false;
                }
            }
            Predicate::Last => {
                if siblings.is_empty() || !std::ptr::eq(siblings[siblings.len() - 1], el) {
                    return false;
                }
            }
            Predicate::TextEquals(t) => {
                if el.text != *t {
                    return false;
                }
            }
            Predicate::HasChild(tt) => {
                if !el.child_elements().iter().any(|c| tag_matches(tt, c)) {
                    return false;
                }
            }
        }
    }
    true
}

fn child_elements_of<'a>(el: &'a Element) -> Vec<&'a Element> {
    el.child_elements()
}

fn eval_step<'a>(current: &[&'a Element], step: &Step, preds: &[Predicate]) -> Vec<&'a Element> {
    let mut out = Vec::new();
    match step {
        Step::Child(tag) => {
            for el in current {
                let sibs: Vec<&Element> = child_elements_of(el);
                for child in &sibs {
                    if tag_matches(tag, child) && matches_predicates(child, preds, &sibs) {
                        out.push(*child);
                    }
                }
            }
        }
        Step::Descendant(tag) => {
            for el in current {
                collect_desc(el, tag, preds, &mut out);
            }
        }
    }
    out
}

fn collect_desc<'a>(
    el: &'a Element,
    tag: &TagTest,
    preds: &[Predicate],
    out: &mut Vec<&'a Element>,
) {
    let sibs = child_elements_of(el);
    for child in &sibs {
        if tag_matches(tag, child) && matches_predicates(child, preds, &sibs) {
            out.push(child);
        }
        collect_desc(child, tag, preds, out);
    }
}

/// Find first element matching XPath subset from `elem`.
pub fn find<'a>(elem: &'a Element, path: &str) -> Result<Option<&'a Element>, XmlError> {
    Ok(findall(elem, path)?.into_iter().next())
}

/// Find all elements matching XPath subset from `elem`.
pub fn findall<'a>(elem: &'a Element, path: &str) -> Result<Vec<&'a Element>, XmlError> {
    let p = split_steps(path)?;
    let mut current = vec![elem];
    if p.absolute {
        // climb to root not stored — treat absolute from current as from elem
        current = vec![elem];
    }
    for (step, preds) in &p.steps {
        current = eval_step(&current, step, preds);
        if current.is_empty() {
            break;
        }
    }
    Ok(current)
}

/// Return text of first match or default.
pub fn findtext<'a>(
    elem: &'a Element,
    path: &str,
    default: Option<&str>,
) -> Result<Option<String>, XmlError> {
    match find(elem, path)? {
        Some(e) => Ok(Some(e.text.clone())),
        None => Ok(default.map(str::to_string)),
    }
}

/// Iterate all elements in document order (optional tag filter).
pub fn iter_elements<'a>(elem: &'a Element, tag: Option<&str>) -> Vec<&'a Element> {
    let mut out = Vec::new();
    walk(elem, tag, &mut out);
    out
}

fn walk<'a>(elem: &'a Element, tag: Option<&str>, out: &mut Vec<&'a Element>) {
    if tag
        .map(|t| elem.tag == t || elem.qname() == t)
        .unwrap_or(true)
    {
        out.push(elem);
    }
    for child in elem.child_elements() {
        walk(child, tag, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::Element;
    use crate::parse::parse;
    use crate::XmlOpts;

    fn sample() -> Element {
        parse(
            r#"<root><item id="1">a</item><item id="2">b</item></root>"#,
            &XmlOpts::default(),
        )
        .unwrap()
        .root
        .unwrap()
    }

    #[test]
    fn find_child() {
        let root = sample();
        let hits = findall(&root, "item").unwrap();
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn find_attr() {
        let root = sample();
        let hit = find(&root, "item[@id='2']").unwrap().unwrap();
        assert_eq!(hit.text, "b");
    }
}
