//! Standalone micro-benchmark for niter combinatorics kernels (std-only).
//! Run: cargo run -p niao_runtime --example niter_micro_bench
use std::time::Instant;

fn cartesian_count(pools: &[usize]) -> usize {
    pools.iter().product()
}

fn cartesian_product_indices(pools: &[usize]) -> Vec<Vec<usize>> {
    let total: usize = pools.iter().product();
    let mut out = Vec::with_capacity(total);
    let mut idx = vec![0usize; pools.len()];
    loop {
        out.push(idx.clone());
        let mut carry = true;
        for i in (0..pools.len()).rev() {
            if carry {
                idx[i] += 1;
                if idx[i] < pools[i] {
                    carry = false;
                } else {
                    idx[i] = 0;
                }
            }
        }
        if carry {
            break;
        }
    }
    out
}

fn combinations(n: usize, r: usize) -> usize {
    if r > n {
        return 0;
    }
    let r = r.min(n - r);
    let mut num = 1usize;
    let mut den = 1usize;
    for i in 0..r {
        num *= n - r + 1 + i;
        den *= i + 1;
    }
    num / den
}

fn combination_rows(n: usize, r: usize) -> Vec<Vec<usize>> {
    let count = combinations(n, r);
    let mut out = Vec::with_capacity(count);
    if r == 0 {
        return vec![vec![]];
    }
    let mut idx: Vec<usize> = (0..r).collect();
    loop {
        out.push(idx.clone());
        let mut i = r;
        while i > 0 {
            i -= 1;
            if idx[i] != i + n - r {
                idx[i] += 1;
                for j in i + 1..r {
                    idx[j] = idx[j - 1] + 1;
                }
                break;
            }
        }
        if i == 0 && idx[0] == n - r {
            break;
        }
    }
    out
}

fn permutations(n: usize, r: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    let mut used = vec![false; n];
    let mut stack = Vec::with_capacity(r);
    fn visit(
        n: usize,
        r: usize,
        used: &mut [bool],
        stack: &mut Vec<usize>,
        out: &mut Vec<Vec<usize>>,
    ) {
        if stack.len() == r {
            out.push(stack.clone());
            return;
        }
        for i in 0..n {
            if !used[i] {
                used[i] = true;
                stack.push(i);
                visit(n, r, used, stack, out);
                stack.pop();
                used[i] = false;
            }
        }
    }
    visit(n, r, &mut used, &mut stack, &mut out);
    out
}

fn windows(n: usize, w: usize) -> usize {
    if n < w {
        0
    } else {
        n - w + 1
    }
}

fn bench(name: &str, f: impl FnOnce()) {
    // warmup
    f();
    let start = Instant::now();
    f();
    println!("{name}: {}µs", start.elapsed().as_micros());
}

fn main() {
    bench("combinations C(20,3)", || {
        let rows = combination_rows(20, 3);
        assert_eq!(rows.len(), 1140);
    });
    bench("permutations P(9,4)", || {
        let rows = permutations(9, 4);
        assert_eq!(rows.len(), 3024);
    });
    bench("product 50x50", || {
        let rows = cartesian_product_indices(&[50, 50]);
        assert_eq!(rows.len(), 2500);
    });
    bench("windows 10k w=5", || {
        let count = windows(10_000, 5);
        assert_eq!(count, 9996);
        // materialize windows
        let mut out = Vec::with_capacity(count);
        for start in 0..=10_000 - 5 {
            out.push((start..start + 5).collect::<Vec<_>>());
        }
        assert_eq!(out.len(), 9996);
    });
    bench("zip_longest 5k/3k", || {
        let mut out = Vec::with_capacity(5000);
        for i in 0..5000 {
            out.push((i, if i < 3000 { i } else { 0 }));
        }
        assert_eq!(out.len(), 5000);
    });
    bench("flatten 500x10", || {
        let outer: Vec<Vec<i32>> = (0..500)
            .map(|i| (0..10).map(|j| i * 10 + j).collect())
            .collect();
        let flat: Vec<i32> = outer.into_iter().flatten().collect();
        assert_eq!(flat.len(), 5000);
    });
}
