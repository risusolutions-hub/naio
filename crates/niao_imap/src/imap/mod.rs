//! IMAP4rev1 client (RFC 3501 subset + IDLE).

mod parse;

pub use parse::{FetchItem, Folder, IdleEvent, MailboxStatus, SelectData, StoreMode};

use crate::error::{ImapError, Result};
use crate::headers::imap_quote;
use crate::wire::Conn;
use parse::{
    parse_capability_line, parse_fetch_line, parse_list_line, parse_search_line, parse_status_line,
};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ConnectOptions {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub pass: String,
    pub tls: bool,
    pub starttls: bool,
    pub timeout: Duration,
    pub mailbox: Option<String>,
}

impl ConnectOptions {
    pub fn default_port(tls: bool) -> u16 {
        if tls {
            993
        } else {
            143
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Auth,
    Selected,
    Logout,
}

pub struct ImapClient {
    conn: Option<Conn>,
    tag_n: u32,
    state: State,
    pub host: String,
    pub port: u16,
    pub capabilities: Vec<String>,
    pub selected: Option<SelectData>,
}

impl ImapClient {
    fn conn_mut(&mut self) -> Result<&mut Conn> {
        self.conn.as_mut().ok_or(ImapError::NotConnected)
    }

    pub fn connect(opts: &ConnectOptions) -> Result<Self> {
        let mut conn = Conn::connect(&opts.host, opts.port, opts.timeout, opts.tls)?;
        let greeting = conn.read_line()?;
        if !(greeting.starts_with("* OK") || greeting.starts_with("* PREAUTH")) {
            return Err(ImapError::Protocol(format!("bad greeting: {greeting}")));
        }

        let mut tag_n: u32 = 0;
        if opts.starttls && !opts.tls {
            tag_n += 1;
            let tag = format!("A{tag_n:04}");
            conn.write_line(&format!("{tag} STARTTLS"))?;
            loop {
                let line = conn.read_line()?;
                if line.starts_with(&tag) {
                    if line.split_whitespace().nth(1) != Some("OK") {
                        return Err(ImapError::Protocol(line));
                    }
                    break;
                }
            }
            conn = conn.upgrade_tls(&opts.host)?;
        }

        let mut client = Self {
            conn: Some(conn),
            tag_n,
            state: State::Auth,
            host: opts.host.clone(),
            port: opts.port,
            capabilities: Vec::new(),
            selected: None,
        };

        client.refresh_capabilities()?;
        let login = format!(
            "LOGIN {} {}",
            imap_quote(&opts.user),
            imap_quote(&opts.pass)
        );
        client.cmd_ok(&login, "")?;
        client.state = State::Auth;

        if let Some(mb) = &opts.mailbox {
            client.select(mb)?;
        }
        Ok(client)
    }

    fn next_tag(&mut self) -> String {
        self.tag_n += 1;
        format!("A{:04}", self.tag_n)
    }

    fn cmd_collect(&mut self, command: &str) -> Result<(Vec<String>, String)> {
        let tag = self.next_tag();
        self.conn_mut()?.write_line(&format!("{tag} {command}"))?;
        let mut untagged = Vec::new();
        loop {
            let line = self.conn_mut()?.read_line()?;
            if let Some(lit_size) = literal_size_at_end(&line) {
                let data = self.conn_mut()?.read_exact(lit_size)?;
                let mut combined = line;
                combined.push_str(&String::from_utf8_lossy(&data));
                let cont = self.conn_mut()?.read_line().unwrap_or_default();
                if !cont.is_empty() {
                    combined.push_str(&cont);
                }
                untagged.push(combined);
                continue;
            }
            if line.starts_with(&tag) {
                return Ok((untagged, line));
            }
            if line.starts_with('+') {
                untagged.push(line);
                continue;
            }
            untagged.push(line);
        }
    }

    fn cmd_ok(&mut self, command: &str, _extra: &str) -> Result<Vec<String>> {
        let (untagged, tagged) = self.cmd_collect(command)?;
        if tagged.contains(" OK") || tagged.split_whitespace().nth(1) == Some("OK") {
            Ok(untagged)
        } else {
            Err(ImapError::Protocol(tagged))
        }
    }

    pub fn refresh_capabilities(&mut self) -> Result<&[String]> {
        let lines = self.cmd_ok("CAPABILITY", "")?;
        for line in &lines {
            if let Some(caps) = parse_capability_line(line) {
                self.capabilities = caps;
                break;
            }
        }
        Ok(&self.capabilities)
    }

    pub fn capabilities(&self) -> &[String] {
        &self.capabilities
    }

    pub fn noop(&mut self) -> Result<Vec<String>> {
        self.cmd_ok("NOOP", "")
    }

    pub fn logout(&mut self) -> Result<()> {
        let _ = self.cmd_ok("LOGOUT", "");
        self.state = State::Logout;
        Ok(())
    }

    pub fn list(&mut self, reference: &str, pattern: &str) -> Result<Vec<Folder>> {
        let cmd = format!("LIST {} {}", imap_quote(reference), imap_quote(pattern));
        let lines = self.cmd_ok(&cmd, "")?;
        Ok(lines.iter().filter_map(|l| parse_list_line(l)).collect())
    }

    pub fn lsub(&mut self, reference: &str, pattern: &str) -> Result<Vec<Folder>> {
        let cmd = format!("LSUB {} {}", imap_quote(reference), imap_quote(pattern));
        let lines = self.cmd_ok(&cmd, "")?;
        Ok(lines.iter().filter_map(|l| parse_list_line(l)).collect())
    }

    pub fn select(&mut self, mailbox: &str) -> Result<SelectData> {
        let cmd = format!("SELECT {}", imap_quote(mailbox));
        let lines = self.cmd_ok(&cmd, "")?;
        let data = SelectData::from_untagged(mailbox, &lines);
        self.selected = Some(data.clone());
        self.state = State::Selected;
        Ok(data)
    }

    pub fn examine(&mut self, mailbox: &str) -> Result<SelectData> {
        let cmd = format!("EXAMINE {}", imap_quote(mailbox));
        let lines = self.cmd_ok(&cmd, "")?;
        let data = SelectData::from_untagged(mailbox, &lines);
        self.selected = Some(data.clone());
        self.state = State::Selected;
        Ok(data)
    }

    pub fn create(&mut self, mailbox: &str) -> Result<()> {
        let cmd = format!("CREATE {}", imap_quote(mailbox));
        self.cmd_ok(&cmd, "")?;
        Ok(())
    }

    pub fn delete_mailbox(&mut self, mailbox: &str) -> Result<()> {
        let cmd = format!("DELETE {}", imap_quote(mailbox));
        self.cmd_ok(&cmd, "")?;
        Ok(())
    }

    pub fn rename(&mut self, old: &str, new: &str) -> Result<()> {
        let cmd = format!("RENAME {} {}", imap_quote(old), imap_quote(new));
        self.cmd_ok(&cmd, "")?;
        Ok(())
    }

    pub fn subscribe(&mut self, mailbox: &str) -> Result<()> {
        let cmd = format!("SUBSCRIBE {}", imap_quote(mailbox));
        self.cmd_ok(&cmd, "")?;
        Ok(())
    }

    pub fn unsubscribe(&mut self, mailbox: &str) -> Result<()> {
        let cmd = format!("UNSUBSCRIBE {}", imap_quote(mailbox));
        self.cmd_ok(&cmd, "")?;
        Ok(())
    }

    pub fn status(&mut self, mailbox: &str, items: &[&str]) -> Result<MailboxStatus> {
        let item_list = if items.is_empty() {
            "(MESSAGES RECENT UIDNEXT UIDVALIDITY UNSEEN)".to_string()
        } else {
            format!("({})", items.join(" "))
        };
        let cmd = format!("STATUS {} {item_list}", imap_quote(mailbox));
        let lines = self.cmd_ok(&cmd, "")?;
        for line in &lines {
            if let Some(st) = parse_status_line(line) {
                return Ok(st);
            }
        }
        Err(ImapError::Protocol("STATUS response missing".into()))
    }

    pub fn search(&mut self, criteria: &str, uid: bool) -> Result<Vec<u32>> {
        let cmd = if uid {
            format!("UID SEARCH {criteria}")
        } else {
            format!("SEARCH {criteria}")
        };
        let lines = self.cmd_ok(&cmd, "")?;
        for line in &lines {
            if let Some(ids) = parse_search_line(line) {
                return Ok(ids);
            }
        }
        Ok(Vec::new())
    }

    pub fn fetch(&mut self, set: &str, items: &str, uid: bool) -> Result<Vec<FetchItem>> {
        let cmd = if uid {
            format!("UID FETCH {set} {items}")
        } else {
            format!("FETCH {set} {items}")
        };
        let lines = self.cmd_ok(&cmd, "")?;
        let mut out = Vec::new();
        for line in &lines {
            if let Some(item) = parse_fetch_line(line) {
                out.push(item);
            }
        }
        Ok(out)
    }

    pub fn store(
        &mut self,
        set: &str,
        flags: &[String],
        mode: StoreMode,
        uid: bool,
    ) -> Result<Vec<FetchItem>> {
        let flag_list = format!("({})", flags.join(" "));
        let op = match mode {
            StoreMode::Set => "FLAGS",
            StoreMode::Add => "+FLAGS",
            StoreMode::Remove => "-FLAGS",
        };
        let cmd = if uid {
            format!("UID STORE {set} {op} {flag_list}")
        } else {
            format!("STORE {set} {op} {flag_list}")
        };
        let lines = self.cmd_ok(&cmd, "")?;
        Ok(lines.iter().filter_map(|l| parse_fetch_line(l)).collect())
    }

    pub fn copy(&mut self, set: &str, mailbox: &str, uid: bool) -> Result<()> {
        let cmd = if uid {
            format!("UID COPY {set} {}", imap_quote(mailbox))
        } else {
            format!("COPY {set} {}", imap_quote(mailbox))
        };
        self.cmd_ok(&cmd, "")?;
        Ok(())
    }

    pub fn move_msgs(&mut self, set: &str, mailbox: &str, uid: bool) -> Result<()> {
        let has_move = self
            .capabilities
            .iter()
            .any(|c| c.eq_ignore_ascii_case("MOVE"));
        if has_move {
            let cmd = if uid {
                format!("UID MOVE {set} {}", imap_quote(mailbox))
            } else {
                format!("MOVE {set} {}", imap_quote(mailbox))
            };
            self.cmd_ok(&cmd, "")?;
            return Ok(());
        }
        // Emulate: COPY + STORE \Deleted + EXPUNGE
        self.copy(set, mailbox, uid)?;
        self.store(set, &["\\Deleted".into()], StoreMode::Add, uid)?;
        self.expunge()?;
        Ok(())
    }

    pub fn expunge(&mut self) -> Result<Vec<u32>> {
        let lines = self.cmd_ok("EXPUNGE", "")?;
        let mut seqs = Vec::new();
        for line in lines {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 && parts[0] == "*" && parts[2] == "EXPUNGE" {
                if let Ok(n) = parts[1].parse() {
                    seqs.push(n);
                }
            }
        }
        Ok(seqs)
    }

    pub fn close_mailbox(&mut self) -> Result<()> {
        self.cmd_ok("CLOSE", "")?;
        self.selected = None;
        self.state = State::Auth;
        Ok(())
    }

    /// Enter IDLE, wait up to `timeout` for mailbox events, then send DONE.
    pub fn idle(&mut self, timeout: Duration) -> Result<Vec<IdleEvent>> {
        if self.state != State::Selected {
            return Err(ImapError::WrongState(
                "IDLE requires selected mailbox".into(),
            ));
        }
        let has_idle = self
            .capabilities
            .iter()
            .any(|c| c.eq_ignore_ascii_case("IDLE"));
        if !has_idle {
            return Err(ImapError::Protocol("server lacks IDLE capability".into()));
        }

        let tag = self.next_tag();
        self.conn_mut()?.write_line(&format!("{tag} IDLE"))?;
        let cont = self.conn_mut()?.read_line()?;
        if !cont.starts_with('+') {
            return Err(ImapError::Protocol(format!("IDLE expected +, got {cont}")));
        }

        let _ = self.conn_mut()?.set_timeout(timeout);
        let mut events = Vec::new();
        match self.conn_mut()?.read_line() {
            Ok(line) => {
                if let Some(ev) = IdleEvent::parse(&line) {
                    events.push(ev);
                }
            }
            Err(ImapError::Timeout(_)) => {}
            Err(e) => {
                let _ = self.conn_mut().and_then(|c| c.write_line("DONE"));
                let _ = self.conn_mut().and_then(|c| c.read_line());
                return Err(e);
            }
        }

        self.conn_mut()?.write_line("DONE")?;
        loop {
            let line = self.conn_mut()?.read_line()?;
            if line.starts_with(&tag) {
                break;
            }
            if let Some(ev) = IdleEvent::parse(&line) {
                events.push(ev);
            }
        }
        let _ = self.conn_mut()?.set_timeout(Duration::from_secs(60));
        Ok(events)
    }
}

fn literal_size_at_end(line: &str) -> Option<usize> {
    // look for `{n}` or `{n+}` at end
    let start = line.rfind('{')?;
    let end = line.rfind('}')?;
    if end <= start {
        return None;
    }
    let inner = &line[start + 1..end];
    let inner = inner.trim_end_matches('+');
    inner.parse().ok()
}
