//! Integration tests for niao_ssh (embedded server).

use niao_ssh::*;
use rand_core::OsRng;
use russh::keys::PrivateKey;

#[test]
fn key_fingerprint_roundtrip() {
    let key = PrivateKey::random(&mut OsRng, russh::keys::Algorithm::Ed25519).unwrap();
    let pem = key
        .to_openssh(russh::keys::ssh_key::LineEnding::LF)
        .unwrap()
        .to_string();
    let fp = key_fingerprint(&pem, false, None).unwrap();
    assert!(fp.starts_with("SHA256:"), "{fp}");
}

#[test]
fn connect_missing_auth() {
    let cfg = ConnectConfig::new("127.0.0.1", "u");
    let err = connect(&cfg).unwrap_err();
    assert!(err.to_string().contains("authentication"));
}

#[test]
fn connect_refused() {
    let mut cfg = ConnectConfig::new("127.0.0.1", "u");
    cfg.port = 1;
    cfg.password = Some("x".into());
    cfg.timeout_ms = Some(300);
    assert!(connect(&cfg).is_err());
}

#[test]
fn invalid_handles() {
    assert!(matches!(
        exec(999_999, "true", None),
        Err(SshError::InvalidHandle(_))
    ));
    assert!(matches!(
        sftp_open(999_999),
        Err(SshError::InvalidHandle(_))
    ));
    assert!(!is_connected(999_999));
}

#[test]
fn password_exec_echo() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let server = rt.block_on(niao_ssh::testutil_start());
    let mut cfg = ConnectConfig::new(server.host(), "testuser");
    cfg.port = server.port();
    cfg.password = Some("testpass".into());
    cfg.timeout_ms = Some(5_000);
    let s = connect(&cfg).unwrap();
    assert!(is_connected(s));
    let r = exec(s, "echo hello-nssh", Some(5_000)).unwrap();
    assert!(r.ok);
    assert_eq!(String::from_utf8_lossy(&r.stdout).trim(), "hello-nssh");
    let r2 = exec(s, "exit 7", Some(5_000)).unwrap();
    assert_eq!(r2.exit_status, 7);
    assert!(!r2.ok);
    let r3 = exec(s, "stderr-msg", Some(5_000)).unwrap();
    assert!(String::from_utf8_lossy(&r3.stderr).contains("err-line"));
    close(s).unwrap();
    assert!(!is_connected(s));
}

#[test]
fn key_auth_exec() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let server = rt.block_on(niao_ssh::testutil_start());
    let pem = server
        .client_key
        .to_openssh(russh::keys::ssh_key::LineEnding::LF)
        .unwrap()
        .to_string();
    let mut cfg = ConnectConfig::new(server.host(), "testuser");
    cfg.port = server.port();
    cfg.key_data = Some(pem);
    cfg.timeout_ms = Some(5_000);
    let s = connect(&cfg).unwrap();
    let r = exec(s, "echo keyed", Some(5_000)).unwrap();
    assert_eq!(String::from_utf8_lossy(&r.stdout).trim(), "keyed");
    close(s).unwrap();
}

#[test]
fn shell_echo() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let server = rt.block_on(niao_ssh::testutil_start());
    let mut cfg = ConnectConfig::new(server.host(), "testuser");
    cfg.port = server.port();
    cfg.password = Some("testpass".into());
    let s = connect(&cfg).unwrap();
    let ch = shell_open(s, "xterm", 80, 24).unwrap();
    let _ = shell_read(ch, Some(500), 4096).unwrap(); // banner "$ "
    shell_write(ch, b"ping").unwrap();
    let data = shell_read(ch, Some(2_000), 4096)
        .unwrap()
        .unwrap_or_default();
    assert!(String::from_utf8_lossy(&data).contains("ping"));
    shell_close(ch).unwrap();
    close(s).unwrap();
}

