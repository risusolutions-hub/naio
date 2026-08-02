//! Pretty / compact GraphQL printers.

use crate::ast::*;

/// Print an executable document with newlines/indent.
pub fn print_document(doc: &Document) -> String {
    let mut out = String::new();
    for (i, def) in doc.definitions.iter().enumerate() {
        if i > 0 {
            out.push_str("\n\n");
        }
        match def {
            Definition::Operation(op) => print_operation(&mut out, op, 0),
            Definition::Fragment(frag) => print_fragment(&mut out, frag, 0),
        }
    }
    out
}

/// Compact single-line-ish print (minify).
pub fn minify_document(doc: &Document) -> String {
    print_document(doc)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace(" {", "{")
        .replace("{ ", "{")
        .replace(" }", "}")
        .replace("( ", "(")
        .replace(" )", ")")
        .replace(" :", ":")
        .replace(": ", ":")
        .replace(" ,", ",")
}

pub fn print_schema_document(doc: &SchemaDocument) -> String {
    let mut out = String::new();
    for (i, def) in doc.definitions.iter().enumerate() {
        if i > 0 {
            out.push_str("\n\n");
        }
        print_type_system(&mut out, def);
    }
    out
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn print_operation(out: &mut String, op: &OperationDefinition, depth: usize) {
    let anonymous = op.name.is_none()
        && op.variables.is_empty()
        && op.directives.is_empty()
        && op.operation == OperationType::Query;
    if !anonymous {
        out.push_str(op.operation.as_str());
        if let Some(name) = &op.name {
            out.push(' ');
            out.push_str(name);
        }
        if !op.variables.is_empty() {
            out.push('(');
            for (i, v) in op.variables.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push('$');
                out.push_str(&v.name);
                out.push_str(": ");
                print_type(out, &v.ty);
                if let Some(def) = &v.default_value {
                    out.push_str(" = ");
                    print_value(out, def);
                }
            }
            out.push(')');
        }
        print_directives(out, &op.directives);
        out.push(' ');
    }
    print_selection_set(out, &op.selection_set, depth);
}

fn print_fragment(out: &mut String, frag: &FragmentDefinition, depth: usize) {
    out.push_str("fragment ");
    out.push_str(&frag.name);
    out.push_str(" on ");
    out.push_str(&frag.type_condition);
    print_directives(out, &frag.directives);
    out.push(' ');
    print_selection_set(out, &frag.selection_set, depth);
}

fn print_selection_set(out: &mut String, set: &SelectionSet, depth: usize) {
    out.push('{');
    out.push('\n');
    for sel in &set.selections {
        indent(out, depth + 1);
        print_selection(out, sel, depth + 1);
        out.push('\n');
    }
    indent(out, depth);
    out.push('}');
}

fn print_selection(out: &mut String, sel: &Selection, depth: usize) {
    match sel {
        Selection::Field(f) => {
            if let Some(alias) = &f.alias {
                out.push_str(alias);
                out.push_str(": ");
            }
            out.push_str(&f.name);
            if !f.arguments.is_empty() {
                out.push('(');
                for (i, a) in f.arguments.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&a.name);
                    out.push_str(": ");
                    print_value(out, &a.value);
                }
                out.push(')');
            }
            print_directives(out, &f.directives);
            if let Some(ss) = &f.selection_set {
                out.push(' ');
                print_selection_set(out, ss, depth);
            }
        }
        Selection::FragmentSpread(s) => {
            out.push_str("...");
            out.push_str(&s.name);
            print_directives(out, &s.directives);
        }
        Selection::InlineFragment(i) => {
            out.push_str("...");
            if let Some(tc) = &i.type_condition {
                out.push_str(" on ");
                out.push_str(tc);
            }
            print_directives(out, &i.directives);
            out.push(' ');
            print_selection_set(out, &i.selection_set, depth);
        }
    }
}

fn print_directives(out: &mut String, dirs: &[Directive]) {
    for d in dirs {
        out.push_str(" @");
        out.push_str(&d.name);
        if !d.arguments.is_empty() {
            out.push('(');
            for (i, a) in d.arguments.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&a.name);
                out.push_str(": ");
                print_value(out, &a.value);
            }
            out.push(')');
        }
    }
}

fn print_type(out: &mut String, ty: &TypeNode) {
    match ty {
        TypeNode::Named(n) => out.push_str(n),
        TypeNode::List(inner) => {
            out.push('[');
            print_type(out, inner);
            out.push(']');
        }
        TypeNode::NonNull(inner) => {
            print_type(out, inner);
            out.push('!');
        }
    }
}

