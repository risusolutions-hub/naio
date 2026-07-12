//! Micro-bench: groupby + join. Paths from NFRAME_LEFT_CSV / NFRAME_RIGHT_CSV.
use niao_frame::{join, read_csv, AggOp, CsvOptions, GroupBy, JoinHow};
use std::env;
use std::time::Instant;

fn main() {
    let left_path = env::var("NFRAME_LEFT_CSV").expect("NFRAME_LEFT_CSV");
    let right_path = env::var("NFRAME_RIGHT_CSV").expect("NFRAME_RIGHT_CSV");
    let left = read_csv(&left_path, CsvOptions::with_header()).unwrap();
    let right = read_csv(&right_path, CsvOptions::with_header()).unwrap();

    let t0 = Instant::now();
    let mut gb = GroupBy::new(&left, &["k"]).unwrap();
    let _ = gb
        .agg(&[("v", AggOp::Sum), ("v", AggOp::Mean)])
        .unwrap();
    let gb_ms = t0.elapsed().as_secs_f64() * 1000.0;

    let t1 = Instant::now();
    let _ = join(&left, &right, &["k"], JoinHow::Inner).unwrap();
    let join_ms = t1.elapsed().as_secs_f64() * 1000.0;

    println!("{gb_ms:.3} {join_ms:.3}");
}
