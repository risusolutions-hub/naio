//! Minimal RFC 959 FTP server for unit/integration tests.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub struct MockFtpServer {
    port: u16,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl MockFtpServer {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock ftp");
        let port = listener.local_addr().expect("local addr").port();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);
        let files = Arc::new(Mutex::new(HashMap::<String, Vec<u8>>::new()));
        let (ready_tx, ready_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            ready_tx.send(()).ok();
            serve_loop(listener, stop_flag, files);
        });
        ready_rx.recv().expect("mock ftp thread ready");
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

fn serve_loop(
    listener: TcpListener,
    stop: Arc<AtomicBool>,
    files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
) {
    listener
        .set_nonblocking(true)
        .expect("set_nonblocking mock listener");
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).ok();
                let files = Arc::clone(&files);
                thread::spawn(move || {
                    if let Err(e) = handle_client(stream, files) {
                        eprintln!("mock ftp session error: {e}");
                    }
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(2));
            }
            Err(_) => break,
        }
    }
}

fn handle_client(
    stream: TcpStream,
    files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
) -> std::io::Result<()> {
    let mut control = BufReader::new(stream);
    writeln!(control.get_mut(), "220 mock FTP ready")?;
    control.get_mut().flush()?;
    let mut logged_in = false;
    loop {
        let mut line = String::new();
        if control.read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        let upper = line.to_uppercase();
        if upper.starts_with("USER ") {
            writeln!(control.get_mut(), "331 Password required")?;
        } else if upper.starts_with("PASS ") {
            logged_in = true;
            writeln!(control.get_mut(), "230 Login OK")?;
        } else if upper == "QUIT" {
            writeln!(control.get_mut(), "221 Bye")?;
            break;
        } else if !logged_in {
            writeln!(control.get_mut(), "530 Not logged in")?;
        } else if upper.starts_with("TYPE ") {
            writeln!(control.get_mut(), "200 Type set")?;
        } else if upper == "PASV" {
            let data_listener = TcpListener::bind("127.0.0.1:0")?;
            let local = data_listener.local_addr()?;
            let v4 = match local.ip() {
                std::net::IpAddr::V4(v) => v,
                _ => std::net::Ipv4Addr::LOCALHOST,
            };
            let [a, b, c, d] = v4.octets();
            let p = local.port();
            writeln!(
                control.get_mut(),
                "227 Entering Passive Mode ({a},{b},{c},{d},{},{}).",
                p >> 8,
                p & 0xff
            )?;
            control.get_mut().flush()?;
            data_listener.set_nonblocking(false)?;
            if let Some((cmd, arg)) = read_data_command(&mut control)? {
                let (mut data, _) = data_listener.accept()?;
                run_data_command(control.get_mut(), &mut data, &cmd, arg.as_deref(), &files)?;
            }
        } else if upper.starts_with("PORT ") {
            let port_spec = line.split_whitespace().nth(1).unwrap_or("");
            let nums: Vec<u16> = port_spec
                .split(',')
                .filter_map(|p| p.parse().ok())
                .collect();
            if nums.len() != 6 {
                writeln!(control.get_mut(), "501 Bad PORT")?;
                continue;
            }
            let ip = format!("{}.{}.{}.{}", nums[0], nums[1], nums[2], nums[3]);
            let port = (nums[4] << 8) | nums[5];
            writeln!(control.get_mut(), "200 PORT command successful")?;
            control.get_mut().flush()?;
            if let Some((cmd, arg)) = read_data_command(&mut control)? {
                let mut data = TcpStream::connect(format!("{ip}:{port}"))?;
                run_data_command(control.get_mut(), &mut data, &cmd, arg.as_deref(), &files)?;
            }
        } else if upper.starts_with("RETR ")
            || upper.starts_with("STOR ")
            || upper.starts_with("LIST")
        {
            writeln!(control.get_mut(), "425 Use PASV or PORT first")?;
        } else {
            writeln!(control.get_mut(), "502 Command not implemented")?;
        }
        control.get_mut().flush()?;
    }
    Ok(())
}

fn read_data_command(
    reader: &mut BufReader<TcpStream>,
) -> std::io::Result<Option<(String, Option<String>)>> {
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    let line = line.trim().to_string();
    let upper = line.to_uppercase();
    if upper.starts_with("RETR ") {
        let path = line.split_whitespace().nth(1).unwrap_or("").to_string();
        writeln!(reader.get_mut(), "150 Opening data connection")?;
        return Ok(Some(("RETR".into(), Some(path))));
    }
    if upper.starts_with("STOR ") {
        let path = line.split_whitespace().nth(1).unwrap_or("").to_string();
        writeln!(reader.get_mut(), "150 Opening data connection")?;
        return Ok(Some(("STOR".into(), Some(path))));
    }
    if upper == "LIST" || upper.starts_with("LIST ") {
        writeln!(reader.get_mut(), "150 Opening data connection")?;
        let arg = line.split_whitespace().nth(1).map(str::to_owned);
        return Ok(Some(("LIST".into(), arg)));
    }
    writeln!(reader.get_mut(), "503 Bad sequence")?;
    Ok(None)
}

fn run_data_command(
    control: &mut TcpStream,
    data: &mut TcpStream,
    cmd: &str,
    arg: Option<&str>,
    files: &Arc<Mutex<HashMap<String, Vec<u8>>>>,
) -> std::io::Result<()> {
    match cmd {
        "RETR" => {
            let path = arg.unwrap_or("");
            let payload = files.lock().unwrap().get(path).cloned().unwrap_or_default();
            data.write_all(&payload)?;
            data.shutdown(std::net::Shutdown::Write)?;
            writeln!(control, "226 Transfer complete")?;
        }
        "STOR" => {
            let path = arg.unwrap_or("").to_string();
            let mut payload = Vec::new();
            data.read_to_end(&mut payload)?;
            files.lock().unwrap().insert(path, payload);
            writeln!(control, "226 Transfer complete")?;
        }
        "LIST" => {
            let guard = files.lock().unwrap();
            for name in guard.keys() {
                let _ = arg;
                writeln!(data, "-rw-r--r-- 1 mock mock 0 Jan 01 00:00 {name}")?;
            }
            data.shutdown(std::net::Shutdown::Write)?;
            writeln!(control, "226 Listing complete")?;
        }
        _ => writeln!(control, "502 Unsupported data command")?,
    }
    Ok(())
}
