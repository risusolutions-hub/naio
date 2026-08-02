//! Runtime FSM engine — fast transition lookup and hierarchical enter/exit.

use crate::spec::{FsmSpec, TransitionDest, TransitionSources};
use std::collections::HashMap;

/// Errors surfaced while building or driving a machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsmError {
    InvalidState(u32),
    InvalidTrigger(u32),
    InvalidTransition(usize),
    NoTransition { trigger: u32, state: u32 },
    AlreadyFinal,
    Validation(String),
}

impl std::fmt::Display for FsmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FsmError::InvalidState(id) => write!(f, "invalid state id {id}"),
            FsmError::InvalidTrigger(id) => write!(f, "invalid trigger id {id}"),
            FsmError::InvalidTransition(i) => write!(f, "invalid transition index {i}"),
            FsmError::NoTransition { trigger, state } => {
                write!(f, "no transition for trigger {trigger} from state {state}")
            }
            FsmError::AlreadyFinal => write!(f, "machine is in a final state"),
            FsmError::Validation(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for FsmError {}

/// One log entry after a successful transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionRecord {
    pub trigger: u32,
    pub from: u32,
    pub to: u32,
    pub transition: usize,
}

/// Result of applying a transition to the engine state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionApply {
    pub trigger: u32,
    pub from_leaf: u32,
    pub to_leaf: u32,
    pub transition: usize,
    pub exited: Vec<u32>,
    pub entered: Vec<u32>,
    pub is_internal: bool,
}

/// A candidate transition returned before guards run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveTransition {
    pub index: usize,
    pub trigger: u32,
    pub from_leaf: u32,
    pub to_leaf: u32,
    pub is_internal: bool,
}

/// Mutable finite-state machine runtime.
#[derive(Debug, Clone)]
pub struct FsmEngine {
    spec: FsmSpec,
    leaf: u32,
    /// Shallow history: composite parent -> last active child leaf.
    history: HashMap<u32, u32>,
    log: Vec<TransitionRecord>,
    fired: u64,
    ignore_invalid: bool,
}

impl FsmEngine {
    pub fn new(spec: FsmSpec) -> Result<Self, FsmError> {
        let leaf = spec.resolve_entry(spec.initial);
        Ok(Self {
            spec,
            leaf,
            history: HashMap::new(),
            log: Vec::new(),
            fired: 0,
            ignore_invalid: false,
        })
    }

    pub fn spec(&self) -> &FsmSpec {
        &self.spec
    }

    pub fn set_ignore_invalid(&mut self, v: bool) {
        self.ignore_invalid = v;
    }

    pub fn ignore_invalid(&self) -> bool {
        self.ignore_invalid
    }

    #[inline]
    pub fn current(&self) -> u32 {
        self.leaf
    }

    pub fn current_states(&self) -> Vec<u32> {
        self.spec.ancestors(self.leaf)
    }

    pub fn is_final(&self) -> bool {
        self.spec.finals.contains(&self.leaf)
    }

    pub fn transition_count(&self) -> u64 {
        self.fired
    }

    pub fn history(&self) -> &[TransitionRecord] {
        &self.log
    }

    pub fn clear_history(&mut self) {
        self.log.clear();
    }

    pub fn reset(&mut self) {
        self.leaf = self.spec.resolve_entry(self.spec.initial);
        self.history.clear();
        self.log.clear();
        self.fired = 0;
    }

    /// Triggers that have at least one transition from the current leaf (guards not checked).
    pub fn available_triggers(&self) -> Vec<u32> {
        self.spec
            .triggers_from
            .get(&self.leaf)
            .cloned()
            .unwrap_or_default()
    }

    /// Candidate transitions for `trigger`, highest priority first.
    pub fn candidates(&self, trigger: u32) -> Result<Vec<ActiveTransition>, FsmError> {
        if trigger as usize >= self.spec.triggers.len() {
            return Err(FsmError::InvalidTrigger(trigger));
        }
        let Some(indices) = self.spec.lookup.get(&(self.leaf, trigger)) else {
            if self.ignore_invalid {
                return Ok(Vec::new());
            }
            return Err(FsmError::NoTransition {
                trigger,
                state: self.leaf,
            });
        };
        let mut out = Vec::with_capacity(indices.len());
        for &idx in indices {
            let tr = &self.spec.transitions[idx];
            let to_leaf = self.resolve_dest(self.leaf, &tr.dest)?;
            out.push(ActiveTransition {
                index: idx,
                trigger,
                from_leaf: self.leaf,
                to_leaf,
                is_internal: matches!(tr.dest, TransitionDest::Internal),
            });
        }
        Ok(out)
    }

