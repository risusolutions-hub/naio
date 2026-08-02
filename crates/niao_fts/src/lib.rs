//! Embedded full-text search: inverted index, BM25, phrase/prefix, facets.
//!
//! Pure Rust engine used by the Niao `nfts` runtime module. Keep this crate
//! free of VM types so a future C11 port only needs a thin boundary.

mod error;
mod index;
mod query;
mod score;
mod tokenize;

pub use error::{FtsError, FtsResult};
pub use index::{analyze, FacetCount, Hit, Index, SchemaInfo};
pub use query::{parse, Query};
pub use tokenize::tokenize;
