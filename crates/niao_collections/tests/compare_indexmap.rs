//! Head-to-head vs `indexmap` crate (dev-dependency). Run with:
//! `cargo test -p niao_collections --release compare_perf -- --nocapture`

use niao_collections::indexmap::{FxBuildHasher, IndexMap as NiaoMap};
use std::time::Instant;

const N: usize = 200_000;

fn secs_insert_niao() -> f64 {
    let mut m = NiaoMap::with_capacity_and_hasher(N, FxBuildHasher);
    let t = Instant::now();
    for i in 0..N {
        m.insert(i, i);
    }
    std::hint::black_box(m.len());
    t.elapsed().as_secs_f64()
}

fn secs_insert_crate() -> f64 {
    let mut m = indexmap::IndexMap::with_capacity(N);
    let t = Instant::now();
    for i in 0..N {
        m.insert(i, i);
    }
    std::hint::black_box(m.len());
    t.elapsed().as_secs_f64()
}

#[test]
fn compare_perf_insert_get() {
    // Warmup
    let _ = secs_insert_niao();
    let _ = secs_insert_crate();

    let mut niao_insert = 0.0;
    let mut crate_insert = 0.0;
    let mut niao_get = 0.0;
    let mut crate_get = 0.0;
    for _ in 0..3 {
        niao_insert += secs_insert_niao();
        crate_insert += secs_insert_crate();

        let mut nm = NiaoMap::with_capacity_and_hasher(N, FxBuildHasher);
        let mut cm = indexmap::IndexMap::with_capacity(N);
        for i in 0..N {
            nm.insert(i, i);
            cm.insert(i, i);
        }
        let t = Instant::now();
        let mut s = 0usize;
        for i in 0..N {
            s = s.wrapping_add(*nm.get(&i).unwrap());
        }
        niao_get += t.elapsed().as_secs_f64();
        std::hint::black_box(s);

        let t = Instant::now();
        let mut s = 0usize;
        for i in 0..N {
            s = s.wrapping_add(*cm.get(&i).unwrap());
        }
        crate_get += t.elapsed().as_secs_f64();
        std::hint::black_box(s);
    }

    let insert_ratio = crate_insert / niao_insert;
    let get_ratio = crate_get / niao_get;
    eprintln!(
        "compare_perf N={N}: insert niao={niao_insert:.4}s crate={crate_insert:.4}s ratio={insert_ratio:.2}x; \
         get niao={niao_get:.4}s crate={crate_get:.4}s ratio={get_ratio:.2}x"
    );
    // Allow 5% noise; target is >= indexmap (ratio >= 1.0 means niao faster / equal time).
    assert!(
        insert_ratio >= 0.95,
        "insert slower than indexmap: niao={niao_insert} crate={crate_insert}"
    );
    assert!(
        get_ratio >= 0.95,
        "get slower than indexmap: niao={niao_get} crate={crate_get}"
    );
}
