//! Declarative FSM / statechart specification.

use std::collections::{HashMap, HashSet};

/// How transition sources are matched.
#[derive(Debug, Clone)]
pub enum TransitionSources {
    /// Match only when the active leaf state equals `state`.
    One(u32),
    /// Match when the active leaf is any of these states.
    Many(Vec<u32>),
    /// Match from any state.
    Any,
}

/// Transition destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionDest {
    /// Move to a concrete state (entering composite initial children as needed).
    State(u32),
    /// Stay in the current leaf state (internal transition — no exit/enter).
    Internal,
    /// Remain in source (`dest = source` in transitions parlance).
    Same,
    /// Re-enter the last active child of a composite parent (shallow history).
    HistoryShallow(u32),
}

/// One state in the chart.
#[derive(Debug, Clone)]
pub struct StateDef {
    pub name: String,
    pub parent: Option<u32>,
    /// Initial child when entering this composite state.
    pub initial_child: Option<u32>,
    /// Shallow-history pseudo-state targeting this composite parent.
    pub is_history: bool,
    pub is_final: bool,
}

/// One directed transition edge.
#[derive(Debug, Clone)]
pub struct TransitionDef {
    pub trigger: u32,
    pub sources: TransitionSources,
    pub dest: TransitionDest,
    /// Higher priority wins when multiple transitions match.
    pub priority: i32,
}

/// Immutable machine definition built from declarative input.
#[derive(Debug, Clone)]
pub struct FsmSpec {
    pub states: Vec<StateDef>,
    pub state_index: HashMap<String, u32>,
    pub triggers: Vec<String>,
    pub trigger_index: HashMap<String, u32>,
    pub transitions: Vec<TransitionDef>,
    pub initial: u32,
    pub finals: HashSet<u32>,
    /// Per (leaf_state, trigger) candidate transition indices, priority-sorted.
    pub lookup: HashMap<(u32, u32), Vec<usize>>,
    /// All triggers that may fire from a leaf state (ignoring guards).
    pub triggers_from: HashMap<u32, Vec<u32>>,
}

