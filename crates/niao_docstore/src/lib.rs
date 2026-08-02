//! Embedded JSON document store with queries and secondary indexes (~tinydb).

mod index;
mod query;
mod store;
mod value;

pub use query::{extract_eq_field, extract_eq_fields, matches, QueryError};
pub use store::{DocumentStore, StoreError, UpdateCond, DEFAULT_TABLE, META_KEY};
pub use value::{
    cmp_values, get_path, merge_patch, set_path, strip_id, values_equal, with_id, IndexKey,
};
