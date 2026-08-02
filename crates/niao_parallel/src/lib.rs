//! `niao_parallel` — zero-dependency data-parallelism primitives.
//!
//! Replaces the subset of `rayon` used inside the Niao runtime: scoped parallel
//! `map` / `zip_map` / chunked map-reduce over slices, a `try_map` that
//! short-circuits on error, and a lightweight configurable [`ThreadPool`].
//! Everything is built on `std::thread::scope`, so worker closures may borrow
//! their input slices without `'static` bounds. No third-party crates.
//!
//! Ordering guarantee: `map`, `zip_map`, and `try_map` return results in the
//! same order as the input. Reductions combine partial results left-to-right in
//! input order, so a non-associative `reduce` still behaves deterministically.

use std::thread;

/// Logical CPU count (minimum 1) — the default degree of parallelism.
pub fn available_threads() -> usize {
    thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

/// Clamp a requested thread count so it is at least 1 and never exceeds the
/// number of items (no point spawning idle workers).
#[inline]
fn effective_threads(requested: usize, len: usize) -> usize {
    requested.max(1).min(len.max(1))
}

#[inline]
fn span_len(len: usize, threads: usize) -> usize {
    (len + threads - 1) / threads
}

/// Parallel map preserving input order: `[f(&x) for x in data]`.
pub fn map<T, R, F>(data: &[T], threads: usize, f: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync,
{
    let len = data.len();
    if len == 0 {
        return Vec::new();
    }
    let nt = effective_threads(threads, len);
    if nt == 1 {
        return data.iter().map(|x| f(x)).collect();
    }
    let chunk = span_len(len, nt);
    let f = &f;
    let mut parts: Vec<Vec<R>> = Vec::with_capacity(nt);
    thread::scope(|scope| {
        let mut handles = Vec::with_capacity(nt);
        for c in data.chunks(chunk) {
            handles.push(scope.spawn(move || c.iter().map(|x| f(x)).collect::<Vec<R>>()));
        }
        for h in handles {
            parts.push(h.join().expect("niao_parallel: worker panicked"));
        }
    });
    let mut out = Vec::with_capacity(len);
    for p in parts {
        out.extend(p);
    }
    out
}

/// Parallel element-wise zip-map over the common prefix of `a` and `b`.
pub fn zip_map<T, U, R, F>(a: &[T], b: &[U], threads: usize, f: F) -> Vec<R>
where
    T: Sync,
    U: Sync,
    R: Send,
    F: Fn(&T, &U) -> R + Sync,
{
    let len = a.len().min(b.len());
    if len == 0 {
        return Vec::new();
    }
    let nt = effective_threads(threads, len);
    if nt == 1 {
        return a[..len]
            .iter()
            .zip(&b[..len])
            .map(|(x, y)| f(x, y))
            .collect();
    }
    let chunk = span_len(len, nt);
    let f = &f;
    let mut parts: Vec<Vec<R>> = Vec::with_capacity(nt);
    thread::scope(|scope| {
        let mut handles = Vec::with_capacity(nt);
        let mut start = 0;
        while start < len {
            let end = (start + chunk).min(len);
            let sa = &a[start..end];
            let sb = &b[start..end];
            handles.push(scope.spawn(move || {
                sa.iter()
                    .zip(sb.iter())
                    .map(|(x, y)| f(x, y))
                    .collect::<Vec<R>>()
            }));
            start = end;
        }
        for h in handles {
            parts.push(h.join().expect("niao_parallel: worker panicked"));
        }
    });
    let mut out = Vec::with_capacity(len);
    for p in parts {
        out.extend(p);
    }
    out
}

/// Parallel map that short-circuits on the first `Err`. Order preserved for the
/// `Ok` case; on error, some other partition's error may be returned (any error
/// is reported, not necessarily the earliest by index).
pub fn try_map<T, R, E, F>(data: &[T], threads: usize, f: F) -> Result<Vec<R>, E>
where
    T: Sync,
    R: Send,
    E: Send,
    F: Fn(&T) -> Result<R, E> + Sync,
{
    let len = data.len();
    if len == 0 {
        return Ok(Vec::new());
    }
    let nt = effective_threads(threads, len);
    if nt == 1 {
        return data.iter().map(|x| f(x)).collect();
    }
    let chunk = span_len(len, nt);
    let f = &f;
    let mut parts: Vec<Result<Vec<R>, E>> = Vec::with_capacity(nt);
    thread::scope(|scope| {
        let mut handles = Vec::with_capacity(nt);
        for c in data.chunks(chunk) {
            handles
                .push(scope.spawn(move || c.iter().map(|x| f(x)).collect::<Result<Vec<R>, E>>()));
        }
        for h in handles {
            parts.push(h.join().expect("niao_parallel: worker panicked"));
        }
    });
    let mut out = Vec::with_capacity(len);
    for p in parts {
        out.extend(p?);
    }
    Ok(out)
}

/// Parallel chunked map-reduce: split `data` into contiguous spans (one per
/// worker), map each `chunk_size`-sized sub-slice with `map`, and combine with
/// `reduce` starting from `identity`.
pub fn chunks_map_reduce<T, R, M, Rd>(
    data: &[T],
    threads: usize,
    chunk_size: usize,
    identity: R,
    map: M,
    reduce: Rd,
) -> R
where
    T: Sync,
    R: Send + Clone,
    M: Fn(&[T]) -> R + Sync,
    Rd: Fn(R, R) -> R + Sync,
{
    let len = data.len();
    if len == 0 {
        return identity;
    }
    let nt = effective_threads(threads, len);
    let cs = chunk_size.max(1);
    if nt == 1 {
        let mut acc = identity;
        for c in data.chunks(cs) {
            acc = reduce(acc, map(c));
        }
        return acc;
    }
    let span = span_len(len, nt);
    let map = &map;
    let reduce = &reduce;
    let mut parts: Vec<R> = Vec::with_capacity(nt);
    thread::scope(|scope| {
        let mut handles = Vec::with_capacity(nt);
        let mut start = 0;
        while start < len {
            let end = (start + span).min(len);
            let s = &data[start..end];
            let id0 = identity.clone();
            handles.push(scope.spawn(move || {
                let mut acc = id0;
                for c in s.chunks(cs) {
                    acc = reduce(acc, map(c));
                }
                acc
            }));
            start = end;
        }
        for h in handles {
            parts.push(h.join().expect("niao_parallel: worker panicked"));
        }
    });
    let mut acc = identity;
    for p in parts {
        acc = reduce(acc, p);
    }
    acc
}

/// Parallel element-wise zip-reduce over the common prefix of `a` and `b`.
/// Each worker folds its span with `map` + `reduce`; partials are combined in
/// order. Used for dot-products and similar.
pub fn zip_reduce<T, U, R, M, Rd>(
    a: &[T],
    b: &[U],
    threads: usize,
    identity: R,
    map: M,
    reduce: Rd,
) -> R
where
    T: Sync,
    U: Sync,
    R: Send + Clone,
    M: Fn(&T, &U) -> R + Sync,
    Rd: Fn(R, R) -> R + Sync,
{
    let len = a.len().min(b.len());
    if len == 0 {
        return identity;
    }
    let nt = effective_threads(threads, len);
    if nt == 1 {
        let mut acc = identity;
        for (x, y) in a[..len].iter().zip(&b[..len]) {
            acc = reduce(acc, map(x, y));
        }
        return acc;
    }
    let span = span_len(len, nt);
    let map = &map;
    let reduce = &reduce;
    let mut parts: Vec<R> = Vec::with_capacity(nt);
    thread::scope(|scope| {
        let mut handles = Vec::with_capacity(nt);
        let mut start = 0;
        while start < len {
            let end = (start + span).min(len);
            let sa = &a[start..end];
            let sb = &b[start..end];
            let id0 = identity.clone();
            handles.push(scope.spawn(move || {
                let mut acc = id0;
                for (x, y) in sa.iter().zip(sb.iter()) {
                    acc = reduce(acc, map(x, y));
                }
                acc
            }));
            start = end;
        }
        for h in handles {
            parts.push(h.join().expect("niao_parallel: worker panicked"));
        }
    });
    let mut acc = identity;
    for p in parts {
        acc = reduce(acc, p);
    }
    acc
}

/// Parallel in-place mutation: applies `f(index, &mut item)` to every element,
/// splitting the slice into disjoint contiguous chunks across threads.
pub fn for_each_mut<T, F>(data: &mut [T], threads: usize, f: F)
where
    T: Send,
    F: Fn(usize, &mut T) + Sync,
{
    let len = data.len();
    if len == 0 {
        return;
    }
    let nt = effective_threads(threads, len);
    if nt == 1 {
        for (i, item) in data.iter_mut().enumerate() {
            f(i, item);
        }
        return;
    }
    let chunk = span_len(len, nt);
    let f = &f;
    thread::scope(|scope| {
        let mut base = 0usize;
        for c in data.chunks_mut(chunk) {
            let start = base;
            base += c.len();
            scope.spawn(move || {
                for (k, item) in c.iter_mut().enumerate() {
                    f(start + k, item);
                }
            });
        }
    });
}

/// A configurable, cheaply-cloned handle describing a degree of parallelism.
///
/// The Niao runtime uses this to remember a user-requested thread count and to
/// mirror the small slice of `rayon::ThreadPool` it relied on (`install` +
/// `current_num_threads`). Work is executed by the free functions above, which
/// take an explicit thread count; `install` simply runs the closure on the
/// calling thread.
#[derive(Clone, Copy, Debug)]
pub struct ThreadPool {
    threads: usize,
}

impl ThreadPool {
    /// Create a pool description with `threads` workers (clamped to >= 1).
    pub fn new(threads: usize) -> Self {
        Self {
            threads: threads.max(1),
        }
    }

    /// The configured worker count.
    pub fn current_num_threads(&self) -> usize {
        self.threads
    }

    /// Run `f` to completion. Provided for API parity with the previous
    /// `rayon::ThreadPool::install`; the closure runs on the calling thread and
    /// any parallelism comes from the free functions invoked inside it with
    /// `pool.current_num_threads()`.
    pub fn install<R, F: FnOnce() -> R>(&self, f: F) -> R {
        f()
    }
}

impl Default for ThreadPool {
    fn default() -> Self {
        Self::new(available_threads())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_preserves_order_all_thread_counts() {
        let data: Vec<i64> = (0..1000).collect();
        for nt in [1usize, 2, 3, 4, 7, 16, 64] {
            let out = map(&data, nt, |&x| x * 2);
            let expect: Vec<i64> = data.iter().map(|&x| x * 2).collect();
            assert_eq!(out, expect, "nt={nt}");
        }
    }

    #[test]
    fn map_empty() {
        let data: Vec<i64> = Vec::new();
        assert!(map(&data, 4, |&x| x).is_empty());
    }

    #[test]
    fn zip_map_add() {
        let a: Vec<i64> = (0..1000).collect();
        let b: Vec<i64> = (0..1000).map(|x| x * 10).collect();
        let out = zip_map(&a, &b, 8, |&x, &y| x + y);
        let expect: Vec<i64> = a.iter().zip(&b).map(|(&x, &y)| x + y).collect();
        assert_eq!(out, expect);
    }

    #[test]
    fn chunks_map_reduce_sum_matches_serial() {
        let data: Vec<i64> = (0..100_000).collect();
        let expect: i64 = data.iter().sum();
        for nt in [1usize, 2, 5, 33] {
            let got = chunks_map_reduce(
                &data,
                nt,
                4096,
                0i64,
                |c| c.iter().sum::<i64>(),
                |a, b| a + b,
            );
            assert_eq!(got, expect, "nt={nt}");
        }
    }

    #[test]
    fn zip_reduce_dot_matches_serial() {
        let a: Vec<f64> = (0..10_000).map(|x| x as f64).collect();
        let b: Vec<f64> = (0..10_000).map(|x| (x as f64) * 0.5).collect();
        let expect: f64 = a.iter().zip(&b).map(|(&x, &y)| x * y).sum();
        let got = zip_reduce(&a, &b, 8, 0.0f64, |&x, &y| x * y, |p, q| p + q);
        assert!((got - expect).abs() < 1e-6, "got={got} expect={expect}");
    }

    #[test]
    fn try_map_ok_and_err() {
        let data: Vec<i64> = (0..1000).collect();
        let ok = try_map(&data, 8, |&x| if x >= 0 { Ok(x + 1) } else { Err("neg") });
        assert_eq!(ok.unwrap().len(), 1000);

        let mixed: Vec<i64> = vec![1, 2, -3, 4];
        let err = try_map(&mixed, 4, |&x| if x >= 0 { Ok(x) } else { Err("neg") });
        assert!(err.is_err());
    }

    #[test]
    fn thread_pool_reports_count() {
        let p = ThreadPool::new(3);
        assert_eq!(p.current_num_threads(), 3);
        assert_eq!(p.install(|| 42), 42);
        assert!(ThreadPool::default().current_num_threads() >= 1);
        assert!(available_threads() >= 1);
    }
}