impl FsmSpec {
    /// Build and validate a specification.
    pub fn build(
        state_defs: Vec<StateDef>,
        trigger_names: Vec<String>,
        transitions: Vec<TransitionDef>,
        initial_name: &str,
    ) -> Result<Self, String> {
        if state_defs.is_empty() {
            return Err("at least one state is required".into());
        }
        if trigger_names.is_empty() {
            return Err("at least one trigger is required".into());
        }

        let mut state_index = HashMap::with_capacity(state_defs.len());
        for (i, s) in state_defs.iter().enumerate() {
            let id = i as u32;
            if state_index.insert(s.name.clone(), id).is_some() {
                return Err(format!("duplicate state name '{}'", s.name));
            }
        }

        let initial = *state_index
            .get(initial_name)
            .ok_or_else(|| format!("unknown initial state '{initial_name}'"))?;

        let mut trigger_index = HashMap::with_capacity(trigger_names.len());
        for (i, t) in trigger_names.iter().enumerate() {
            if trigger_index.insert(t.clone(), i as u32).is_some() {
                return Err(format!("duplicate trigger '{t}'"));
            }
        }

        // Validate parent links and history nodes.
        for (id, s) in state_defs.iter().enumerate() {
            let id = id as u32;
            if let Some(p) = s.parent {
                if p as usize >= state_defs.len() {
                    return Err(format!("state '{}' has invalid parent id {p}", s.name));
                }
                if s.is_history && !state_defs[p as usize].initial_child.is_some() {
                    // history must point at a composite with children — checked below
                }
            }
            if let Some(child) = s.initial_child {
                if child as usize >= state_defs.len() {
                    return Err(format!(
                        "state '{}' has invalid initial_child id {child}",
                        s.name
                    ));
                }
                let child_def = &state_defs[child as usize];
                if child_def.parent != Some(id) {
                    return Err(format!(
                        "initial_child of '{}' must list '{}' as parent",
                        s.name, child_def.name
                    ));
                }
            }
            if s.is_history {
                let parent = s
                    .parent
                    .ok_or_else(|| format!("history state '{}' must have a parent", s.name))?;
                if state_defs[parent as usize].initial_child.is_none() {
                    return Err(format!(
                        "history state '{}' parent '{}' is not composite",
                        s.name, state_defs[parent as usize].name
                    ));
                }
            }
        }

        // Detect cycles in parent chain.
        for (id, s) in state_defs.iter().enumerate() {
            let mut seen = HashSet::new();
            let mut cur = s.parent;
            while let Some(p) = cur {
                if !seen.insert(p) {
                    return Err(format!("cycle in parent chain at state '{}'", s.name));
                }
                if p as usize >= state_defs.len() {
                    break;
                }
                cur = state_defs[p as usize].parent;
            }
            let _ = id;
        }

        let finals: HashSet<u32> = state_defs
            .iter()
            .enumerate()
            .filter(|(_, s)| s.is_final)
            .map(|(i, _)| i as u32)
            .collect();

        // Validate transitions.
        for (ti, tr) in transitions.iter().enumerate() {
            if tr.trigger as usize >= trigger_names.len() {
                return Err(format!("transition {ti} has invalid trigger id"));
            }
            match &tr.sources {
                TransitionSources::One(s) => validate_state_id(*s, &state_defs, ti)?,
                TransitionSources::Many(v) => {
                    if v.is_empty() {
                        return Err(format!("transition {ti} has empty source list"));
                    }
                    for s in v {
                        validate_state_id(*s, &state_defs, ti)?;
                    }
                }
                TransitionSources::Any => {}
            }
            match tr.dest {
                TransitionDest::State(s) => validate_state_id(s, &state_defs, ti)?,
                TransitionDest::Internal | TransitionDest::Same => {}
                TransitionDest::HistoryShallow(p) => {
                    validate_state_id(p, &state_defs, ti)?;
                    if state_defs[p as usize].initial_child.is_none() {
                        return Err(format!(
                            "transition {ti} history target '{}' is not composite",
                            state_defs[p as usize].name
                        ));
                    }
                }
            }
        }

        let mut lookup: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
        let mut triggers_from: HashMap<u32, Vec<u32>> = HashMap::new();

        for (ti, tr) in transitions.iter().enumerate() {
            let leaf_states: Vec<u32> = (0..state_defs.len() as u32)
                .filter(|&sid| {
                    // Only leaf states (no initial_child) are active positions,
                    // except history pseudo-states which are never active leaves.
                    if state_defs[sid as usize].initial_child.is_some()
                        || state_defs[sid as usize].is_history
                    {
                        return false;
                    }
                    matches_source(sid, &tr.sources)
                })
                .collect();

            for sid in leaf_states {
                lookup.entry((sid, tr.trigger)).or_default().push(ti);
                triggers_from.entry(sid).or_default().push(tr.trigger);
            }
        }

        for list in lookup.values_mut() {
            list.sort_by(|&a, &b| {
                transitions[b]
                    .priority
                    .cmp(&transitions[a].priority)
                    .then_with(|| a.cmp(&b))
            });
        }
        for list in triggers_from.values_mut() {
            list.sort_unstable();
            list.dedup();
        }

        Ok(Self {
            states: state_defs,
            state_index,
            triggers: trigger_names,
            trigger_index,
            transitions,
            initial,
            finals,
            lookup,
            triggers_from,
        })
    }

    #[inline]
    pub fn state_id(&self, name: &str) -> Option<u32> {
        self.state_index.get(name).copied()
    }

    #[inline]
    pub fn trigger_id(&self, name: &str) -> Option<u32> {
        self.trigger_index.get(name).copied()
    }

    #[inline]
    pub fn state_name(&self, id: u32) -> Option<&str> {
        self.states.get(id as usize).map(|s| s.name.as_str())
    }

    #[inline]
    pub fn trigger_name(&self, id: u32) -> Option<&str> {
        self.triggers.get(id as usize).map(|s| s.as_str())
    }

