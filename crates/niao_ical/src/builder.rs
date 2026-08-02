use crate::component::Component;
use crate::property::Property;

/// Fluent builders for common calendar / contact shapes.

/// >>> use niao_ical::builder::calendar;
/// >>> let cal = calendar().event(|e| e.summary("Hi").uid("1")).build();
/// >>> cal.name == "VCALENDAR" && cal.children.len() == 1
/// true
pub fn calendar() -> CalendarBuilder {
    CalendarBuilder::default()
}

/// >>> use niao_ical::builder::contact;
/// >>> let c = contact().full_name("Ada Lovelace").email("ada@example.com").build();
/// >>> c.get("FN").map(|p| p.value.as_str()) == Some("Ada Lovelace")
/// true
pub fn contact() -> ContactBuilder {
    ContactBuilder::default()
}

#[derive(Debug, Default)]
pub struct CalendarBuilder {
    prodid: String,
    method: Option<String>,
    events: Vec<Component>,
    todos: Vec<Component>,
    timezones: Vec<Component>,
}

impl CalendarBuilder {
    pub fn prodid(mut self, v: impl Into<String>) -> Self {
        self.prodid = v.into();
        self
    }

    pub fn method(mut self, v: impl Into<String>) -> Self {
        self.method = Some(v.into());
        self
    }

    pub fn event(mut self, f: impl FnOnce(EventBuilder) -> EventBuilder) -> Self {
        self.events.push(f(EventBuilder::default()).build());
        self
    }

    pub fn todo(mut self, f: impl FnOnce(TodoBuilder) -> TodoBuilder) -> Self {
        self.todos.push(f(TodoBuilder::default()).build());
        self
    }

    pub fn build(self) -> Component {
        let mut cal = Component::new("VCALENDAR")
            .with_property(Property::new("VERSION", "2.0"))
            .with_property(Property::new(
                "PRODID",
                if self.prodid.is_empty() {
                    "-//Niao//nical//EN".into()
                } else {
                    self.prodid
                },
            ));
        if let Some(m) = self.method {
            cal = cal.with_property(Property::new("METHOD", m));
        }
        for tz in self.timezones {
            cal = cal.with_child(tz);
        }
        for ev in self.events {
            cal = cal.with_child(ev);
        }
        for td in self.todos {
            cal = cal.with_child(td);
        }
        cal
    }
}

#[derive(Debug, Default)]
pub struct EventBuilder {
    pub props: Vec<Property>,
    pub alarms: Vec<Component>,
}

impl EventBuilder {
    pub fn summary(mut self, v: impl Into<String>) -> Self {
        self.props.push(Property::new("SUMMARY", v));
        self
    }

    pub fn uid(mut self, v: impl Into<String>) -> Self {
        self.props.push(Property::new("UID", v));
        self
    }

    pub fn dtstart(mut self, v: impl Into<String>) -> Self {
        self.props
            .push(Property::new("DTSTART", v).with_param("VALUE", "DATE-TIME"));
        self
    }

    pub fn dtend(mut self, v: impl Into<String>) -> Self {
        self.props
            .push(Property::new("DTEND", v).with_param("VALUE", "DATE-TIME"));
        self
    }

    pub fn location(mut self, v: impl Into<String>) -> Self {
        self.props.push(Property::new("LOCATION", v));
        self
    }

    pub fn description(mut self, v: impl Into<String>) -> Self {
        self.props.push(Property::new("DESCRIPTION", v));
        self
    }

    pub fn rrule(mut self, v: impl Into<String>) -> Self {
        self.props.push(Property::new("RRULE", v));
        self
    }

    pub fn property(mut self, p: Property) -> Self {
        self.props.push(p);
        self
    }

    pub fn alarm_display(
        mut self,
        trigger: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        let alarm = Component::new("VALARM")
            .with_property(Property::new("ACTION", "DISPLAY"))
            .with_property(Property::new("TRIGGER", trigger))
            .with_property(Property::new("DESCRIPTION", description));
        self.alarms.push(alarm);
        self
    }

    pub fn build(self) -> Component {
        let mut ev = Component::new("VEVENT");
        for p in self.props {
            ev = ev.with_property(p);
        }
        for a in self.alarms {
            ev = ev.with_child(a);
        }
        ev
    }
}

#[derive(Debug, Default)]
pub struct TodoBuilder {
    props: Vec<Property>,
}

impl TodoBuilder {
    pub fn summary(mut self, v: impl Into<String>) -> Self {
        self.props.push(Property::new("SUMMARY", v));
        self
    }

    pub fn due(mut self, v: impl Into<String>) -> Self {
        self.props.push(Property::new("DUE", v));
        self
    }

    pub fn status(mut self, v: impl Into<String>) -> Self {
        self.props.push(Property::new("STATUS", v));
        self
    }

    pub fn build(self) -> Component {
        let mut td = Component::new("VTODO");
        for p in self.props {
            td = td.with_property(p);
        }
        td
    }
}

#[derive(Debug, Default)]
pub struct ContactBuilder {
    props: Vec<Property>,
}

impl ContactBuilder {
    pub fn full_name(mut self, v: impl Into<String>) -> Self {
        self.props.push(Property::new("FN", v));
        self
    }

    pub fn structured_name(mut self, family: impl Into<String>, given: impl Into<String>) -> Self {
        let f: String = family.into();
        let g: String = given.into();
        self.props.push(Property::new("N", format!("{f};{g};;;")));
        self
    }

    pub fn email(mut self, v: impl Into<String>) -> Self {
        self.props
            .push(Property::new("EMAIL", v).with_param("TYPE", "INTERNET"));
        self
    }

    pub fn tel(mut self, v: impl Into<String>) -> Self {
        self.props
            .push(Property::new("TEL", v).with_param("TYPE", "CELL"));
        self
    }

    pub fn org(mut self, v: impl Into<String>) -> Self {
        self.props.push(Property::new("ORG", v));
        self
    }

    pub fn uid(mut self, v: impl Into<String>) -> Self {
        self.props.push(Property::new("UID", v));
        self
    }

    pub fn rev(mut self, v: impl Into<String>) -> Self {
        self.props.push(Property::new("REV", v));
        self
    }

    pub fn property(mut self, p: Property) -> Self {
        self.props.push(p);
        self
    }

    pub fn build(self) -> Component {
        let mut c = Component::new("VCARD").with_property(Property::new("VERSION", "4.0"));
        for p in self.props {
            c = c.with_property(p);
        }
        c
    }
}
