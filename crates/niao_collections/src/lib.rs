//! Native collections for Niao: insertion-ordered maps/sets (and fast hashing).
//!
//! Modules are split so parallel agents can land `ahash` / `dashmap` without
//! colliding on the IndexMap implementation.

pub mod hasher;
pub mod indexmap;

pub use hasher::{hash_bytes, AHasher, HashMap, HashMapExt, HashSet, HashSetExt, RandomState};
pub use indexmap::{IndexMap, IndexSet};
