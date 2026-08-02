//! Micro-benchmarks for nmsgpack hot paths.
//! Run: cargo run -p niao_msgpack --bin nmsgpack_bench --release

use niao_msgpack::{pack, unpack, MsgValue, PackOptions, Packer, UnpackOptions, MAX_BYTES};
use std::time::Instant;

fn make_nested_map(depth: usize, width: usize) -> MsgValue {
    if depth == 0 {
        return MsgValue::Int(42);
    }
    let mut pairs = Vec::with_capacity(width);
    for i in 0..width {
        pairs.push((
            MsgValue::String(format!("k{i}")),
            make_nested_map(depth - 1, width),
        ));
    }
    MsgValue::Map(pairs)
}

fn make_array(n: usize) -> MsgValue {
    let mut items = Vec::with_capacity(n);
    for i in 0..n {
        items.push(MsgValue::Map(vec![
            (MsgValue::String("id".into()), MsgValue::Int(i as i64)),
            (
                MsgValue::String("name".into()),
                MsgValue::String(format!("item-{i}")),
            ),
            (
                MsgValue::String("active".into()),
                MsgValue::Bool(i % 2 == 0),
            ),
        ]));
    }
    MsgValue::Array(items)
}

fn bench<F: Fn() -> usize>(name: &str, f: F, warmup: u32, iters: u32) {
    for _ in 0..warmup {
        let _ = f();
    }
    let mut samples = Vec::with_capacity(iters as usize);
    for _ in 0..iters {
        let t0 = Instant::now();
        let n = f();
        let elapsed = t0.elapsed().as_nanos() as u64;
        samples.push(elapsed);
        let _ = n;
    }
    samples.sort_unstable();
    let sum: u64 = samples.iter().sum();
    let mean = sum / iters as u64;
    let p50 = samples[samples.len() / 2];
    println!("{name}: mean={mean} ns p50={p50} ns (n={iters})");
}

fn main() {
    let opts = PackOptions::default();
    let uopts = UnpackOptions::default();

    let small = MsgValue::Map(vec![
        (MsgValue::String("a".into()), MsgValue::Int(1)),
        (
            MsgValue::String("b".into()),
            MsgValue::String("hello".into()),
        ),
        (
            MsgValue::String("c".into()),
            MsgValue::Array(vec![MsgValue::Bool(true), MsgValue::Float(3.14)]),
        ),
    ]);
    let small_bytes = pack(&small, &opts).unwrap();

    let nested = make_nested_map(4, 8);
    let nested_bytes = pack(&nested, &opts).unwrap();

    let array_1k = make_array(1000);
    let array_bytes = pack(&array_1k, &opts).unwrap();

    println!(
        "payload sizes: small={} nested={} array1k={} (limit {MAX_BYTES})",
        small_bytes.len(),
        nested_bytes.len(),
        array_bytes.len()
    );

    let warmup = 3u32;
    let iters = 20u32;

    bench(
        "pack small map",
        || pack(&small, &opts).map(|b| b.len()).unwrap_or(0),
        warmup,
        iters,
    );
    bench(
        "unpack small map",
        || unpack(&small_bytes, &uopts).map(|_| 1).unwrap_or(0),
        warmup,
        iters,
    );
    bench(
        "pack nested map depth=4 width=8",
        || pack(&nested, &opts).map(|b| b.len()).unwrap_or(0),
        warmup,
        iters,
    );
    bench(
        "unpack nested map",
        || unpack(&nested_bytes, &uopts).map(|_| 1).unwrap_or(0),
        warmup,
        iters,
    );
    bench(
        "pack array 1000 objects",
        || pack(&array_1k, &opts).map(|b| b.len()).unwrap_or(0),
        warmup,
        iters,
    );
    bench(
        "unpack array 1000 objects",
        || unpack(&array_bytes, &uopts).map(|_| 1).unwrap_or(0),
        warmup,
        iters,
    );
    bench(
        "stream pack 100 ints",
        || {
            let mut p = Packer::with_defaults();
            for i in 0..100 {
                p.pack(&MsgValue::Int(i)).unwrap();
            }
            p.finish().len()
        },
        warmup,
        iters,
    );
    bench(
        "roundtrip array 1000 objects",
        || {
            let b = pack(&array_1k, &opts).unwrap();
            unpack(&b, &uopts).map(|_| 1).unwrap_or(0)
        },
        warmup,
        iters,
    );
}