#[test]
fn sftp_roundtrip() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let server = rt.block_on(niao_ssh::testutil_start());
    let mut cfg = ConnectConfig::new(server.host(), "testuser");
    cfg.port = server.port();
    cfg.password = Some("testpass".into());
    let s = connect(&cfg).unwrap();
    let sf = sftp_open(s).unwrap();
    sftp_mkdir(sf, "/mydir").unwrap();
    sftp_write(sf, "/mydir/a.txt", b"hello").unwrap();
    sftp_write(sf, "/empty", b"").unwrap();
    let big = vec![7u8; 1024 * 256];
    sftp_write(sf, "/big.bin", &big).unwrap();
    assert_eq!(sftp_read(sf, "/mydir/a.txt").unwrap(), b"hello");
    assert_eq!(sftp_read(sf, "/empty").unwrap(), b"");
    assert_eq!(sftp_read(sf, "/big.bin").unwrap(), big);
    let st = sftp_stat(sf, "/mydir/a.txt").unwrap();
    assert_eq!(st.size, 5);
    assert!(st.is_file);
    sftp_rename(sf, "/mydir/a.txt", "/mydir/b.txt").unwrap();
    assert_eq!(sftp_read(sf, "/mydir/b.txt").unwrap(), b"hello");
    let listing = sftp_listdir(sf, "/mydir").unwrap();
    assert!(listing.iter().any(|e| e.name == "b.txt"));
    let dir = std::env::temp_dir();
    let local_up = dir.join("nssh_up.txt");
    let local_dn = dir.join("nssh_dn.txt");
    std::fs::write(&local_up, b"put-me").unwrap();
    sftp_put(sf, local_up.to_str().unwrap(), "/mydir/c.txt").unwrap();
    sftp_get(sf, "/mydir/c.txt", local_dn.to_str().unwrap()).unwrap();
    assert_eq!(std::fs::read(&local_dn).unwrap(), b"put-me");
    sftp_remove(sf, "/mydir/c.txt").unwrap();
    sftp_remove(sf, "/mydir/b.txt").unwrap();
    sftp_rmdir(sf, "/mydir").unwrap();
    sftp_close(sf).unwrap();
    close(s).unwrap();
    let _ = std::fs::remove_file(local_up);
    let _ = std::fs::remove_file(local_dn);
}

#[test]
fn forward_local_tcp() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    // Echo backend
    let backend = rt.block_on(async {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = l.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let mut buf = [0u8; 256];
                    if let Ok(n) = tokio::io::AsyncReadExt::read(&mut s, &mut buf).await {
                        let _ = tokio::io::AsyncWriteExt::write_all(&mut s, &buf[..n]).await;
                    }
                });
            }
        });
        addr
    });
    let server = rt.block_on(niao_ssh::testutil_start());
    let mut cfg = ConnectConfig::new(server.host(), "testuser");
    cfg.port = server.port();
    cfg.password = Some("testpass".into());
    let s = connect(&cfg).unwrap();
    let fw = forward_local(s, 0, "127.0.0.1", backend.port()).unwrap();
    let msg = b"tunnel-hi";
    let mut stream = std::net::TcpStream::connect(format!("127.0.0.1:{}", fw.bind_port)).unwrap();
    use std::io::{Read, Write};
    stream.write_all(msg).unwrap();
    let mut buf = [0u8; 32];
    let n = stream.read(&mut buf).unwrap();
    assert_eq!(&buf[..n], msg);
    forward_close(fw.id).unwrap();
    close(s).unwrap();
}

#[test]
fn agent_identities_does_not_panic() {
    // May Err if no agent — that is fine.
    let _ = agent_identities();
}

#[test]
fn nssh_live_bench_release_numbers() {
    // Measured when run under --release; still valid in debug (just slower).
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let server = rt.block_on(niao_ssh::testutil_start());
    let mut cfg = ConnectConfig::new(server.host(), "testuser");
    cfg.port = server.port();
    cfg.password = Some("testpass".into());
    let iters = 15usize;
    let t0 = std::time::Instant::now();
    for _ in 0..iters {
        let s = connect(&cfg).unwrap();
        let r = exec(s, "echo bench", Some(5_000)).unwrap();
        assert!(r.ok);
        close(s).unwrap();
    }
    let ns = t0.elapsed().as_nanos() as f64 / iters as f64;
    let ops = 1e9 / ns;
    eprintln!("LIVE connect+exec: {ops:.2} ops/sec, {ns:.0} ns/op");

    let s = connect(&cfg).unwrap();
    let sf = sftp_open(s).unwrap();
    let payload = vec![0x5Au8; 64 * 1024];
    let iters = 12usize;
    let t0 = std::time::Instant::now();
    for _ in 0..iters {
        sftp_write(sf, "/bench.bin", &payload).unwrap();
        let back = sftp_read(sf, "/bench.bin").unwrap();
        assert_eq!(back.len(), payload.len());
    }
    let secs = t0.elapsed().as_secs_f64();
    let mbs = (payload.len() * 2 * iters) as f64 / secs / (1024.0 * 1024.0);
    eprintln!("LIVE sftp 64KiB write+read: {mbs:.2} MB/s");
    let _ = sftp_close(sf);
    close(s).unwrap();
}
