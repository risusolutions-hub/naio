//! DOM tree types (~xml.etree.ElementTree).

use crate::error::{XmlError, MAX_NODES};
use std::collections::HashMap;

/// Parse/emit options.
#[derive(Debug, Clone)]
pub struct XmlOpts {
    pub keep_comments: bool,
    pub keep_pi: bool,
    pub recover: bool,
    pub huge_tree: bool,
    pub xml_declaration: bool,
    pub encoding: Option<String>,
    pub indent: Option<String>,
    pub pretty: bool,
}

impl Default for XmlOpts {
    fn default() -> Self {
        Self {
            keep_comments: true,
            keep_pi: true,
            recover: false,
            huge_tree: false,
            xml_declaration: true,
            encoding: None,
            indent: None,
            pretty: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attr {
    pub local: String,
    pub prefix: Option<String>,
    pub namespace: Option<String>,
    pub value: String,
}

impl Attr {
    pub fn key(&self) -> String {
        match (&self.namespace, &self.prefix) {
            (Some(ns), Some(p)) => format!("{{{ns}}}{p}:{}", self.local),
            (Some(ns), None) => format!("{{{ns}}}{}", self.local),
            (None, Some(p)) => format!("{p}:{}", self.local),
            (None, None) => self.local.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Node {
    Element(Element),
    Text(String),
    Comment(String),
    Pi { target: String, data: String },
}

#[derive(Debug, Clone)]
pub struct Element {
    pub tag: String,
    pub prefix: Option<String>,
    pub namespace: Option<String>,
    pub attrs: Vec<Attr>,
    pub text: String,
    pub tail: String,
    pub children: Vec<Node>,
}

impl Element {
    pub fn new(tag: impl Into<String>) -> Self {
        Self {
            tag: tag.into(),
            prefix: None,
            namespace: None,
            attrs: Vec::new(),
            text: String::new(),
            tail: String::new(),
            children: Vec::new(),
        }
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }

    pub fn attr_map(&self) -> HashMap<String, String> {
        let mut m = HashMap::with_capacity(self.attrs.len());
        for a in &self.attrs {
            m.insert(a.key(), a.value.clone());
        }
        m
    }

    pub fn get_attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|a| a.key() == key || a.local == key)
            .map(|a| a.value.as_str())
    }

    pub fn set_attr(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        if let Some(a) = self
            .attrs
            .iter_mut()
            .find(|a| a.key() == key || a.local == key)
        {
            a.value = value.into();
            return;
        }
        self.attrs.push(Attr {
            local: key,
            prefix: None,
            namespace: None,
            value: value.into(),
        });
    }

    pub fn remove_attr(&mut self, key: &str) -> bool {
        if let Some(i) = self
            .attrs
            .iter()
            .position(|a| a.key() == key || a.local == key)
        {
            self.attrs.remove(i);
            true
        } else {
            false
        }
    }

    pub fn qname(&self) -> String {
        match (&self.prefix, &self.namespace) {
            (Some(p), Some(ns)) => format!("{{{ns}}}{p}:{}", self.tag),
            (Some(p), None) => format!("{p}:{}", self.tag),
            (None, Some(ns)) => format!("{{{ns}}}{}", self.tag),
            (None, None) => self.tag.clone(),
        }
    }

    pub fn append(&mut self, node: Node) -> Result<(), XmlError> {
        self.children.push(node);
        Ok(())
    }

    pub fn append_element(&mut self, child: Element) -> Result<(), XmlError> {
        self.children.push(Node::Element(child));
        Ok(())
    }

    pub fn sub_element(
        &mut self,
        tag: impl Into<String>,
        attrs: &HashMap<String, String>,
        text: Option<&str>,
    ) -> Result<&mut Element, XmlError> {
        let mut el = Element::new(tag);
        for (k, v) in attrs {
            el.set_attr(k.clone(), v.clone());
        }
        if let Some(t) = text {
            el.text = t.to_string();
        }
        self.children.push(Node::Element(el));
        match self.children.last_mut() {
            Some(Node::Element(e)) => Ok(e),
            _ => unreachable!(),
        }
    }

    pub fn clear(&mut self) {
        self.attrs.clear();
        self.text.clear();
        self.tail.clear();
        self.children.clear();
    }

    pub fn child_elements(&self) -> Vec<&Element> {
        self.children
            .iter()
            .filter_map(|n| match n {
                Node::Element(e) => Some(e),
                _ => None,
            })
            .collect()
    }

    pub fn child_elements_mut(&mut self) -> Vec<&mut Element> {
        self.children
            .iter_mut()
            .filter_map(|n| match n {
                Node::Element(e) => Some(e),
                _ => None,
            })
            .collect()
    }

    pub fn count_nodes(&self) -> usize {
        let mut n = 1usize;
        for c in &self.children {
            n += count_node(c);
        }
        n
    }
}

fn count_node(node: &Node) -> usize {
    match node {
        Node::Element(e) => e.count_nodes(),
        _ => 1,
    }
}

#[derive(Debug, Clone)]
pub struct Document {
    pub version: Option<String>,
    pub encoding: Option<String>,
    pub root: Option<Element>,
}

impl Document {
    pub fn new(root: Element) -> Self {
        Self {
            version: Some("1.0".into()),
            encoding: None,
            root: Some(root),
        }
    }

    pub fn empty() -> Self {
        Self {
            version: Some("1.0".into()),
            encoding: None,
            root: None,
        }
    }

    pub fn node_count(&self) -> usize {
        self.root.as_ref().map(|r| r.count_nodes()).unwrap_or(0)
    }

    pub fn check_limits(&self, huge_tree: bool) -> Result<(), XmlError> {
        if huge_tree {
            return Ok(());
        }
        if self.node_count() > MAX_NODES {
            return Err(XmlError::TooManyNodes);
        }
        Ok(())
    }
}

/// Path to a node: indices into each Element's `children` vector (element children only).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodePath(pub Vec<usize>);

impl NodePath {
    pub fn root() -> Self {
        Self(Vec::new())
    }
}

pub fn resolve_element<'a>(doc: &'a Document, path: &NodePath) -> Result<&'a Element, XmlError> {
    let root = doc
        .root
        .as_ref()
        .ok_or_else(|| XmlError::InvalidNode("document has no root element".into()))?;
    if path.0.is_empty() {
        return Ok(root);
    }
    let mut cur = root;
    for &idx in &path.0 {
        let mut el_idx = 0usize;
        let mut found = None;
        for child in &cur.children {
            if let Node::Element(e) = child {
                if el_idx == idx {
                    found = Some(e);
                    break;
                }
                el_idx += 1;
            }
        }
        cur = found.ok_or_else(|| XmlError::InvalidNode("invalid element path".into()))?;
    }
    Ok(cur)
}

pub fn resolve_element_mut<'a>(
    doc: &'a mut Document,
    path: &NodePath,
) -> Result<&'a mut Element, XmlError> {
    fn step<'b>(el: &'b mut Element, idx: usize) -> Result<&'b mut Element, XmlError> {
        let mut el_idx = 0usize;
        for child in &mut el.children {
            if let Node::Element(e) = child {
                if el_idx == idx {
                    return Ok(e);
                }
                el_idx += 1;
            }
        }
        Err(XmlError::InvalidNode("invalid element path".into()))
    }

    let root = doc
        .root
        .as_mut()
        .ok_or_else(|| XmlError::InvalidNode("document has no root element".into()))?;
    if path.0.is_empty() {
        return Ok(root);
    }
    let mut cur = root;
    for &idx in &path.0 {
        cur = step(cur, idx)?;
    }
    Ok(cur)
}

pub fn parent_path(path: &NodePath) -> Option<NodePath> {
    if path.0.is_empty() {
        None
    } else {
        let mut p = path.0.clone();
        p.pop();
        Some(NodePath(p))
    }
}

pub fn deep_copy_element(el: &Element) -> Element {
    el.clone()
}
