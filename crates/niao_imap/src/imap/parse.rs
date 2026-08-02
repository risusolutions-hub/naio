//! IMAP response parsing helpers.

#[derive(Debug, Clone)]
pub struct Folder {
    pub name: String,
    pub delimiter: String,
    pub attrs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SelectData {
    pub mailbox: String,
    pub exists: u32,
    pub recent: u32,
    pub uidnext: Option<u32>,
    pub uidvalidity: Option<u32>,
    pub unseen: Option<u32>,
    pub flags: Vec<String>,
    pub permanent_flags: Vec<String>,
    pub readonly: bool,
}

impl SelectData {
    pub fn from_untagged(mailbox: &str, lines: &[String]) -> Self {
        let mut data = SelectData {
            mailbox: mailbox.to_string(),
            exists: 0,
            recent: 0,
            uidnext: None,
            uidvalidity: None,
            unseen: None,
            flags: Vec::new(),
            permanent_flags: Vec::new(),
            readonly: false,
        };
        for line in lines {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 && parts[0] == "*" {
                if parts[2] == "EXISTS" {
                    data.exists = parts[1].parse().unwrap_or(0);
                } else if parts[2] == "RECENT" {
                    data.recent = parts[1].parse().unwrap_or(0);
                } else if parts[1] == "FLAGS" {
                    data.flags = extract_paren_list(line);
                } else if line.contains("[UIDNEXT ") {
                    data.uidnext = extract_bracket_num(line, "UIDNEXT");
                } else if line.contains("[UIDVALIDITY ") {
                    data.uidvalidity = extract_bracket_num(line, "UIDVALIDITY");
                } else if line.contains("[UNSEEN ") {
                    data.unseen = extract_bracket_num(line, "UNSEEN");
                } else if line.contains("PERMANENTFLAGS") {
                    data.permanent_flags = extract_paren_list(line);
                } else if line.contains("[READ-ONLY]") {
                    data.readonly = true;
                }
            }
        }
        data
    }
}

#[derive(Debug, Clone)]
pub struct MailboxStatus {
    pub mailbox: String,
    pub messages: Option<u32>,
    pub recent: Option<u32>,
    pub uidnext: Option<u32>,
    pub uidvalidity: Option<u32>,
    pub unseen: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct FetchItem {
    pub seq: u32,
    pub uid: Option<u32>,
    pub flags: Vec<String>,
    pub size: Option<u32>,
    pub body: Option<String>,
    pub raw: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreMode {
    Set,
    Add,
    Remove,
}

impl StoreMode {
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "add" | "+flags" | "+" => StoreMode::Add,
            "remove" | "-flags" | "-" => StoreMode::Remove,
            _ => StoreMode::Set,
        }
    }
}

#[derive(Debug, Clone)]
pub enum IdleEvent {
    Exists(u32),
    Expunge(u32),
    Recent(u32),
    Other(String),
}

impl IdleEvent {
    pub fn parse(line: &str) -> Option<Self> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 && parts[0] == "*" {
            match parts[2] {
                "EXISTS" => Some(IdleEvent::Exists(parts[1].parse().ok()?)),
                "EXPUNGE" => Some(IdleEvent::Expunge(parts[1].parse().ok()?)),
                "RECENT" => Some(IdleEvent::Recent(parts[1].parse().ok()?)),
                _ => Some(IdleEvent::Other(line.to_string())),
            }
        } else {
            None
        }
    }

    pub fn kind_name(&self) -> &'static str {
        match self {
            IdleEvent::Exists(_) => "exists",
            IdleEvent::Expunge(_) => "expunge",
            IdleEvent::Recent(_) => "recent",
            IdleEvent::Other(_) => "other",
        }
    }

    pub fn value(&self) -> Option<u32> {
        match self {
            IdleEvent::Exists(n) | IdleEvent::Expunge(n) | IdleEvent::Recent(n) => Some(*n),
            IdleEvent::Other(_) => None,
        }
    }
}

pub fn parse_capability_line(line: &str) -> Option<Vec<String>> {
    let upper = line.to_ascii_uppercase();
    if !upper.contains("CAPABILITY") {
        return None;
    }
    let rest = if let Some(idx) = upper.find("CAPABILITY") {
        &line[idx + "CAPABILITY".len()..]
    } else {
        return None;
    };
    Some(
        rest.split_whitespace()
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .collect(),
    )
}

pub fn parse_list_line(line: &str) -> Option<Folder> {
    // * LIST (\HasNoChildren) "/" "INBOX"
    if !line.to_ascii_uppercase().contains(" LIST ")
        && !line.to_ascii_uppercase().contains(" LSUB ")
    {
        return None;
    }
    let attrs = extract_paren_list(line);
    // find quoted delimiter and name after attrs
    let after_paren = line.find(')').map(|i| &line[i + 1..])?;
    let mut q = quoted_strings(after_paren);
    if q.len() < 2 {
        // maybe unquoted
        let toks: Vec<&str> = after_paren.split_whitespace().collect();
        if toks.len() >= 2 {
            return Some(Folder {
                name: unquote(toks[1]),
                delimiter: unquote(toks[0]),
                attrs,
            });
        }
        return None;
    }
    Some(Folder {
        delimiter: q.remove(0),
        name: q.remove(0),
        attrs,
    })
}

