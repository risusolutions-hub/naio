//! `niao_reflect` — runtime introspection primitives for the Niao `nreflect` library.
//!
//! Doc-comment extraction, module AST scanning, source slicing by span, and a
//! thread-local registry of loaded modules / call-stack frames.

mod doc;
mod module;
mod registry;
mod stack;

pub use doc::{doc_for_decl, doc_from_source, extract_doc_before_line};
pub use module::{
    find_decl_by_name, format_signature, format_type_name, module_members, parse_module_info,
    scan_sources_parallel, MemberInfo, ModuleInfo, ParamInfo, SignatureInfo,
};
pub use registry::{
    bind_function, clear_registry, function_meta, list_modules, module_record, register_module,
    ModuleRecord,
};
pub use stack::{current_frame, push_frame, stack_frames, FrameGuard, StackFrame};
