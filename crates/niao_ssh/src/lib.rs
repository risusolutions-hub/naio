//! `niao_ssh` — production SSH client: exec, interactive shell, SFTP,
//! local port forwarding, agent + key/password auth (~paramiko, fabric).
//!
//! Thin Niao binding lives in `niao_runtime::nssh`; this crate holds the
//! protocol logic so a future C11 port only needs a new boundary layer.

mod agent;
mod config;
mod error;
mod key;
mod runtime;
mod session;
pub mod testutil;

pub use agent::{agent_identities, AgentIdentity};
pub use config::ConnectConfig;
pub use error::{SshError, SshResult};
pub use key::{key_fingerprint, load_key_data, load_key_file};
pub use session::{
    close, connect, exec, forward_addr, forward_close, forward_local, is_connected, sftp_close,
    sftp_get, sftp_listdir, sftp_mkdir, sftp_open, sftp_put, sftp_read, sftp_remove, sftp_rename,
    sftp_rmdir, sftp_stat, sftp_write, shell_close, shell_open, shell_read, shell_write,
    ExecResult, ForwardInfo, SftpEntry, SftpStat,
};
pub use testutil::{start_test_server as testutil_start, TestServer, TEST_PASS, TEST_USER};
