use crate::{spawn, tcp, Poller, TimerQueue};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[test]
fn executor_runs_jobs() {
    let done = Arc::new(AtomicU32::new(0));
    for _ in 0..100 {
        let d = Arc::clone(&done);
        spawn(move || {
            d.fetch_add(1, Ordering::Relaxed);
        });
    }
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(done.load(Ordering::Relaxed), 100);
}

#[test]
fn poller_create() {
    let _ = Poller::new().expect("poller");
}

#[test]
fn poller_idle_does_not_spin() {
    let mut poller = Poller::new().unwrap();
    let start = Instant::now();
    let _ = poller.poll(Some(50)).unwrap();
    assert!(start.elapsed() >= Duration::from_millis(40));
}

#[test]
fn timer_accuracy() {
    let mut q = TimerQueue::new();
    q.schedule(Duration::from_millis(25));
    let before = Instant::now();
    while q.pop_expired(Instant::now()).is_empty() {
        std::thread::sleep(Duration::from_millis(1));
    }
    let elapsed = before.elapsed();
    assert!(
        elapsed >= Duration::from_millis(20),
        "timer fired too early: {elapsed:?}"
    );
    assert!(
        elapsed <= Duration::from_millis(80),
        "timer fired too late: {elapsed:?}"
    );
}

#[test]
fn tcp_echo_stress() {
    #[cfg(unix)]
    const CONNS: u32 = 10_000;
    #[cfg(windows)]
    const CONNS: u32 = 500;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    let done = Arc::new(AtomicU32::new(0));
    let target = Arc::clone(&done);

    std::thread::spawn(move || {
        let mut buf = [0u8; 4];
        while target.load(Ordering::Relaxed) < CONNS {
            if let Ok((mut stream, _)) = listener.accept() {
                stream.read_exact(&mut buf).ok();
                stream.write_all(&buf).ok();
                target.fetch_add(1, Ordering::Relaxed);
            } else {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    });

    for _ in 0..CONNS {
        let mut stream = tcp::tcp_connect(&addr.to_string(), Duration::from_secs(5)).unwrap();
        stream.write_all(b"ping").unwrap();
        let mut buf = [0u8; 4];
        stream.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"ping");
    }
    assert_eq!(done.load(Ordering::Relaxed), CONNS);
}
