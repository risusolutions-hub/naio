//! Fragment utilities.

use crate::ast::*;
use crate::error::{GqlError, GqlResult};
use std::collections::HashMap;

/// Collect fragment definitions from a document.
pub fn list_fragments(doc: &Document) -> Vec<&FragmentDefinition> {
    doc.definitions
        .iter()
        .filter_map(|d| match d {
            Definition::Fragment(f) => Some(f),
            _ => None,
        })
        .collect()
}

/// Collect operations from a document.
pub fn list_operations(doc: &Document) -> Vec<&OperationDefinition> {
    doc.definitions
        .iter()
        .filter_map(|d| match d {
            Definition::Operation(o) => Some(o),
            _ => None,
        })
        .collect()
}

/// Variable names declared on an operation (or all ops if name is None and single).
pub fn variable_names(doc: &Document, operation: Option<&str>) -> GqlResult<Vec<String>> {
    let ops = list_operations(doc);
    let op = if let Some(name) = operation {
        ops.iter()
            .find(|o| o.name.as_deref() == Some(name))
            .copied()
            .ok_or_else(|| GqlError::new(format!("unknown operation '{name}'")))?
    } else if ops.len() == 1 {
        ops[0]
    } else if ops.is_empty() {
        return Ok(Vec::new());
    } else {
        return Err(GqlError::new(
            "operation name required when document has multiple operations",
        ));
    };
    Ok(op.variables.iter().map(|v| v.name.clone()).collect())
}

/// Inline all fragment spreads into operations; returns a new document without fragment defs.
pub fn spread_fragments(doc: &Document) -> GqlResult<Document> {
    let fragments: HashMap<&str, &FragmentDefinition> = list_fragments(doc)
        .into_iter()
        .map(|f| (f.name.as_str(), f))
        .collect();

    let mut definitions = Vec::new();
    for def in &doc.definitions {
        if let Definition::Operation(op) = def {
            let mut new_op = op.clone();
            new_op.selection_set =
                inline_selection_set(&op.selection_set, &fragments, &mut Vec::new())?;
            definitions.push(Definition::Operation(new_op));
        }
    }
    if definitions.is_empty() {
        return Err(GqlError::new(
            "document has no operations to spread fragments into",
        ));
    }
    Ok(Document {
        definitions,
        source: String::new(),
    })
}

fn inline_selection_set(
    set: &SelectionSet,
    fragments: &HashMap<&str, &FragmentDefinition>,
    stack: &mut Vec<String>,
) -> GqlResult<SelectionSet> {
    let mut selections = Vec::new();
    for sel in &set.selections {
        match sel {
            Selection::Field(f) => {
                let mut nf = f.clone();
                if let Some(ss) = &f.selection_set {
                    nf.selection_set = Some(inline_selection_set(ss, fragments, stack)?);
                }
                selections.push(Selection::Field(nf));
            }
            Selection::FragmentSpread(s) => {
                if stack.contains(&s.name) {
                    return Err(GqlError::new(format!(
                        "fragment cycle involving '{}'",
                        s.name
                    )));
                }
                let frag = fragments
                    .get(s.name.as_str())
                    .ok_or_else(|| GqlError::new(format!("unknown fragment '{}'", s.name)))?;
                stack.push(s.name.clone());
                let inlined = inline_selection_set(&frag.selection_set, fragments, stack)?;
                stack.pop();
                // Preserve type condition via inline fragment
                selections.push(Selection::InlineFragment(InlineFragment {
                    type_condition: Some(frag.type_condition.clone()),
                    directives: s.directives.clone(),
                    selection_set: inlined,
                }));
            }
            Selection::InlineFragment(i) => {
                let mut ni = i.clone();
                ni.selection_set = inline_selection_set(&i.selection_set, fragments, stack)?;
                selections.push(Selection::InlineFragment(ni));
            }
        }
    }
    Ok(SelectionSet { selections })
}
