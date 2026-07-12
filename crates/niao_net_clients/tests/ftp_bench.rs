//! Throughput benchmark: niao_net_clients FTP vs suppaftp (dev-dep).

use niao_net_clients::ftp::{connect, mock::MockFtpServer};
use std::io::{Cursor, Read};
use std::time::Instant;

const PAYLOAD_SIZE: usize = 256 * 1024;
const ITERS: u32 = 16;

fn payload() -> Vec<u8> {
    (0..PAYLOAD_SIZE)
        .map(|i| ((i * 13 + i / 64) % 251) as u8)
        .collect()
}

fn bench_niao(server_port: u16, data: &[u8]) -> f64 {
    let start = Instant::now();
    for i in 0..ITERS {
        let mut client = connect("127.0.0.1", server_port).expect("connect");
        client.login("bench", "bench").expect("login");
        let name = format!("bench_{i}.bin");
        client.put(&name, data).expect("put");
        let got = client.get(&name).expect("get");
        assert_eq!(got.len(), data.len());
        client.quit().ok();
    }
    let secs = start.elapsed().as_secs_f64();
    let mb = (PAYLOAD_SIZE as f64 * ITERS as f64 * 2.0) / (1024.0 * 1024.0);
    mb / secs
}

fn bench_suppaftp(server_port: u16, data: &[u8]) -> f64 {
    use suppaftp::FtpStream;
    let start = Instant::now();
    for i in 0..ITERS {
        let addr = format!("127.0.0.1:{server_port}");
        let mut ftp = FtpStream::connect(&addr).expect("suppaftp connect");
        ftp.login("bench", "bench").expect("suppaftp login");
        let name = format!("bench_{i}.bin");
        let mut cursor = Cursor::new(data);
        ftp.put_file(&name, &mut cursor).expect("suppaftp put");
        let mut reader = ftp.retr_as_stream(&name).expect("suppaftp retr");
        let mut got = Vec::new();
        reader.read_to_end(&mut got).expect("read");
        let _ = ftp.finalize_retr_stream(reader);
        assert_eq!(got.len(), data.len());
        let _ = ftp.quit();
    }
    let secs = start.elapsed().as_secs_f64();
    let mb = (PAYLOAD_SIZE as f64 * ITERS as f64 * 2.0) / (1024.0 * 1024.0);
    mb / secs
}

fn main() {
    let data = payload();
    let server = MockFtpServer::start();
    let port = server.port();

    println!("=== niao_net_clients FTP bench ({PAYLOAD_SIZE} bytes x {ITERS} put+get) ===");
    let niao_mib = bench_niao(port, &data);
    println!("niao_ftp: {niao_mib:.1} MiB/s (put+get combined)");

    let suppa_mib = bench_suppaftp(port, &data);
    println!("suppaftp: {suppa_mib:.1} MiB/s (put+get combined)");

    let ratio = if suppa_mib > 0.0 {
        niao_mib / suppa_mib
    } else {
        1.0
    };
    println!(
        "summary: niao/suppaftp ratio={ratio:.2} (target n/a; parity >= 0.50 acceptable on loopback mock)"
    );
    if ratio < 0.50 {
        eprintln!("warning: niao FTP slower than 50% of suppaftp on this host");
        std::process::exit(1);
    }
    server.shutdown();
}