fn print_value(out: &mut String, v: &ValueNode) {
    match v {
        ValueNode::Variable(n) => {
            out.push('$');
            out.push_str(n);
        }
        ValueNode::Int(n) => out.push_str(&n.to_string()),
        ValueNode::Float(f) => {
            let s = format!("{f}");
            if !s.contains('.') && !s.contains('e') && !s.contains('E') {
                out.push_str(&format!("{f:.1}"));
            } else {
                out.push_str(&s);
            }
        }
        ValueNode::String(s) => {
            out.push('"');
            for ch in s.chars() {
                match ch {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    c => out.push(c),
                }
            }
            out.push('"');
        }
        ValueNode::Boolean(b) => out.push_str(if *b { "true" } else { "false" }),
        ValueNode::Null => out.push_str("null"),
        ValueNode::Enum(e) => out.push_str(e),
        ValueNode::List(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                print_value(out, item);
            }
            out.push(']');
        }
        ValueNode::Object(fields) => {
            out.push('{');
            for (i, (k, v)) in fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(k);
                out.push_str(": ");
                print_value(out, v);
            }
            out.push('}');
        }
    }
}

fn print_type_system(out: &mut String, def: &TypeSystemDefinition) {
    match def {
        TypeSystemDefinition::Schema(s) => {
            if let Some(d) = &s.description {
                print_description(out, d);
            }
            out.push_str("schema");
            print_directives(out, &s.directives);
            out.push_str(" {\n");
            if let Some(q) = &s.query {
                out.push_str("  query: ");
                out.push_str(q);
                out.push('\n');
            }
            if let Some(m) = &s.mutation {
                out.push_str("  mutation: ");
                out.push_str(m);
                out.push('\n');
            }
            if let Some(sub) = &s.subscription {
                out.push_str("  subscription: ");
                out.push_str(sub);
                out.push('\n');
            }
            out.push('}');
        }
        TypeSystemDefinition::Scalar(s) => {
            if let Some(d) = &s.description {
                print_description(out, d);
            }
            out.push_str("scalar ");
            out.push_str(&s.name);
            print_directives(out, &s.directives);
        }
        TypeSystemDefinition::Object(o) => {
            if let Some(d) = &o.description {
                print_description(out, d);
            }
            out.push_str("type ");
            out.push_str(&o.name);
            if !o.implements.is_empty() {
                out.push_str(" implements ");
                out.push_str(&o.implements.join(" & "));
            }
            print_directives(out, &o.directives);
            if !o.fields.is_empty() {
                out.push_str(" {\n");
                for f in &o.fields {
                    out.push_str("  ");
                    out.push_str(&f.name);
                    if !f.arguments.is_empty() {
                        out.push('(');
                        for (i, a) in f.arguments.iter().enumerate() {
                            if i > 0 {
                                out.push_str(", ");
                            }
                            out.push_str(&a.name);
                            out.push_str(": ");
                            print_type(out, &a.ty);
                        }
                        out.push(')');
                    }
                    out.push_str(": ");
                    print_type(out, &f.ty);
                    out.push('\n');
                }
                out.push('}');
            }
        }
        TypeSystemDefinition::Interface(i) => {
            if let Some(d) = &i.description {
                print_description(out, d);
            }
            out.push_str("interface ");
            out.push_str(&i.name);
            print_directives(out, &i.directives);
            if !i.fields.is_empty() {
                out.push_str(" {\n");
                for f in &i.fields {
                    out.push_str("  ");
                    out.push_str(&f.name);
                    out.push_str(": ");
                    print_type(out, &f.ty);
                    out.push('\n');
                }
                out.push('}');
            }
        }
        TypeSystemDefinition::Union(u) => {
            if let Some(d) = &u.description {
                print_description(out, d);
            }
            out.push_str("union ");
            out.push_str(&u.name);
            print_directives(out, &u.directives);
            if !u.members.is_empty() {
                out.push_str(" = ");
                out.push_str(&u.members.join(" | "));
            }
        }
        TypeSystemDefinition::Enum(e) => {
            if let Some(d) = &e.description {
                print_description(out, d);
            }
            out.push_str("enum ");
            out.push_str(&e.name);
            print_directives(out, &e.directives);
            if !e.values.is_empty() {
                out.push_str(" {\n");
                for v in &e.values {
                    out.push_str("  ");
                    out.push_str(&v.name);
                    out.push('\n');
                }
                out.push('}');
            }
        }
        TypeSystemDefinition::InputObject(i) => {
            if let Some(d) = &i.description {
                print_description(out, d);
            }
            out.push_str("input ");
            out.push_str(&i.name);
            print_directives(out, &i.directives);
            if !i.fields.is_empty() {
                out.push_str(" {\n");
                for f in &i.fields {
                    out.push_str("  ");
                    out.push_str(&f.name);
                    out.push_str(": ");
                    print_type(out, &f.ty);
                    out.push('\n');
                }
                out.push('}');
            }
        }
    }
}

fn print_description(out: &mut String, desc: &str) {
    if desc.contains('\n') {
        out.push_str("\"\"\"");
        out.push('\n');
        out.push_str(desc);
        out.push('\n');
        out.push_str("\"\"\"\n");
    } else {
        out.push('"');
        out.push_str(desc);
        out.push_str("\"\n");
    }
}
