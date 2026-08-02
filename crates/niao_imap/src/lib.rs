//! IMAP4 + POP3 mailbox retrieval for Niao (`nimap`).
//!
//! Native protocol implementation with optional rustls TLS — not a thin re-export.

pub mod error;
pub mod headers;
pub mod imap;
pub mod mock;
pub mod pop3;
pub mod tls;
pub mod wire;

pub use error::{ImapError, Result};
pub use headers::{format_message_set, imap_quote, parse_headers};
pub use imap::{
    ConnectOptions, FetchItem, Folder, IdleEvent, ImapClient, MailboxStatus, SelectData, StoreMode,
};
pub use pop3::{PopClient, PopConnectOptions, PopListItem, PopStat, PopUidlItem};

#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn imap_mock_roundtrip() {
        let server = mock::MockImapServer::start();
        let port = server.port();
        let opts = ConnectOptions {
            host: "127.0.0.1".into(),
            port,
            user: "u".into(),
            pass: "p".into(),
            tls: false,
            starttls: false,
            timeout: Duration::from_secs(5),
            mailbox: None,
        };
        let mut c = ImapClient::connect(&opts).expect("connect");
        assert!(c.capabilities().iter().any(|x| x == "IDLE"));
        let folders = c.list("", "*").expect("list");
        assert!(folders.iter().any(|f| f.name == "INBOX"));
        let sel = c.select("INBOX").expect("select");
        assert_eq!(sel.exists, 1);
        let ids = c.search("ALL", false).expect("search");
        assert_eq!(ids, vec![1]);
        let items = c.fetch("1", "(FLAGS UID BODY[])", false).expect("fetch");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].uid, Some(1));
        assert!(items[0].body.as_ref().unwrap().contains("Subject: Hello"));
        let events = c.idle(Duration::from_millis(50)).expect("idle");
        let _ = events;
        c.logout().expect("logout");
        server.shutdown();
    }

    #[test]
    fn pop_mock_roundtrip() {
        let server = mock::MockPopServer::start();
        let port = server.port();
        let opts = PopConnectOptions {
            host: "127.0.0.1".into(),
            port,
            user: "u".into(),
            pass: "p".into(),
            tls: false,
            starttls: false,
            timeout: Duration::from_secs(5),
        };
        let mut c = PopClient::connect(&opts).expect("pop connect");
        let st = c.stat().expect("stat");
        assert_eq!(st.count, 1);
        let list = c.list(None).expect("list");
        assert_eq!(list.len(), 1);
        let raw = c.retr(1).expect("retr");
        assert!(raw.contains("Pop Hello"));
        let headers = parse_headers(&raw);
        assert_eq!(
            headers.get("subject").map(String::as_str),
            Some("Pop Hello")
        );
        c.quit().expect("quit");
        server.shutdown();
    }

    #[test]
    fn quote_and_set() {
        assert_eq!(imap_quote("a"), "\"a\"");
        assert_eq!(format_message_set(&[1, 2, 3, 9]), "1:3,9");
    }
}