    fn resolve_dest(&self, from_leaf: u32, dest: &TransitionDest) -> Result<u32, FsmError> {
        Ok(match dest {
            TransitionDest::State(s) => self.spec.resolve_entry(*s),
            TransitionDest::Internal | TransitionDest::Same => from_leaf,
            TransitionDest::HistoryShallow(parent) => {
                if let Some(&child) = self.history.get(parent) {
                    child
                } else {
                    self.spec.resolve_entry(*parent)
                }
            }
        })
    }

    /// Apply transition by index after guards and hooks have been satisfied externally.
    pub fn apply(&mut self, transition: usize) -> Result<TransitionApply, FsmError> {
        if transition >= self.spec.transitions.len() {
            return Err(FsmError::InvalidTransition(transition));
        }
        let tr = self.spec.transitions[transition].clone();
        let from_leaf = self.leaf;
        let to_leaf = self.resolve_dest(from_leaf, &tr.dest)?;
        let is_internal = matches!(tr.dest, TransitionDest::Internal);

        if is_internal {
            self.record(tr.trigger, from_leaf, to_leaf, transition);
            return Ok(TransitionApply {
                trigger: tr.trigger,
                from_leaf,
                to_leaf,
                transition,
                exited: Vec::new(),
                entered: Vec::new(),
                is_internal: true,
            });
        }

        if from_leaf == to_leaf && matches!(tr.dest, TransitionDest::Same) {
            self.record(tr.trigger, from_leaf, to_leaf, transition);
            return Ok(TransitionApply {
                trigger: tr.trigger,
                from_leaf,
                to_leaf,
                transition,
                exited: Vec::new(),
                entered: Vec::new(),
                is_internal: false,
            });
        }

        let exited = self.spec.exit_set(from_leaf, to_leaf);
        let entered = self.spec.enter_set(from_leaf, to_leaf);

        // Update shallow history for parents being exited.
        for &s in &exited {
            if let Some(parent) = self.spec.states[s as usize].parent {
                self.history.insert(parent, s);
            }
        }

        self.leaf = to_leaf;
        self.record(tr.trigger, from_leaf, to_leaf, transition);

        Ok(TransitionApply {
            trigger: tr.trigger,
            from_leaf,
            to_leaf,
            transition,
            exited,
            entered,
            is_internal: false,
        })
    }

    fn record(&mut self, trigger: u32, from: u32, to: u32, transition: usize) {
        self.fired += 1;
        self.log.push(TransitionRecord {
            trigger,
            from,
            to,
            transition,
        });
    }

