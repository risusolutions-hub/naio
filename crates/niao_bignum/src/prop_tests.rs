//! Property tests comparing `niao_bignum` against `num-bigint` (dev-only).

use crate::BigInt;
use num_bigint::BigInt as NumBigInt;
use num_traits::FromPrimitive;
use std::str::FromStr;

fn to_num(v: &BigInt) -> NumBigInt {
    NumBigInt::from_str(&v.to_string()).expect("parse into num-bigint")
}

fn from_num(v: &NumBigInt) -> BigInt {
    BigInt::from_str(&v.to_string()).expect("parse from num-bigint")
}

#[test]
fn prop_add_sub_mul() {
    let cases = [
        ("0", "0"),
        ("1", "1"),
        ("9223372036854775807", "9223372036854775807"),
        ("-9223372036854775808", "3"),
        ("999999999999999999999999999999", "123456789012345678901234567890"),
        ("-123456789012345678901234567890", "-987654321098765432109876543210"),
    ];
    for (a, b) in cases {
        let na = BigInt::from_str(a).unwrap();
        let nb = BigInt::from_str(b).unwrap();
        let ref_a = to_num(&na);
        let ref_b = to_num(&nb);

        assert_eq!((&na + &nb).to_string(), (ref_a.clone() + ref_b.clone()).to_string());
        assert_eq!((&na - &nb).to_string(), (ref_a.clone() - ref_b.clone()).to_string());
        assert_eq!((&na * &nb).to_string(), (ref_a * ref_b).to_string());
    }
}

#[test]
fn prop_div_mod() {
    let cases = [
        ("100", "7"),
        ("-100", "7"),
        ("100", "-7"),
        ("-100", "-7"),
        ("999999999999999999999999999999", "123456789012345678901234567890"),
        ("-999999999999999999999999999999", "123456789012345678901234567890"),
    ];
    for (a, b) in cases {
        let na = BigInt::from_str(a).unwrap();
        let nb = BigInt::from_str(b).unwrap();
        let ref_a = to_num(&na);
        let ref_b = to_num(&nb);

        assert_eq!((&na / &nb).to_string(), (ref_a.clone() / ref_b.clone()).to_string());
        assert_eq!((&na % &nb).to_string(), (ref_a % ref_b).to_string());
    }
}

#[test]
fn prop_pow_cmp() {
    let base = BigInt::from_str("12345").unwrap();
    let ref_base = to_num(&base);
    assert_eq!(base.pow(5).to_string(), ref_base.pow(5).to_string());

    let a = BigInt::from_str("-42").unwrap();
    let b = BigInt::from_str("7").unwrap();
    let ref_a = to_num(&a);
    let ref_b = to_num(&b);
    assert_eq!(a.cmp(&b), ref_a.cmp(&ref_b));
}

#[test]
fn vm_factorial_50_matches_num_bigint() {
    let mut acc = NumBigInt::from(1i64);
    for i in 2..=50 {
        acc *= NumBigInt::from(i);
    }
    let ours = {
        let mut v = BigInt::from(1);
        for i in 2..=50 {
            v *= BigInt::from(i);
        }
        v
    };
    assert_eq!(ours.to_string(), acc.to_string());
}

#[test]
fn vm_overflow_promotion() {
    let a = BigInt::from(i64::MAX);
    let b = BigInt::from(i64::MAX);
    let sum = &a + &b;
    let expected = NumBigInt::from_i64(i64::MAX).unwrap() + NumBigInt::from_i64(i64::MAX).unwrap();
    assert_eq!(sum.to_string(), expected.to_string());
    assert!(sum > BigInt::from(i64::MAX));
}

#[test]
fn parse_radix_roundtrip() {
    let s = "18446744073709551615";
    let v = BigInt::parse_bytes(s.as_bytes(), 10).unwrap();
    let (sign, digits) = v.to_radix_be(10);
    assert_eq!(sign, crate::Sign::Plus);
    let text: String = digits.iter().map(|d| char::from(b'0' + *d)).collect();
    assert_eq!(text, s);
}

#[test]
fn from_num_samples() {
    let samples = [
        "0",
        "1",
        "-1",
        "30414093201713378043612608166064768844377641568960512000000000000",
    ];
    for s in samples {
        let n = NumBigInt::from_str(s).unwrap();
        let ours = from_num(&n);
        assert_eq!(ours.to_string(), n.to_string());
    }
}
