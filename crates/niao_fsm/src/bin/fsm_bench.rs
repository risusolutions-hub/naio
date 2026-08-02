//! Native micro-benchmark for niao_fsm hot paths.

use niao_fsm::{FsmEngine, FsmSpec, StateDef, TransitionDef, TransitionDest, TransitionSources};
use std::time::Instant;

fn build_ring(n: usize) -> FsmSpec {
    let states: Vec<StateDef> = (0..n)
        .map(|i| StateDef {
            name: format!("s{i}"),
            parent: None,
            initial_child: None,
            is_history: false,
            is_final: i == n - 1,
        })
        .collect();
    let triggers = vec!["step".into()];
    let transitions: Vec<TransitionDef> = (0..n)
        .map(|i| TransitionDef {
            trigger: 0,
            sources: TransitionSources::One(i as u32),
            dest: TransitionDest::State(((i + 1) % n) as u32),
            priority: 0,
        })
        .collect();
    FsmSpec::build(states, triggers, transitions, "s0").unwrap()
}

fn bench_send(n_states: usize, steps: usize) -> f64 {
    let spec = build_ring(n_states);
    let mut m = FsmEngine::new(spec).unwrap();
    let t0 = Instant::now();
    for _ in 0..steps {
        let cands = m.candidates(0).unwrap();
        m.apply(cands[0].index).unwrap();
    }
    t0.elapsed().as_secs_f64() * 1000.0
}

fn bench_triggers(n_states: usize, queries: usize) -> f64 {
    let spec = build_ring(n_states);
    let m = FsmEngine::new(spec).unwrap();
    let t0 = Instant::now();
    for _ in 0..queries {
        let _ = m.available_triggers();
        let _ = m.candidates(0);
    }
    t0.elapsed().as_secs_f64() * 1000.0
}

fn bench_hierarchical(depth: usize, steps: usize) -> f64 {
    let mut states = Vec::new();
    for i in 0..depth {
        states.push(StateDef {
            name: format!("c{i}"),
            parent: if i == 0 { None } else { Some((i - 1) as u32) },
            initial_child: if i + 1 < depth {
                Some((i + 1) as u32)
            } else {
                None
            },
            is_history: false,
            is_final: false,
        });
    }
    let leaf = (depth - 1) as u32;
    states.push(StateDef {
        name: "out".into(),
        parent: None,
        initial_child: None,
        is_history: false,
        is_final: false,
    });
    let out_id = states.len() as u32 - 1;
    let spec = FsmSpec::build(
        states,
        vec!["leave".into(), "enter".into()],
        vec![
            TransitionDef {
                trigger: 0,
                sources: TransitionSources::One(leaf),
                dest: TransitionDest::State(out_id),
                priority: 0,
            },
            TransitionDef {
                trigger: 1,
                sources: TransitionSources::One(out_id),
                dest: TransitionDest::State(0),
                priority: 0,
            },
        ],
        "c0",
    )
    .unwrap();
    let mut m = FsmEngine::new(spec).unwrap();
    let t0 = Instant::now();
    for _ in 0..steps {
        if m.available_triggers().contains(&0) {
            let c = m.candidates(0).unwrap();
            m.apply(c[0].index).unwrap();
        } else {
            let c = m.candidates(1).unwrap();
            m.apply(c[0].index).unwrap();
        }
    }
    t0.elapsed().as_secs_f64() * 1000.0
}

fn main() {
    let steps = 500_000;
    let n = 64;
    println!("niao_fsm bench");
    println!(
        "  send ring n={n} x{steps}: {:.2} ms ({:.0} ns/step)",
        bench_send(n, steps),
        bench_send(n, steps) * 1_000_000.0 / steps as f64
    );
    println!(
        "  triggers lookup n={n} x{steps}: {:.2} ms",
        bench_triggers(n, steps)
    );
    println!(
        "  hierarchical depth=8 x{steps}: {:.2} ms",
        bench_hierarchical(8, steps / 10)
    );
}