    /// Graphviz DOT export for documentation / debugging.
    pub fn to_dot(&self) -> String {
        let mut out = String::from("digraph fsm {\n  rankdir=LR;\n");
        for (i, s) in self.spec.states.iter().enumerate() {
            let id = i;
            let shape = if s.is_final {
                "doublecircle"
            } else if s.initial_child.is_some() {
                "box"
            } else if s.is_history {
                "circle"
            } else {
                "ellipse"
            };
            let label = if id as u32 == self.spec.initial {
                format!("{}*", s.name)
            } else {
                s.name.clone()
            };
            let highlight = if id as u32 == self.leaf {
                ", style=bold"
            } else {
                ""
            };
            out.push_str(&format!(
                "  s{id} [label=\"{label}\" shape={shape}{highlight}];\n"
            ));
            if let Some(p) = s.parent {
                out.push_str(&format!("  s{p} -> s{id} [style=dashed arrowhead=none];\n"));
            }
        }
        for (ti, tr) in self.spec.transitions.iter().enumerate() {
            let trig = self.spec.trigger_name(tr.trigger).unwrap_or("?");
            let dest_label = match tr.dest {
                TransitionDest::State(d) => format!("s{d}"),
                TransitionDest::Internal => "internal".into(),
                TransitionDest::Same => "same".into(),
                TransitionDest::HistoryShallow(p) => format!("hist_s{p}"),
            };
            let _src_label = match &tr.sources {
                TransitionSources::One(s) => format!("s{s}"),
                TransitionSources::Many(v) => v
                    .iter()
                    .map(|s| format!("s{s}"))
                    .collect::<Vec<_>>()
                    .join(","),
                TransitionSources::Any => "*".into(),
            };
            out.push_str(&format!("  t{ti} [label=\"{trig}\" shape=plaintext];\n"));
            match &tr.sources {
                TransitionSources::One(s) => {
                    out.push_str(&format!("  s{s} -> t{ti};\n"));
                }
                TransitionSources::Many(v) => {
                    for s in v {
                        out.push_str(&format!("  s{s} -> t{ti};\n"));
                    }
                }
                TransitionSources::Any => {
                    out.push_str(&format!("  __any -> t{ti} [style=dashed];\n"));
                }
            }
            if let TransitionDest::State(d) = tr.dest {
                out.push_str(&format!("  t{ti} -> s{d};\n"));
            } else {
                out.push_str(&format!("  t{ti} -> {dest_label} [style=dotted];\n"));
            }
        }
        out.push_str("}\n");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{StateDef, TransitionDef, TransitionDest, TransitionSources};

    fn traffic_spec() -> FsmSpec {
        FsmSpec::build(
            vec![
                StateDef {
                    name: "red".into(),
                    parent: None,
                    initial_child: None,
                    is_history: false,
                    is_final: false,
                },
                StateDef {
                    name: "green".into(),
                    parent: None,
                    initial_child: None,
                    is_history: false,
                    is_final: false,
                },
                StateDef {
                    name: "yellow".into(),
                    parent: None,
                    initial_child: None,
                    is_history: false,
                    is_final: false,
                },
            ],
            vec!["next".into()],
            vec![
                TransitionDef {
                    trigger: 0,
                    sources: TransitionSources::One(0),
                    dest: TransitionDest::State(1),
                    priority: 0,
                },
                TransitionDef {
                    trigger: 0,
                    sources: TransitionSources::One(1),
                    dest: TransitionDest::State(2),
                    priority: 0,
                },
                TransitionDef {
                    trigger: 0,
                    sources: TransitionSources::One(2),
                    dest: TransitionDest::State(0),
                    priority: 0,
                },
            ],
            "red",
        )
        .unwrap()
    }

    #[test]
    fn cycle() {
        let mut m = FsmEngine::new(traffic_spec()).unwrap();
        assert_eq!(m.current(), 0);
        m.apply(0).unwrap();
        assert_eq!(m.current(), 1);
        m.apply(1).unwrap();
        assert_eq!(m.current(), 2);
        m.apply(2).unwrap();
        assert_eq!(m.current(), 0);
        assert_eq!(m.transition_count(), 3);
    }

    #[test]
    fn internal_transition() {
        let spec = FsmSpec::build(
            vec![StateDef {
                name: "on".into(),
                parent: None,
                initial_child: None,
                is_history: false,
                is_final: false,
            }],
            vec!["tick".into()],
            vec![TransitionDef {
                trigger: 0,
                sources: TransitionSources::One(0),
                dest: TransitionDest::Internal,
                priority: 0,
            }],
            "on",
        )
        .unwrap();
        let mut m = FsmEngine::new(spec).unwrap();
        let apply = m.apply(0).unwrap();
        assert!(apply.is_internal);
        assert!(apply.exited.is_empty());
        assert_eq!(m.current(), 0);
    }

    #[test]
    fn hierarchical() {
        let spec = FsmSpec::build(
            vec![
                StateDef {
                    name: "work".into(),
                    parent: None,
                    initial_child: Some(1),
                    is_history: false,
                    is_final: false,
                },
                StateDef {
                    name: "busy".into(),
                    parent: Some(0),
                    initial_child: None,
                    is_history: false,
                    is_final: false,
                },
                StateDef {
                    name: "idle".into(),
                    parent: None,
                    initial_child: None,
                    is_history: false,
                    is_final: false,
                },
            ],
            vec!["pause".into(), "resume".into()],
            vec![
                TransitionDef {
                    trigger: 0,
                    sources: TransitionSources::One(1),
                    dest: TransitionDest::State(2),
                    priority: 0,
                },
                TransitionDef {
                    trigger: 1,
                    sources: TransitionSources::One(2),
                    dest: TransitionDest::State(0),
                    priority: 0,
                },
            ],
            "work",
        )
        .unwrap();
        let mut m = FsmEngine::new(spec).unwrap();
        assert_eq!(m.current(), 1);
        let apply = m.apply(0).unwrap();
        assert_eq!(apply.exited, vec![1, 0]);
        assert_eq!(m.current(), 2);
        let apply = m.apply(1).unwrap();
        assert_eq!(apply.entered, vec![0, 1]);
        assert_eq!(m.current(), 1);
    }
}
