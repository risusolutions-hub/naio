//! Thread-local registry of loaded modules and function → source bindings.

use crate::module::{module_members, ModuleInfo};
use niao_ast::Program;
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Clone)]
pub struct ModuleRecord {
    pub path: String,
    pub source: String,
    pub exports: Vec<String>,
    pub info: ModuleInfo,
}

#[derive(Clone)]
pub struct FunctionMeta {
    pub module_path: String,
    pub name: String,
    pub source: String,
    pub span_start: usize,
    pub span_end: usize,
    pub line: usize,
    pub col: usize,
}

thread_local! {
    static MODULES: RefCell<HashMap<String, ModuleRecord>> = RefCell::new(HashMap::new());
    static FUNCTIONS: RefCell<HashMap<usize, FunctionMeta>> = RefCell::new(HashMap::new());
}

/// Register or replace a loaded module (path → source). Parses members once.
pub fn register_module(
    path: impl Into<String>,
    source: impl Into<String>,
    program: Option<&Program>,
) {
    let path = path.into();
    let source = source.into();
    let info = program
        .map(module_members)
        .unwrap_or_else(|| crate::module::parse_module_info(&source));
    let exports: Vec<String> = info.functions.iter().map(|f| f.name.clone()).collect();
    MODULES.with(|m| {
        m.borrow_mut().insert(
            path.clone(),
            ModuleRecord {
                path,
                source,
                exports,
                info,
            },
        );
    });
}

/// Bind a live function value (by `Rc` address) to its module metadata.
pub fn bind_function(
    func_ptr: usize,
    module_path: impl Into<String>,
    name: impl Into<String>,
    span_start: usize,
    span_end: usize,
    line: usize,
    col: usize,
) {
    let module_path = module_path.into();
    let name = name.into();
    let source = MODULES.with(|m| {
        m.borrow()
            .get(&module_path)
            .map(|r| r.source.clone())
            .unwrap_or_default()
    });
    FUNCTIONS.with(|f| {
        f.borrow_mut().insert(
            func_ptr,
            FunctionMeta {
                module_path,
                name,
                source,
                span_start,
                span_end,
                line,
                col,
            },
        );
    });
}

pub fn function_meta(func_ptr: usize) -> Option<FunctionMeta> {
    FUNCTIONS.with(|f| f.borrow().get(&func_ptr).cloned())
}

pub fn list_modules() -> Vec<ModuleRecord> {
    MODULES.with(|m| m.borrow().values().cloned().collect())
}

pub fn module_record(path: &str) -> Option<ModuleRecord> {
    MODULES.with(|m| m.borrow().get(path).cloned())
}

pub fn clear_registry() {
    MODULES.with(|m| m.borrow_mut().clear());
    FUNCTIONS.with(|f| f.borrow_mut().clear());
}
