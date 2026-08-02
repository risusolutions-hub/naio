//! Physical units, dimensional analysis, and quantity conversion for Niao.
//!
//! Pint-inspired dynamic unit registry with SI base dimensions, compound
//! unit parsing, affine temperature units, and quantity arithmetic.

mod dimension;
mod error;
mod parse;
mod quantity;
mod registry;
mod unit;

pub use dimension::Dimension;
pub use error::{UnitError, UnitResult};
pub use parse::{parse_quantity, parse_unit_expr, parse_unit_name};
pub use quantity::Quantity;
pub use registry::Registry;
pub use unit::{Affine, Unit};
