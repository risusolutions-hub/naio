//! Release-mode micro-benchmarks for niao_view hot paths.
use niao_view::{batch_render, render, CompiledTemplate, EscapeMode, ViewEnv, ViewOpts};
use serde_json::json;
use std::time::Instant;

fn main() {
    let opts = ViewOpts {
        autoescape: EscapeMode::Html,
        ..Default::default()
    };
    let source =
        "<h1>{{ title }}</h1><ul>{% for u in users %}<li>{{ u.name|e }}</li>{% endfor %}</ul>";
    let ctx = json!({
        "title": "Bench",
        "users": [
            {"name": "Ada"},
            {"name": "Grace"},
            {"name": "Edsger"}
        ]
    });

    let compiled = CompiledTemplate::compile(source, &opts).unwrap();
    let warmup = 2_000usize;
    for _ in 0..warmup {
        let _ = compiled.render(&ctx).unwrap();
    }

    let n = 50_000usize;
    let t0 = Instant::now();
    for _ in 0..n {
        let _ = compiled.render(&ctx).unwrap();
    }
    let elapsed = t0.elapsed();
    let ns = elapsed.as_nanos() as f64 / n as f64;
    let ops = 1_000_000_000.0 / ns;
    println!(
        "compiled render: {n} runs in {:.2} ms ({:.0} ns/op, {:.0} ops/sec)",
        elapsed.as_secs_f64() * 1000.0,
        ns,
        ops
    );

    // Naive baseline: re-compile every time
    let t1 = Instant::now();
    for _ in 0..n {
        let _ = render(source, &ctx, &opts).unwrap();
    }
    let naive = t1.elapsed();
    let naive_ns = naive.as_nanos() as f64 / n as f64;
    println!(
        "naive recompile: {n} runs in {:.2} ms ({:.0} ns/op) — compiled is {:.1}x faster",
        naive.as_secs_f64() * 1000.0,
        naive_ns,
        naive_ns / ns
    );

    let mut env = ViewEnv::new(opts.clone());
    env.add("base.html", "<html>{% block body %}{% endblock %}</html>")
        .unwrap();
    env.add(
        "page.html",
        "{% extends \"base.html\" %}{% block body %}<p>{{ msg }}</p>{% endblock %}",
    )
    .unwrap();
    let inh_ctx = json!({"msg": "hi"});
    let t2 = Instant::now();
    for _ in 0..n {
        let _ = env.render_named("page.html", &inh_ctx).unwrap();
    }
    let inh = t2.elapsed();
    println!(
        "inheritance render: {n} runs in {:.2} ms ({:.0} ns/op)",
        inh.as_secs_f64() * 1000.0,
        inh.as_nanos() as f64 / n as f64
    );

    let rows: Vec<_> = (0..10_000)
        .map(|i| json!({"title": format!("T{i}"), "users": [{"name": "x"}]}))
        .collect();
    let t3 = Instant::now();
    let out = batch_render(source, &rows, &opts, 0).unwrap();
    let batch = t3.elapsed();
    let mb = out.iter().map(|s| s.len()).sum::<usize>() as f64 / (1024.0 * 1024.0);
    println!(
        "batch parallel: {} rows in {:.2} ms ({:.0} µs/row, {:.2} MB out)",
        out.len(),
        batch.as_secs_f64() * 1000.0,
        batch.as_nanos() as f64 / out.len() as f64 / 1000.0,
        mb
    );
}
