//! Micro-benchmarks for nproto hot paths.
//! Run: cargo run -p niao_proto --bin nproto_bench --release

use niao_proto::{compile_source, decode_raw, ProtoMessage};
use std::time::Instant;

const PERSON_PROTO: &str = r#"
syntax = "proto3";
package bench;

message Person {
  string name = 1;
  int32 age = 2;
  repeated string tags = 3;
  map<string, int32> scores = 4;
}

message Team {
  repeated Person members = 1;
  string name = 2;
}
"#;

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
    let mean = samples.iter().sum::<u64>() / iters as u64;
    let p50 = samples[samples.len() / 2];
    println!("{name}: mean={mean} ns p50={p50} ns (n={iters})");
}

fn main() {
    let schema = compile_source("bench.proto", PERSON_PROTO, &[]).expect("compile");
    let mut template = ProtoMessage::new(&schema, "bench.Person").expect("new");
    template
        .set_field_json("name", &serde_json::json!("benchmark-user"))
        .unwrap();
    template
        .set_field_json("age", &serde_json::json!(30))
        .unwrap();
    template
        .set_field_json(
            "tags",
            &serde_json::json!(["alpha", "beta", "gamma", "delta"]),
        )
        .unwrap();
    template
        .set_field_json("scores", &serde_json::json!({"math": 95, "code": 99}))
        .unwrap();
    let encoded = template.encode().expect("encode");
    println!("Person payload size: {} bytes", encoded.len());

    let warmup = 3u32;
    let iters = 20u32;

    bench(
        "compile schema",
        || {
            compile_source("bench.proto", PERSON_PROTO, &[])
                .map(|_| 1)
                .unwrap_or(0)
        },
        warmup,
        iters,
    );

    bench(
        "encode Person",
        || template.encode().map(|b| b.len()).unwrap_or(0),
        warmup,
        iters,
    );

    bench(
        "decode Person",
        || {
            ProtoMessage::decode(&schema, "bench.Person", &encoded)
                .map(|_| 1)
                .unwrap_or(0)
        },
        warmup,
        iters,
    );

    bench(
        "encode+decode roundtrip",
        || {
            let bytes = template.encode().unwrap();
            ProtoMessage::decode(&schema, "bench.Person", &bytes)
                .map(|_| bytes.len())
                .unwrap_or(0)
        },
        warmup,
        iters,
    );

    bench(
        "to_json Person",
        || template.to_json(false).map(|s| s.len()).unwrap_or(0),
        warmup,
        iters,
    );

    bench(
        "from_json Person",
        || {
            let text = template.to_json(false).unwrap();
            ProtoMessage::from_json(&schema, "bench.Person", &text)
                .map(|_| 1)
                .unwrap_or(0)
        },
        warmup,
        iters,
    );

    bench(
        "decode_raw wire",
        || decode_raw(&encoded).map(|f| f.len()).unwrap_or(0),
        warmup,
        iters,
    );

    // Nested repeated messages
    let mut team = ProtoMessage::new(&schema, "bench.Team").expect("team");
    team.set_field_json("name", &serde_json::json!("crew"))
        .unwrap();
    let members: Vec<serde_json::Value> = (0..100)
        .map(|i| {
            serde_json::json!({
                "name": format!("user{i}"),
                "age": 20 + (i % 40),
                "tags": ["worker"],
                "scores": {"k": i}
            })
        })
        .collect();
    team.set_field_json("members", &serde_json::Value::Array(members))
        .unwrap();
    let team_bytes = team.encode().expect("team encode");
    println!("Team(100 members) payload size: {} bytes", team_bytes.len());

    bench(
        "encode Team x100",
        || team.encode().map(|b| b.len()).unwrap_or(0),
        warmup,
        iters,
    );

    bench(
        "decode Team x100",
        || {
            ProtoMessage::decode(&schema, "bench.Team", &team_bytes)
                .map(|_| 1)
                .unwrap_or(0)
        },
        warmup,
        iters,
    );
}
