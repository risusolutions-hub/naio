//! Minimal POP3 mock server.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub struct MockPopServer {
    port: u16,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl MockPopServer {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock pop");
        let port = listener.local_addr().expect("addr").port();
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            listener.set_nonblocking(true).ok();
            while !stop2.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        stream.set_nonblocking(false).ok();
                        thread::spawn(move || {
                            let _ = handle_client(stream);
                        });
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            port,
            stop,
            handle: Some(handle),
        }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn shutdown(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

const MSG: &str = "From: alice@example.com\r\nSubject: Pop Hello\r\n\r\nBody line\r\n";

fn handle_client(stream: TcpStream) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;
    writeln!(writer, "+OK mock POP3 ready")?;
    writer.flush()?;

    let mut deleted = false;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim_end_matches(['\r', '\n']).to_string();
        let upper = line.to_ascii_uppercase();
        if upper.starts_with("USER ") {
            writeln!(writer, "+OK user ok")?;
        } else if upper.starts_with("PASS ") {
            writeln!(writer, "+OK logged in")?;
        } else if upper == "STAT" {
            if deleted {
                writeln!(writer, "+OK 0 0")?;
            } else {
                writeln!(writer, "+OK 1 {}", MSG.len())?;
            }
        } else if upper == "LIST" {
            writeln!(writer, "+OK")?;
            if !deleted {
                writeln!(writer, "1 {}", MSG.len())?;
            }
            writeln!(writer, ".")?;
        } else if upper.starts_with("LIST ") {
            writeln!(writer, "+OK 1 {}", MSG.len())?;
        } else if upper.starts_with("RETR ") {
            writeln!(writer, "+OK")?;
            for l in MSG.lines() {
                if l.starts_with('.') {
                    writeln!(writer, ".{l}")?;
                } else {
                    writeln!(writer, "{l}")?;
                }
            }
            writeln!(writer, ".")?;
        } else if upper.starts_with("TOP ") {
            writeln!(writer, "+OK")?;
            writeln!(writer, "From: alice@example.com")?;
            writeln!(writer, "Subject: Pop Hello")?;
            writeln!(writer, ".")?;
        } else if upper.starts_with("DELE ") {
            deleted = true;
            writeln!(writer, "+OK deleted")?;
        } else if upper == "NOOP" {
            writeln!(writer, "+OK")?;
        } else if upper == "RSET" {
            deleted = false;
            writeln!(writer, "+OK")?;
        } else if upper == "UIDL" {
            writeln!(writer, "+OK")?;
            if !deleted {
                writeln!(writer, "1 uid-msg-1")?;
            }
            writeln!(writer, ".")?;
        } else if upper.starts_with("UIDL ") {
            writeln!(writer, "+OK 1 uid-msg-1")?;
        } else if upper == "CAPA" {
            writeln!(writer, "+OK")?;
            writeln!(writer, "USER")?;
            writeln!(writer, "UIDL")?;
            writeln!(writer, "TOP")?;
            writeln!(writer, ".")?;
        } else if upper == "QUIT" {
            writeln!(writer, "+OK bye")?;
            break;
        } else {
            writeln!(writer, "-ERR unknown")?;
        }
        writer.flush()?;
    }
    Ok(())
}
