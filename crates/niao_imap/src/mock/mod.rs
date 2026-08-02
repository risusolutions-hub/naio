//! In-process mock IMAP4 / POP3 servers for tests and benches.

mod imap_server;
mod pop_server;

pub use imap_server::MockImapServer;
pub use pop_server::MockPopServer;
