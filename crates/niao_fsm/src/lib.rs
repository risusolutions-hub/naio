//! Finite state machines and hierarchical statecharts — states, guards,
//! transitions, and enter/exit ordering used by the Niao `nfsm` library.

mod machine;
mod spec;

pub use machine::{ActiveTransition, FsmEngine, FsmError, TransitionApply};
pub use spec::{FsmSpec, StateDef, TransitionDef, TransitionDest, TransitionSources};
