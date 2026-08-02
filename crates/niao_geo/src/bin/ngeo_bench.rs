//! Micro-benchmarks for ngeo hot paths.
//! Run: cargo run -p niao_geo --bin ngeo_bench --release

use niao_geo::{
    batch_haversine_m, batch_haversine_m_naive, haversine_m, parse_geojson, point_in_ring, Coord,
    Polygon,
};
use std::time::Instant;

fn bench<F: Fn() -> usize>(name: &str, f: F, warmup: u32, iters: u32) {
    for _ in 0..warmup {
        let _ = f();
    }
    let mut samples = Vec::with_capacity(iters as usize);
    for _ in 0..iters {
        let t0 = Instant::now();
        let n = f();
        samples.push(t0.elapsed().as_nanos() as u64);
        let _ = n;
    }
    samples.sort_unstable();
    let mean: u64 = samples.iter().sum::<u64>() / iters as u64;
    let p50 = samples[samples.len() / 2];
    let ops_per_sec = if mean > 0 {
        1_000_000_000.0 / mean as f64
    } else {
        0.0
    };
    println!("{name}: mean={mean} ns/op ({ops_per_sec:.0} ops/s) p50={p50} ns (n={iters})");
}

fn make_targets(n: usize) -> Vec<Coord> {
    (0..n)
        .map(|i| {
            let lon = (i as f64 * 0.01) % 180.0;
            let lat = (i as f64 * 0.007) % 80.0;
            Coord::new(lon, lat).unwrap()
        })
        .collect()
}

fn unit_square() -> Polygon {
    let ring = vec![
        Coord::new(0.0, 0.0).unwrap(),
        Coord::new(1.0, 0.0).unwrap(),
        Coord::new(1.0, 1.0).unwrap(),
        Coord::new(0.0, 1.0).unwrap(),
        Coord::new(0.0, 0.0).unwrap(),
    ];
    Polygon::new(ring, vec![]).unwrap()
}

fn main() {
    let warmup = 3u32;
    let iters = 30u32;

    let a = Coord::new(-73.9857, 40.7484).unwrap();
    let b = Coord::new(-0.1276, 51.5072).unwrap();

    bench(
        "haversine NYC-London",
        || haversine_m(a, b).round() as usize,
        warmup,
        iters,
    );

    bench(
        "parse GeoJSON Point",
        || {
            parse_geojson(r#"{"type":"Point","coordinates":[-73.9857,40.7484]}"#)
                .map(|_| 1)
                .unwrap_or(0)
        },
        warmup,
        iters,
    );

    let poly = unit_square();
    let probe = Coord::new(0.5, 0.5).unwrap();
    bench(
        "point in polygon",
        || poly.contains(probe) as usize,
        warmup,
        iters,
    );

    let targets_1k = make_targets(1_000);
    bench(
        "batch haversine 1k (parallel)",
        || batch_haversine_m(a, &targets_1k).len(),
        warmup,
        iters,
    );
    bench(
        "batch haversine 1k (naive)",
        || batch_haversine_m_naive(a, &targets_1k).len(),
        warmup,
        iters,
    );

    let ring = poly.exterior.clone();
    bench(
        "ray cast point in ring",
        || point_in_ring(probe, &ring) as usize,
        warmup,
        iters,
    );
}
