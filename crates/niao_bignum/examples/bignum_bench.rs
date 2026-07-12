//! BigInt multiply throughput vs `num-bigint`.
//!
//! Target: Karatsuba beats schoolbook above 256 bits.

use niao_bignum::bench_util;
use niao_bignum::{BigInt, KARATSUBA_THRESHOLD};
use std::str::FromStr;
use std::time::Instant;

const ITERS: u32 = 256;

fn make_bigint_decimal(limbs: usize) -> BigInt {
    let mut dec = String::from('1');
    for _ in 0..limbs * 18 {
        dec.push('7');
    }
    BigInt::from_str(&dec).unwrap()
}

fn bench_mul(name: &str, a: &BigInt, b: &BigInt) -> f64 {
    let start = Instant::now();
    for _ in 0..ITERS {
        std::hint::black_box((a * b).to_string());
    }
    let secs = start.elapsed().as_secs_f64();
    let ops = ITERS as f64 / secs;
    println!("{name}: {ops:.0} mul/s ({ITERS} iters in {secs:.3}s)");
    ops
}

fn bench_num_bigint(a: &BigInt, b: &BigInt) -> f64 {
    use num_bigint::BigInt as Num;
    use std::str::FromStr as _;
    let na = Num::from_str(&a.to_string()).unwrap();
    let nb = Num::from_str(&b.to_string()).unwrap();
    let start = Instant::now();
    for _ in 0..ITERS {
        std::hint::black_box((&na * &nb).to_string());
    }
    ITERS as f64 / start.elapsed().as_secs_f64()
}

fn bench_vm_factorial() -> f64 {
    let start = Instant::now();
    for _ in 0..32 {
        let mut acc = BigInt::from(1);
        for i in 2..=500 {
            acc *= BigInt::from(i);
        }
        std::hint::black_box(acc.to_string());
    }
    32.0 / start.elapsed().as_secs_f64()
}

fn bench_num_factorial() -> f64 {
    use num_bigint::BigInt as Num;
    let start = Instant::now();
    for _ in 0..32 {
        let mut acc = Num::from(1i64);
        for i in 2..=500 {
            acc *= Num::from(i);
        }
        std::hint::black_box(acc.to_string());
    }
    32.0 / start.elapsed().as_secs_f64()
}

fn main() {
    println!("=== niao_bignum bench (release recommended) ===");
    println!(
        "KARATSUBA_THRESHOLD = {KARATSUBA_THRESHOLD} limbs ({} bits)",
        KARATSUBA_THRESHOLD * 64
    );

    let (school_256, kara_256) = bench_util::bench_schoolbook_vs_karatsuba(4);
    let (school_128, kara_128) = bench_util::bench_schoolbook_vs_karatsuba(2);
    let (school_512, kara_512) = bench_util::bench_schoolbook_vs_karatsuba(8);
    let (school_2048, kara_2048) = bench_util::bench_schoolbook_vs_karatsuba(32);

    let ratio_256 = kara_256 / school_256;
    let ratio_128 = kara_128 / school_128;
    let ratio_512 = kara_512 / school_512;
    let ratio_2048 = kara_2048 / school_2048;

    println!(
        "karatsuba_vs_schoolbook_256bit: school={school_256:.0}/s kara={kara_256:.0}/s ratio={ratio_256:.2}x"
    );
    println!(
        "karatsuba_vs_schoolbook_128bit: school={school_128:.0}/s kara={kara_128:.0}/s ratio={ratio_128:.2}x"
    );
    println!(
        "karatsuba_vs_schoolbook_512bit: school={school_512:.0}/s kara={kara_512:.0}/s ratio={ratio_512:.2}x"
    );
    println!(
        "karatsuba_vs_schoolbook_2048bit: school={school_2048:.0}/s kara={kara_2048:.0}/s ratio={ratio_2048:.2}x"
    );

    let a512 = make_bigint_decimal(8);
    let b512 = make_bigint_decimal(8);
    let niao_ops = bench_mul("niao_mul_512bit", &a512, &b512);
    let num_ops = bench_num_bigint(&a512, &b512);
    println!(
        "niao_vs_num_bigint_512bit: niao={niao_ops:.0}/s num-bigint={num_ops:.0}/s ratio={:.2}x",
        niao_ops / num_ops
    );

    let vm_fact = bench_vm_factorial();
    let num_fact = bench_num_factorial();
    println!(
        "factorial_500_path: niao={vm_fact:.1}/s num-bigint={num_fact:.1}/s ratio={:.2}x",
        vm_fact / num_fact
    );

    let pass = ratio_256 >= 1.0;
    println!(
        "karatsuba_target_256bit_plus: {}",
        if pass { "PASS" } else { "FAIL" }
    );
    if !pass {
        std::process::exit(1);
    }
}