    /// Resolve the default entry state for a composite (initial child chain).
    pub fn resolve_entry(&self, state: u32) -> u32 {
        let mut cur = state;
        loop {
            let Some(child) = self.states[cur as usize].initial_child else {
                break;
            };
            cur = child;
        }
        cur
    }

    /// Ancestor chain from leaf to root (inclusive).
    pub fn ancestors(&self, leaf: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut cur = Some(leaf);
        while let Some(id) = cur {
            out.push(id);
            cur = self.states[id as usize].parent;
        }
        out
    }

    /// States to exit when moving from `from_leaf` to `to_leaf` (innermost first).
    pub fn exit_set(&self, from_leaf: u32, to_leaf: u32) -> Vec<u32> {
        let to_set: HashSet<u32> = self.ancestors(to_leaf).into_iter().collect();
        let from_anc = self.ancestors(from_leaf);
        let mut out = Vec::new();
        for &s in &from_anc {
            if to_set.contains(&s) {
                break;
            }
            out.push(s);
        }
        out
    }

    /// States to enter when moving from `from_leaf` to `to_leaf` (outermost first).
    pub fn enter_set(&self, from_leaf: u32, to_leaf: u32) -> Vec<u32> {
        let from_set: HashSet<u32> = self.ancestors(from_leaf).into_iter().collect();
        let to_anc = self.ancestors(to_leaf);
        let mut out = Vec::new();
        for &s in to_anc.iter().rev() {
            if !from_set.contains(&s) {
                out.push(s);
            }
        }
        out
    }
}

fn validate_state_id(id: u32, states: &[StateDef], ti: usize) -> Result<(), String> {
    if id as usize >= states.len() {
        return Err(format!("transition {ti} references invalid state id {id}"));
    }
    Ok(())
}

fn matches_source(leaf: u32, sources: &TransitionSources) -> bool {
    match sources {
        TransitionSources::One(s) => leaf == *s,
        TransitionSources::Many(v) => v.contains(&leaf),
        TransitionSources::Any => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simple_spec() -> FsmSpec {
        let states = vec![
            StateDef {
                name: "idle".into(),
                parent: None,
                initial_child: None,
                is_history: false,
                is_final: false,
            },
            StateDef {
                name: "running".into(),
                parent: None,
                initial_child: None,
                is_history: false,
                is_final: false,
            },
            StateDef {
                name: "done".into(),
                parent: None,
                initial_child: None,
                is_history: false,
                is_final: true,
            },
        ];
        let triggers = vec!["start".into(), "finish".into()];
        let transitions = vec![
            TransitionDef {
                trigger: 0,
                sources: TransitionSources::One(0),
                dest: TransitionDest::State(1),
                priority: 0,
            },
            TransitionDef {
                trigger: 1,
                sources: TransitionSources::One(1),
                dest: TransitionDest::State(2),
                priority: 0,
            },
        ];
        FsmSpec::build(states, triggers, transitions, "idle").unwrap()
    }

    #[test]
    fn build_simple() {
        let spec = simple_spec();
        assert_eq!(spec.initial, 0);
        assert!(spec.finals.contains(&2));
        assert_eq!(spec.lookup[&(0, 0)], vec![0]);
    }

    #[test]
    fn hierarchical_entry() {
        let states = vec![
            StateDef {
                name: "root".into(),
                parent: None,
                initial_child: Some(1),
                is_history: false,
                is_final: false,
            },
            StateDef {
                name: "a".into(),
                parent: Some(0),
                initial_child: None,
                is_history: false,
                is_final: false,
            },
        ];
        let spec = FsmSpec::build(
            states,
            vec!["go".into()],
            vec![TransitionDef {
                trigger: 0,
                sources: TransitionSources::Any,
                dest: TransitionDest::State(0),
                priority: 0,
            }],
            "root",
        )
        .unwrap();
        assert_eq!(spec.resolve_entry(0), 1);
    }
}
