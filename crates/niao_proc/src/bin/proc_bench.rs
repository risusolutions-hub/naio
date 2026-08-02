//! Micro-benchmarks for niao_proc hot paths.
//! Run: cargo run -p niao_proc --bin proc_bench --release

use niao_proc::{Channel, OsPipe, ProcessPool, SharedMemory, SpawnOpts};
use std::time::Instant;

fn bench<F: FnMut() -> usize>(name: &str, mut f: F, iters: u32) {
    for _ in 0..2 {
        let _ = f();
    }
    let start = Instant::now();
    let mut n = 0usize;
    for _ in 0..iters {
        n = f();
    }
    let elapsed = start.elapsed();
    let mean_ns = elapsed.as_nanos() / iters as u128;
    println!("{name}: n={n} iters={iters} mean={mean_ns} ns total={elapsed:?}");
}

fn main() {
    bench(
        "channel ping 10k",
        || {
            let ch = Channel::bounded(1024);
            for i in 0..10_000i64 {
                ch.send(i).unwrap();
                let _ = ch.recv(None).unwrap();
            }
            10_000
        },
        5,
    );

    bench(
        "pipe write/read 5k",
        || {
            let mut pipe = OsPipe::new().unwrap();
            let payload = b"x";
            for _ in 0..5_000 {
                pipe.writer.write_all(payload).unwrap();
                let mut buf = [0u8; 4];
                let _ = pipe.reader.read(&mut buf).unwrap();
            }
            pipe.writer.close();
            5_000
        },
        8,
    );

    bench(
        "shm write/read 20k",
        || {
            let name = format!("bench_{}", std::process::id());
            let mut shm = SharedMemory::create(&name, 4096).unwrap();
            let data = b"abcdefghij";
            for _ in 0..20_000 {
                shm.write(0, data).unwrap();
                let _ = shm.read(0, 10).unwrap();
            }
            let _ = SharedMemory::unlink(&name);
            20_000
        },
        8,
    );

    bench(
        "spawn/wait x20",
        || {
            let opts = SpawnOpts {
                stdout_pipe: true,
                stderr_pipe: true,
                ..Default::default()
            };
            let program = if cfg!(windows) { "niao.exe" } else { "niao" };
            for _ in 0..20 {
                let mut child =
                    niao_proc::ChildProcess::spawn(program, &["--version".to_string()], &opts)
                        .unwrap();
                let _ = child.wait(None).unwrap();
            }
            20
        },
        3,
    );

    bench(
        "pool_map 40 cmds w=4",
        || {
            let pool = ProcessPool::new(4);
            let program = if cfg!(windows) { "niao.exe" } else { "niao" };
            let commands: Vec<Vec<String>> = (0..40)
                .map(|_| vec![program.to_string(), "--version".to_string()])
                .collect();
            let opts = SpawnOpts {
                stdout_pipe: true,
                stderr_pipe: true,
                ..Default::default()
            };
            let results = pool.map(&commands, &opts).unwrap();
            results.len()
        },
        5,
    );
}
