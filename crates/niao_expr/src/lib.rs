//! Safe sandboxed expression evaluator (~Python `simpleeval` / `asteval` subset).
//!
//! Parses and evaluates arithmetic, comparison, boolean, membership, ternary,
//! attribute/index access, list/dict literals, and a fixed builtin function set.
//! No statements, imports, or assignment — suitable for user formulas and config.

mod ast;
mod error;
mod eval;
mod lex;
mod parse;
mod value;

pub use ast::Compiled;
pub use error::ExprError;
pub use eval::{default_functions, default_operators, eval_once, BinOpTag, Evaluator, ExternalFn};
pub use parse::{parse, valid};
pub use value::{str_key, Value};
