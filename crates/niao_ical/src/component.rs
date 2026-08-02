use crate::property::Property;
use std::collections::HashMap;

/// A VCALENDAR / VEVENT / VCARD / … component tree node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Component {
    pub name: String,
    pub properties: Vec<Property>,
    pub children: Vec<Component>,
}

impl Component {
    /// >>> use niao_ical::Component;
    /// >>> let c = Component::new("VCALENDAR");
    /// >>> c.name == "VCALENDAR"
    /// true
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into().to_ascii_uppercase(),
            properties: Vec::new(),
            children: Vec::new(),
        }
    }

    /// >>> use niao_ical::{Component, Property};
    /// >>> let c = Component::new("VEVENT").with_property(Property::new("SUMMARY", "Meet"));
    /// >>> c.get("SUMMARY").map(|p| p.value.as_str()) == Some("Meet")
    /// true
    pub fn with_property(mut self, prop: Property) -> Self {
        self.properties.push(prop);
        self
    }

    /// >>> use niao_ical::Component;
    /// >>> let c = Component::new("VCALENDAR").with_child(Component::new("VEVENT"));
    /// >>> c.children.len() == 1 && c.children[0].name == "VEVENT"
    /// true
    pub fn with_child(mut self, child: Component) -> Self {
        self.children.push(child);
        self
    }

    /// First property with the given name (case-insensitive).
    pub fn get(&self, name: &str) -> Option<&Property> {
        let key = name.to_ascii_uppercase();
        self.properties.iter().find(|p| p.name == key)
    }

    /// All properties with the given name.
    pub fn get_all(&self, name: &str) -> Vec<&Property> {
        let key = name.to_ascii_uppercase();
        self.properties.iter().filter(|p| p.name == key).collect()
    }

    /// Direct child components of `name`.
    pub fn children_named(&self, name: &str) -> Vec<&Component> {
        let key = name.to_ascii_uppercase();
        self.children.iter().filter(|c| c.name == key).collect()
    }

    /// Depth-first walk over self and descendants.
    pub fn walk(&self) -> Vec<&Component> {
        let mut out = vec![self];
        for child in &self.children {
            out.extend(child.walk());
        }
        out
    }

    /// Properties as a map (last duplicate wins).
    pub fn props_map(&self) -> HashMap<String, String> {
        let mut m = HashMap::new();
        for p in &self.properties {
            m.insert(p.name.clone(), p.value.clone());
        }
        m
    }
}
