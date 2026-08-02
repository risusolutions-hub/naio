use niao_flock::{
    break_stale, lock as flock_lock, pid_alive, write_pid, LockHandle, LockMode, LockOptions,
    PidFile, PidOptions,
};
use std::fs;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

fn temp(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("niao_flock_it_{name}"))
}

#[test]
fn lock_acquire_release_roundtrip() {
    let path = temp("roundtrip.lock");
    let _ = fs::remove_file(&path);
    let opts = LockOptions::default();
    let mut h = flock_lock(&path, &opts).unwrap();
    assert!(h.is_locked());
    h.release().unwrap();
    assert!(!h.is_locked());
    let _ = fs::remove_file(path);
}

#[test]
fn try_acquire_contention() {
    let path = temp("contend.lock");
    let _ = fs::remove_file(&path);
    let opts = LockOptions::default();
    let (tx, rx) = mpsc::channel();
    let path2 = path.clone();
    let opts2 = opts.clone();
    let holder = thread::spawn(move || {
        let mut a = flock_lock(&path2, &opts2).unwrap();
        tx.send(()).unwrap();
        thread::sleep(Duration::from_millis(200));
        a.release().unwrap();
    });
    rx.recv().unwrap();
    let mut b = match LockHandle::open(&path, &opts) {
        Ok(h) => h,
        Err(_) => {
            holder.join().unwrap();
            let mut h = LockHandle::open(&path, &opts).unwrap();
            assert!(h.try_acquire(LockMode::Exclusive).unwrap());
            let _ = fs::remove_file(path);
            return;
        }
    };
    assert!(!b.try_acquire(LockMode::Exclusive).unwrap());
    holder.join().unwrap();
    assert!(b.try_acquire(LockMode::Exclusive).unwrap());
    let _ = fs::remove_file(path);
}

#[test]
fn shared_locks_coexist() {
    let path = temp("shared.lock");
    let _ = fs::remove_file(&path);
    let opts = LockOptions {
        mode: LockMode::Shared,
        ..LockOptions::default()
    };
    let mut a = LockHandle::open(&path, &opts).unwrap();
    let mut b = LockHandle::open(&path, &opts).unwrap();
    assert!(a.try_acquire(LockMode::Shared).unwrap());
    assert!(b.try_acquire(LockMode::Shared).unwrap());
    let _ = fs::remove_file(path);
}

#[test]
fn timeout_returns_error() {
    let path = temp("timeout.lock");
    let _ = fs::remove_file(&path);
    let (tx, rx) = mpsc::channel();
    let path2 = path.clone();
    let holder = thread::spawn(move || {
        let _h = flock_lock(
            &path2,
            &LockOptions {
                timeout: None,
                ..LockOptions::default()
            },
        )
        .unwrap();
        tx.send(()).unwrap();
        thread::sleep(Duration::from_millis(300));
    });
    rx.recv().unwrap();
    match flock_lock(
        &path,
        &LockOptions {
            timeout: Some(Duration::from_millis(50)),
            poll_interval: Duration::from_millis(5),
            ..LockOptions::default()
        },
    ) {
        Err(e) => {
            let is_timeout = matches!(e, niao_flock::FlockError::Timeout { .. })
                || matches!(
                    e,
                    niao_flock::FlockError::Io(ref io)
                        if io.kind() == std::io::ErrorKind::TimedOut
                );
            assert!(is_timeout);
        }
        Ok(_) => panic!("expected timeout"),
    }
    holder.join().unwrap();
    let _ = fs::remove_file(path);
}

#[test]
fn pid_file_acquire_and_read() {
    let path = temp("daemon.pid");
    let _ = fs::remove_file(&path);
    let pf = PidFile::acquire(&path, &PidOptions::default()).unwrap();
    assert!(pf.pid > 0);
    assert!(pid_alive(pf.pid));
    pf.release().unwrap();
}

#[test]
fn break_stale_removes_dead_pid() {
    let path = temp("stale.lock");
    let _ = fs::remove_file(&path);
    write_pid(&path, Some(999_999_999)).unwrap();
    assert!(break_stale(&path, false).unwrap());
    assert!(!path.exists());
}

#[test]
fn flock_constants_match_python() {
    assert_eq!(niao_flock::LOCK_SH, 1);
    assert_eq!(niao_flock::LOCK_EX, 2);
    assert_eq!(niao_flock::LOCK_NB, 4);
    assert_eq!(niao_flock::LOCK_UN, 8);
}
