//! Arbitrary-precision decimals and exact rationals for Niao.
//!
//! Built on [`niao_bignum`] — zero extra numeric dependencies.

mod context;
mod decimal;
mod error;
mod fraction;
mod rounding;

pub use context::Context;
pub use decimal::{parse_decimal, Decimal, DecimalKind};
pub use error::{DecimalError, DecimalResult};
pub use fraction::{parse_fraction, Fraction};
pub use rounding::RoundingMode;

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn bench_accumulates() {
        let ctx = Context::new(28, RoundingMode::HalfEven);
        let d = parse_decimal("19.995").unwrap();
        let tax = parse_decimal("0.0825").unwrap().mul(&d, &ctx).unwrap();
        let total = d.add(&tax, &ctx).unwrap().quantize(-2, &ctx).unwrap();
        assert_eq!(total.to_string(), "21.64");
        let mut acc = Decimal::zero();
        for _ in 0..100 {
            acc = acc.add(&total, &ctx).unwrap();
        }
        assert!(!acc.is_zero());
        assert!(acc.to_string().starts_with("2164"));
    }

    #[test]
    fn money_pipeline() {
        let ctx = Context::money();
        let price = parse_decimal("19.995").unwrap();
        let tax = parse_decimal("0.0825").unwrap();
        let total = price.add(&tax.mul(&price, &ctx).unwrap(), &ctx).unwrap();
        let rounded = total.quantize(-2, &ctx).unwrap();
        assert_eq!(rounded.to_string(), "21.64");
    }
}