pub fn parse_search_line(line: &str) -> Option<Vec<u32>> {
    let upper = line.to_ascii_uppercase();
    if !upper.starts_with("* SEARCH") {
        return None;
    }
    Some(
        line.split_whitespace()
            .skip(2)
            .filter_map(|t| t.parse().ok())
            .collect(),
    )
}

pub fn parse_status_line(line: &str) -> Option<MailboxStatus> {
    // * STATUS "INBOX" (MESSAGES 3 RECENT 1 ...)
    let upper = line.to_ascii_uppercase();
    if !upper.contains(" STATUS ") {
        return None;
    }
    let q = quoted_strings(line);
    let mailbox = q.first().cloned().unwrap_or_else(|| {
        line.split_whitespace()
            .nth(2)
            .unwrap_or("")
            .trim_matches('"')
            .to_string()
    });
    let mut st = MailboxStatus {
        mailbox,
        messages: None,
        recent: None,
        uidnext: None,
        uidvalidity: None,
        unseen: None,
    };
    let paren = extract_paren_list_tokens(line);
    let mut i = 0;
    while i + 1 < paren.len() {
        match paren[i].to_ascii_uppercase().as_str() {
            "MESSAGES" => st.messages = paren[i + 1].parse().ok(),
            "RECENT" => st.recent = paren[i + 1].parse().ok(),
            "UIDNEXT" => st.uidnext = paren[i + 1].parse().ok(),
            "UIDVALIDITY" => st.uidvalidity = paren[i + 1].parse().ok(),
            "UNSEEN" => st.unseen = paren[i + 1].parse().ok(),
            _ => {}
        }
        i += 2;
    }
    Some(st)
}

pub fn parse_fetch_line(line: &str) -> Option<FetchItem> {
    // * 1 FETCH (UID 1 FLAGS (\Seen) BODY[] {..}data)
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 3 || parts[0] != "*" || parts[2] != "FETCH" {
        return None;
    }
    let seq: u32 = parts[1].parse().ok()?;
    let mut item = FetchItem {
        seq,
        uid: None,
        flags: Vec::new(),
        size: None,
        body: None,
        raw: line.to_string(),
    };
    if let Some(uid) = extract_token_num(line, "UID") {
        item.uid = Some(uid);
    }
    if let Some(sz) = extract_token_num(line, "RFC822.SIZE") {
        item.size = Some(sz);
    }
    item.flags = extract_flags_from_fetch(line);
    // body may be embedded after literal in our combined line
    if let Some(idx) = line.find("BODY[]") {
        let rest = &line[idx..];
        if let Some(brace) = rest.find('{') {
            if let Some(end) = rest[brace..].find('}') {
                let after = &rest[brace + end + 1..];
                if !after.is_empty() {
                    item.body = Some(after.to_string());
                }
            }
        } else if let Some(q) = quoted_strings(rest).into_iter().next() {
            item.body = Some(q);
        }
    } else if let Some(idx) = line.find("RFC822 ") {
        let rest = &line[idx + 6..];
        if let Some(q) = quoted_strings(rest).into_iter().next() {
            item.body = Some(q);
        }
    }
    Some(item)
}

fn extract_paren_list(line: &str) -> Vec<String> {
    extract_paren_list_tokens(line)
}

fn extract_paren_list_tokens(line: &str) -> Vec<String> {
    let start = match line.find('(') {
        Some(i) => i + 1,
        None => return Vec::new(),
    };
    let end = match line[start..].find(')') {
        Some(i) => start + i,
        None => return Vec::new(),
    };
    line[start..end]
        .split_whitespace()
        .map(|s| s.to_string())
        .collect()
}

fn extract_flags_from_fetch(line: &str) -> Vec<String> {
    if let Some(idx) = line.find("FLAGS") {
        extract_paren_list_tokens(&line[idx..])
    } else {
        Vec::new()
    }
}

fn extract_bracket_num(line: &str, key: &str) -> Option<u32> {
    let pat = format!("[{key} ");
    let idx = line.find(&pat)?;
    let rest = &line[idx + pat.len()..];
    let end = rest.find(']')?;
    rest[..end].trim().parse().ok()
}

fn extract_token_num(line: &str, key: &str) -> Option<u32> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    for i in 0..parts.len().saturating_sub(1) {
        let tok = parts[i].trim_matches(|c| c == '(' || c == ')');
        if tok.eq_ignore_ascii_case(key) {
            return parts[i + 1]
                .trim_matches(|c| c == '(' || c == ')')
                .parse()
                .ok();
        }
    }
    None
}

fn quoted_strings(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' {
            let mut buf = String::new();
            while let Some(ch) = chars.next() {
                if ch == '\\' {
                    if let Some(n) = chars.next() {
                        buf.push(n);
                    }
                } else if ch == '"' {
                    break;
                } else {
                    buf.push(ch);
                }
            }
            out.push(buf);
        }
    }
    out
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        s[1..s.len() - 1]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_line() {
        let ids = parse_search_line("* SEARCH 1 2 5").unwrap();
        assert_eq!(ids, vec![1, 2, 5]);
    }

    #[test]
    fn list_line() {
        let f = parse_list_line(r#"* LIST (\HasNoChildren) "/" "INBOX""#).unwrap();
        assert_eq!(f.name, "INBOX");
        assert_eq!(f.delimiter, "/");
    }

    #[test]
    fn capability_line() {
        let c = parse_capability_line("* CAPABILITY IMAP4rev1 IDLE MOVE").unwrap();
        assert!(c.iter().any(|x| x == "IDLE"));
    }
}
