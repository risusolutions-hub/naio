//! Micro-benchmark for niao_expr hot paths.
use niao_expr::{parse, Evaluator, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

fn main() {
    let source = "price * qty * (1 + tax_rate) + shipping if qty > 0 else 0";
    let compiled = parse(source).unwrap();
    let mut ev = Evaluator::new();
    ev.set_var("price", Value::Float(19.995));
    ev.set_var("qty", Value::Int(3));
    ev.set_var("tax_rate", Value::Float(0.0825));
    ev.set_var("shipping", Value::Float(4.99));

    let n = 500_000usize;
    let t0 = Instant::now();
    for _ in 0..n {
        let _ = ev.run(&compiled).unwrap();
    }
    let elapsed = t0.elapsed();
    let per_ns = elapsed.as_nanos() as f64 / n as f64;
    println!(
        "eval compiled: {n} runs in {:.2} ms ({:.0} ns/op)",
        elapsed.as_secs_f64() * 1000.0,
        per_ns
    );

    let rows: Vec<HashMap<Arc<str>, Value>> = (0..50_000)
        .map(|i| {
            let mut m = HashMap::new();
            m.insert(Arc::from("price"), Value::Float(10.0 + i as f64));
            m.insert(Arc::from("qty"), Value::Int((i % 5) + 1));
            m.insert(Arc::from("tax_rate"), Value::Float(0.08));
            m.insert(Arc::from("shipping"), Value::Float(2.5));
            m
        })
        .collect();
    let t1 = Instant::now();
    let out = ev.batch(&compiled, &rows, 0);
    let batch_elapsed = t1.elapsed();
    println!(
        "batch parallel: {} rows in {:.2} ms ({:.0} µs/row)",
        out.len(),
        batch_elapsed.as_secs_f64() * 1000.0,
        batch_elapsed.as_nanos() as f64 / out.len() as f64 / 1000.0
    );
    println!("sample result: {:?}", out[0].as_ref().ok());
}
