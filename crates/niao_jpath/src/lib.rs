//! JSONPath / JMESPath queries, JSON Pointer and JSON Patch over values.
//!
//! Native Rust implementation with JSONPath via `jsonpath_lib2`, JMESPath via
//! `jmespath`, RFC 6902/7396 patch via `json-patch`, and native JSON Pointer.

mod error;
mod jmespath;
mod jsonpath;
mod parallel;
mod patch;
mod pointer;
mod value;

pub use error::{JpathError, JpathResult};
pub use jmespath::{
    compile as compile_jmes, search as jmes, search_with_compiled, valid as jmes_valid,
    CompiledJmes,
};
pub use jsonpath::{
    compile as compile_path, delete as path_delete, find as path_find, find_one as path_find_one,
    find_pointers, replace as path_replace, search as path_search, valid as path_valid,
    CompiledJsonPath,
};
pub use parallel::{
    parallel_find, parallel_find_one, parallel_jmes, parallel_jmes_compiled, parallel_search,
    ParallelOpts,
};
pub use patch::{
    apply as patch_apply, diff, merge as merge_patch, op_names as patch_op_names,
    test as patch_test, valid as patch_valid,
};
pub use pointer::{
    create_path, escape as pointer_escape, exists as pointer_exists, get as pointer_get,
    join as pointer_join, parent as pointer_parent, remove as pointer_remove,
    resolve as pointer_resolve, set as pointer_set, test as pointer_test,
    unescape as pointer_unescape,
};
pub use value::values_equal;
