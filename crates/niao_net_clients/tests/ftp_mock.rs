//! Integration tests against the in-crate FTP mock server.

use niao_net_clients::ftp::{connect, connect_with, mock::MockFtpServer, FtpOptions, TransferMode};

#[test]
fn mock_get_put_list() {
    let server = MockFtpServer::start();
    let port = server.port();
    let mut ftp = connect("127.0.0.1", port).unwrap();
    ftp.login("test", "test").unwrap();
    ftp.put("notes.txt", b"integration test payload").unwrap();
    let bytes = ftp.get("notes.txt").unwrap();
    assert_eq!(bytes, b"integration test payload");
    let listing = ftp.list(Some(".")).unwrap();
    assert!(listing.iter().any(|l| l.contains("notes.txt")));
    ftp.quit().unwrap();
    server.shutdown();
}

#[test]
fn mock_active_mode_stor_retr() {
    let server = MockFtpServer::start();
    let port = server.port();
    let mut ftp = connect_with(
        "127.0.0.1",
        port,
        FtpOptions {
            mode: TransferMode::Active,
            ..Default::default()
        },
    )
    .unwrap();
    ftp.login("a", "b").unwrap();
    ftp.put("a.bin", &[9, 8, 7]).unwrap();
    assert_eq!(ftp.get("a.bin").unwrap(), vec![9, 8, 7]);
    server.shutdown();
}
