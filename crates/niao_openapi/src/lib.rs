//! OpenAPI 3 spec generation (from ahiru routes) + typed client stub generation.
//! (~fastapi openapi, openapi-gen subset)

mod client;
mod doc;
mod error;
mod merge;
mod parallel;
mod pathutil;
mod schema;
mod validate;

pub use client::{client_stub, client_stub_str, ClientStubOpts};
pub use doc::{from_ahiru, from_routes, OpenApiDoc};
pub use error::{OpenApiError, OpenApiResult};
pub use merge::merge;
pub use parallel::{parallel_client_stubs, parallel_validate, sample_routes};
pub use pathutil::{method_key, normalize_path, operation_id, path_params};
pub use schema::{
    infer_schema, operation, param, request_body, response, schema_array, schema_boolean,
    schema_integer, schema_number, schema_object, schema_ref, schema_string,
};
pub use validate::{is_valid, validate, ValidationIssue, ValidationReport};

#[cfg(test)]
mod integration;
