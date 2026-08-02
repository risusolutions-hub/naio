//! Minimal IMAP4rev1 mock server.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct MockMessage {
    pub uid: u32,
    pub flags: Vec<String>,
    pub raw: String,
}

struct Mailbox {
    messages: Vec<MockMessage>,
    uidnext: u32,
    uidvalidity: u32,
}

impl Mailbox {
    fn inbox() -> Self {
        let raw =
            "From: alice@example.com\r\nTo: bob@example.com\r\nSubject: Hello\r\n\r\nHi Bob!\r\n";
        Self {
            messages: vec![MockMessage {
                uid: 1,
                flags: vec!["\\Seen".into()],
                raw: raw.into(),
            }],
            uidnext: 2,
            uidvalidity: 1,
        }
    }
}

pub struct MockImapServer {
    port: u16,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl MockImapServer {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock imap");
        let port = listener.local_addr().expect("addr").port();
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = Arc::clone(&stop);
        let mailboxes = Arc::new(Mutex::new({
            let mut m = HashMap::new();
            m.insert("INBOX".to_string(), Mailbox::inbox());
            m
        }));
        let handle = thread::spawn(move || {
            listener.set_nonblocking(true).ok();
            while !stop2.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        stream.set_nonblocking(false).ok();
                        let boxes = Arc::clone(&mailboxes);
                        thread::spawn(move || {
                            let _ = handle_client(stream, boxes);
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

fn handle_client(
    stream: TcpStream,
    mailboxes: Arc<Mutex<HashMap<String, Mailbox>>>,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;
    writeln!(writer, "* OK mock IMAP ready")?;
    writer.flush()?;

    let mut selected: Option<String> = None;

    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim_end_matches(['\r', '\n']).to_string();
        if line.is_empty() {
            continue;
        }
        let (tag, cmd) = split_tag(&line);
        let upper = cmd.to_ascii_uppercase();

        if upper.starts_with("CAPABILITY") {
            writeln!(writer, "* CAPABILITY IMAP4rev1 IDLE MOVE AUTH=PLAIN")?;
            writeln!(writer, "{tag} OK CAPABILITY completed")?;
        } else if upper.starts_with("LOGIN ") {
            writeln!(writer, "{tag} OK LOGIN completed")?;
        } else if upper.starts_with("LOGOUT") {
            writeln!(writer, "* BYE logging out")?;
            writeln!(writer, "{tag} OK LOGOUT completed")?;
            break;
        } else if upper.starts_with("NOOP") {
            writeln!(writer, "{tag} OK NOOP completed")?;
        } else if upper.starts_with("LIST ") || upper.starts_with("LSUB ") {
            let boxes = mailboxes.lock().unwrap();
            for name in boxes.keys() {
                writeln!(writer, "* LIST (\\HasNoChildren) \"/\" \"{}\"", name)?;
            }
            writeln!(writer, "{tag} OK LIST completed")?;
        } else if upper.starts_with("SELECT ") || upper.starts_with("EXAMINE ") {
            let mb = parse_quoted_arg(&cmd).unwrap_or_else(|| "INBOX".into());
            let boxes = mailboxes.lock().unwrap();
            if let Some(box_) = boxes.get(&mb) {
                selected = Some(mb.clone());
                writeln!(
                    writer,
                    "* FLAGS (\\Seen \\Answered \\Flagged \\Deleted \\Draft)"
                )?;
                writeln!(writer, "* {} EXISTS", box_.messages.len())?;
                writeln!(writer, "* 0 RECENT")?;
                writeln!(writer, "* OK [UIDVALIDITY {}] UIDs valid", box_.uidvalidity)?;
                writeln!(writer, "* OK [UIDNEXT {}] Predicted next UID", box_.uidnext)?;
                let ro = if upper.starts_with("EXAMINE") {
                    "READ-ONLY"
                } else {
                    "READ-WRITE"
                };
                writeln!(writer, "{tag} OK [{ro}] SELECT completed")?;
            } else {
                writeln!(writer, "{tag} NO mailbox missing")?;
            }
        } else if upper.starts_with("CREATE ") {
            let mb = parse_quoted_arg(&cmd).unwrap_or_default();
            let mut boxes = mailboxes.lock().unwrap();
            boxes.entry(mb).or_insert_with(|| Mailbox {
                messages: Vec::new(),
                uidnext: 1,
                uidvalidity: 1,
            });
            writeln!(writer, "{tag} OK CREATE completed")?;
        } else if upper.starts_with("DELETE ") {
            let mb = parse_quoted_arg(&cmd).unwrap_or_default();
            mailboxes.lock().unwrap().remove(&mb);
            writeln!(writer, "{tag} OK DELETE completed")?;
        } else if upper.starts_with("RENAME ") {
            let args = parse_two_quoted(&cmd);
            if let Some((old, new)) = args {
                let mut boxes = mailboxes.lock().unwrap();
                if let Some(mb) = boxes.remove(&old) {
                    boxes.insert(new, mb);
                    writeln!(writer, "{tag} OK RENAME completed")?;
                } else {
                    writeln!(writer, "{tag} NO no such mailbox")?;
                }
            } else {
                writeln!(writer, "{tag} BAD rename args")?;
            }
        } else if upper.starts_with("SUBSCRIBE ") || upper.starts_with("UNSUBSCRIBE ") {
            writeln!(writer, "{tag} OK subscribe completed")?;
        } else if upper.starts_with("STATUS ") {
            let mb = parse_quoted_arg(&cmd).unwrap_or_else(|| "INBOX".into());
            let boxes = mailboxes.lock().unwrap();
            if let Some(box_) = boxes.get(&mb) {
                writeln!(
                    writer,
                    "* STATUS \"{}\" (MESSAGES {} RECENT 0 UIDNEXT {} UIDVALIDITY {} UNSEEN 0)",
                    mb,
                    box_.messages.len(),
                    box_.uidnext,
                    box_.uidvalidity
                )?;
                writeln!(writer, "{tag} OK STATUS completed")?;
            } else {
                writeln!(writer, "{tag} NO no mailbox")?;
            }
        } else if upper.starts_with("SEARCH ") || upper.starts_with("UID SEARCH ") {
            let uid = upper.starts_with("UID ");
            let boxes = mailboxes.lock().unwrap();
            let mb = selected.as_ref().and_then(|s| boxes.get(s));
            if let Some(box_) = mb {
                let ids: Vec<String> = if uid {
                    box_.messages.iter().map(|m| m.uid.to_string()).collect()
                } else {
                    (1..=box_.messages.len()).map(|i| i.to_string()).collect()
                };
                writeln!(writer, "* SEARCH {}", ids.join(" "))?;
                writeln!(writer, "{tag} OK SEARCH completed")?;
            } else {
                writeln!(writer, "{tag} NO not selected")?;
            }
        } else if upper.starts_with("FETCH ") || upper.starts_with("UID FETCH ") {
            let uid_mode = upper.starts_with("UID ");
            let set = fetch_set(&cmd);
            let boxes = mailboxes.lock().unwrap();
            if let Some(name) = &selected {
                if let Some(box_) = boxes.get(name) {
                    for (i, msg) in box_.messages.iter().enumerate() {
                        let seq = (i + 1) as u32;
                        let id = if uid_mode { msg.uid } else { seq };
                        if !set_contains(&set, id) {
                            continue;
                        }
                        let flags = format!("({})", msg.flags.join(" "));
                        let body = msg.raw.as_bytes();
                        write!(
                            writer,
                            "* {seq} FETCH (UID {} FLAGS {flags} RFC822.SIZE {} BODY[] {{{}}}\r\n",
                            msg.uid,
                            body.len(),
                            body.len()
                        )?;
                        writer.write_all(body)?;
                        writeln!(writer, ")")?;
                    }
                }
            }
            writeln!(writer, "{tag} OK FETCH completed")?;
        } else if upper.starts_with("STORE ") || upper.starts_with("UID STORE ") {
            writeln!(writer, "{tag} OK STORE completed")?;
        } else if upper.starts_with("COPY ") || upper.starts_with("UID COPY ") {
            writeln!(writer, "{tag} OK COPY completed")?;
        } else if upper.starts_with("MOVE ") || upper.starts_with("UID MOVE ") {
            writeln!(writer, "{tag} OK MOVE completed")?;
        } else if upper.starts_with("EXPUNGE") {
            writeln!(writer, "{tag} OK EXPUNGE completed")?;
        } else if upper.starts_with("CLOSE") {
            selected = None;
            writeln!(writer, "{tag} OK CLOSE completed")?;
        } else if upper.starts_with("IDLE") {
            writeln!(writer, "+ idling")?;
            writer.flush()?;
            // wait for DONE
            let mut done_line = String::new();
            reader.read_line(&mut done_line)?;
            writeln!(writer, "{tag} OK IDLE terminated")?;
        } else if upper.starts_with("STARTTLS") {
            writeln!(writer, "{tag} NO TLS not available on mock")?;
        } else {
            writeln!(writer, "{tag} BAD unknown command")?;
        }
        writer.flush()?;
    }
    Ok(())
}

fn split_tag(line: &str) -> (String, String) {
    match line.split_once(' ') {
        Some((t, rest)) => (t.to_string(), rest.to_string()),
        None => (line.to_string(), String::new()),
    }
}

fn parse_quoted_arg(cmd: &str) -> Option<String> {
    let start = cmd.find('"')?;
    let rest = &cmd[start + 1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn parse_two_quoted(cmd: &str) -> Option<(String, String)> {
    let a = parse_quoted_arg(cmd)?;
    let first_end = cmd.find('"')? + 1 + a.len() + 1;
    let rest = &cmd[first_end..];
    let b = parse_quoted_arg(rest)?;
    Some((a, b))
}

fn fetch_set(cmd: &str) -> String {
    // "FETCH 1:3 (FLAGS" or "UID FETCH 1 (FLAGS"
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.len() >= 2 {
        if parts[0].eq_ignore_ascii_case("UID") && parts.len() >= 3 {
            parts[2].to_string()
        } else {
            parts[1].to_string()
        }
    } else {
        "1".into()
    }
}

fn set_contains(set: &str, id: u32) -> bool {
    for part in set.split(',') {
        if let Some((a, b)) = part.split_once(':') {
            let a: u32 = a.parse().unwrap_or(0);
            let b: u32 = if b == "*" {
                u32::MAX
            } else {
                b.parse().unwrap_or(0)
            };
            if id >= a && id <= b {
                return true;
            }
        } else if part.parse::<u32>().ok() == Some(id) {
            return true;
        }
    }
    false
}
