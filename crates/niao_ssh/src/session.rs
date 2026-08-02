//! Session registry, connect, exec, shell, SFTP, and local forwarding.

use crate::agent::connect_agent;
use crate::config::ConnectConfig;
use crate::error::{SshError, SshResult};
use crate::key::{arc_key, load_key_data, load_key_file};
use crate::runtime::{block_on, spawn};
use russh::client::{self, AuthResult, Handle, Msg};
use russh::keys::{PrivateKeyWithHashAlg, PublicKey};
use russh::{Channel, ChannelMsg, Disconnect};
use russh_sftp::client::SftpSession;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{oneshot, Mutex};
use tokio::time::{timeout, Instant};

// ---------------------------------------------------------------------------
// Client handler
// ---------------------------------------------------------------------------

struct AcceptAll;

impl client::Handler for AcceptAll {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

struct SessionState {
    handle: Mutex<Handle<AcceptAll>>,
    alive: AtomicBool,
}

struct ShellState {
    channel: Mutex<Channel<Msg>>,
    buf: Mutex<Vec<u8>>,
    eof: AtomicBool,
    session: i64,
}

struct SftpState {
    sftp: SftpSession,
    session: i64,
}

struct ForwardState {
    abort: Option<oneshot::Sender<()>>,
    bind_addr: SocketAddr,
    session: i64,
}

static NEXT_ID: AtomicI64 = AtomicI64::new(1);

fn alloc_id() -> i64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

fn sessions() -> &'static StdMutex<HashMap<i64, Arc<SessionState>>> {
    static M: std::sync::OnceLock<StdMutex<HashMap<i64, Arc<SessionState>>>> =
        std::sync::OnceLock::new();
    M.get_or_init(|| StdMutex::new(HashMap::new()))
}

fn shells() -> &'static StdMutex<HashMap<i64, Arc<ShellState>>> {
    static M: std::sync::OnceLock<StdMutex<HashMap<i64, Arc<ShellState>>>> =
        std::sync::OnceLock::new();
    M.get_or_init(|| StdMutex::new(HashMap::new()))
}

fn sftps() -> &'static StdMutex<HashMap<i64, Arc<SftpState>>> {
    static M: std::sync::OnceLock<StdMutex<HashMap<i64, Arc<SftpState>>>> =
        std::sync::OnceLock::new();
    M.get_or_init(|| StdMutex::new(HashMap::new()))
}

fn forwards() -> &'static StdMutex<HashMap<i64, ForwardState>> {
    static M: std::sync::OnceLock<StdMutex<HashMap<i64, ForwardState>>> =
        std::sync::OnceLock::new();
    M.get_or_init(|| StdMutex::new(HashMap::new()))
}

fn get_session(id: i64) -> SshResult<Arc<SessionState>> {
    let map = sessions()
        .lock()
        .map_err(|_| SshError::msg("session lock poisoned"))?;
    let s = map.get(&id).cloned().ok_or(SshError::InvalidHandle(id))?;
    if !s.alive.load(Ordering::Relaxed) {
        return Err(SshError::InvalidHandle(id));
    }
    Ok(s)
}

fn get_shell(id: i64) -> SshResult<Arc<ShellState>> {
    shells()
        .lock()
        .map_err(|_| SshError::msg("shell lock poisoned"))?
        .get(&id)
        .cloned()
        .ok_or(SshError::InvalidHandle(id))
}

fn get_sftp(id: i64) -> SshResult<Arc<SftpState>> {
    sftps()
        .lock()
        .map_err(|_| SshError::msg("sftp lock poisoned"))?
        .get(&id)
        .cloned()
        .ok_or(SshError::InvalidHandle(id))
}

// ---------------------------------------------------------------------------
// Public result types
// ---------------------------------------------------------------------------

/// Outcome of a remote command execution.
#[derive(Debug, Clone)]
pub struct ExecResult {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_status: u32,
    pub ok: bool,
}

/// Directory listing entry.
#[derive(Debug, Clone)]
pub struct SftpEntry {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub is_file: bool,
}

/// `stat` attributes.
#[derive(Debug, Clone)]
pub struct SftpStat {
    pub size: u64,
    pub is_dir: bool,
    pub is_file: bool,
    pub permissions: Option<u32>,
}

/// Local forward bind info.
#[derive(Debug, Clone)]
pub struct ForwardInfo {
    pub id: i64,
    pub bind_port: u16,
    pub bind_addr: String,
}

// ---------------------------------------------------------------------------
// Connect
// ---------------------------------------------------------------------------

/// Open an SSH session. Returns a session handle id.
pub fn connect(cfg: &ConnectConfig) -> SshResult<i64> {
    block_on(connect_async(cfg))
}

async fn connect_async(cfg: &ConnectConfig) -> SshResult<i64> {
    if cfg.host.is_empty() {
        return Err(SshError::msg("host is required"));
    }
    if cfg.user.is_empty() {
        return Err(SshError::msg("user is required"));
    }
    if cfg.password.is_none() && cfg.key_path.is_none() && cfg.key_data.is_none() && !cfg.agent {
        return Err(SshError::msg(
            "authentication required: set password, key, key_data, or agent",
        ));
    }

    let inactivity = cfg
        .timeout_ms
        .map(|ms| Duration::from_millis(ms.max(1)))
        .unwrap_or(Duration::from_secs(30));

    let config = client::Config {
        inactivity_timeout: Some(inactivity),
        nodelay: true,
        ..Default::default()
    };

    let addr = (cfg.host.as_str(), cfg.port);
    let connect_fut = client::connect(Arc::new(config), addr, AcceptAll);
    let mut handle = match cfg.timeout_ms {
        Some(ms) => timeout(Duration::from_millis(ms.max(1)), connect_fut)
            .await
            .map_err(|_| SshError::Timeout)?
            .map_err(|e| SshError::Connect(e.to_string()))?,
        None => connect_fut
            .await
            .map_err(|e| SshError::Connect(e.to_string()))?,
    };

    let mut authenticated = false;

    if let Some(ref pw) = cfg.password {
        let res = handle
            .authenticate_password(cfg.user.clone(), pw.clone())
            .await?;
        if res.success() {
            authenticated = true;
        }
    }

    if !authenticated {
        if let Some(ref path) = cfg.key_path {
            let key = load_key_file(path, cfg.passphrase.as_deref())?;
            authenticated = try_pubkey(&mut handle, &cfg.user, key).await?;
        }
    }

    if !authenticated {
        if let Some(ref data) = cfg.key_data {
            let key = load_key_data(data, cfg.passphrase.as_deref())?;
            authenticated = try_pubkey(&mut handle, &cfg.user, key).await?;
        }
    }

    if !authenticated && cfg.agent {
        authenticated = try_agent(&mut handle, &cfg.user).await?;
    }

    if !authenticated {
        let _ = handle
            .disconnect(Disconnect::ByApplication, "auth failed", "en")
            .await;
        return Err(SshError::AuthFailed);
    }

    let id = alloc_id();
    sessions()
        .lock()
        .map_err(|_| SshError::msg("session lock poisoned"))?
        .insert(
            id,
            Arc::new(SessionState {
                handle: Mutex::new(handle),
                alive: AtomicBool::new(true),
            }),
        );
    Ok(id)
}

async fn try_pubkey(
    handle: &mut Handle<AcceptAll>,
    user: &str,
    key: russh::keys::PrivateKey,
) -> SshResult<bool> {
    let hash = handle.best_supported_rsa_hash().await?.flatten();
    let res = handle
        .authenticate_publickey(user, PrivateKeyWithHashAlg::new(arc_key(key), hash))
        .await?;
    Ok(res.success())
}

async fn try_agent(handle: &mut Handle<AcceptAll>, user: &str) -> SshResult<bool> {
    let mut agent = match connect_agent().await {
        Ok(a) => a,
        Err(_) => return Ok(false),
    };
    let identities = agent
        .request_identities()
        .await
        .map_err(|e| SshError::Agent(e.to_string()))?;
    let hash = handle.best_supported_rsa_hash().await?.flatten();
    for identity in identities {
        let res: AuthResult = handle
            .authenticate_publickey_with(user, identity, hash, &mut agent)
            .await
            .map_err(|e| SshError::Agent(format!("{e:?}")))?;
        if res.success() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Whether `session` is still open.
pub fn is_connected(session: i64) -> bool {
    sessions()
        .lock()
        .ok()
        .and_then(|m| m.get(&session).map(|s| s.alive.load(Ordering::Relaxed)))
        .unwrap_or(false)
}

/// Disconnect and drop the session (and owned shells/sftp/forwards).
pub fn close(session: i64) -> SshResult<()> {
    block_on(close_async(session))
}

async fn close_async(session: i64) -> SshResult<()> {
    {
        let mut fw = forwards()
            .lock()
            .map_err(|_| SshError::msg("forward lock poisoned"))?;
        let ids: Vec<i64> = fw
            .iter()
            .filter(|(_, f)| f.session == session)
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            if let Some(mut f) = fw.remove(&id) {
                if let Some(tx) = f.abort.take() {
                    let _ = tx.send(());
                }
            }
        }
    }
    {
        let mut sh = shells()
            .lock()
            .map_err(|_| SshError::msg("shell lock poisoned"))?;
        sh.retain(|_, s| s.session != session);
    }
    {
        let mut sf = sftps()
            .lock()
            .map_err(|_| SshError::msg("sftp lock poisoned"))?;
        sf.retain(|_, s| s.session != session);
    }

    let state = {
        let mut map = sessions()
            .lock()
            .map_err(|_| SshError::msg("session lock poisoned"))?;
        map.remove(&session)
            .ok_or(SshError::InvalidHandle(session))?
    };
    state.alive.store(false, Ordering::Relaxed);
    let handle = state.handle.lock().await;
    let _ = handle
        .disconnect(Disconnect::ByApplication, "nssh close", "en")
        .await;
    Ok(())
}

// ---------------------------------------------------------------------------
// Exec
// ---------------------------------------------------------------------------

/// Run a remote command to completion.
pub fn exec(session: i64, command: &str, timeout_ms: Option<u64>) -> SshResult<ExecResult> {
    block_on(exec_async(session, command.to_string(), timeout_ms))
}

async fn exec_async(
    session: i64,
    command: String,
    timeout_ms: Option<u64>,
) -> SshResult<ExecResult> {
    let state = get_session(session)?;
    let fut = async {
        let handle = state.handle.lock().await;
        let mut channel = handle.channel_open_session().await?;
        drop(handle); // allow other ops while channel runs
        channel.exec(true, command.as_str()).await?;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut exit_status = None;
        loop {
            match channel.wait().await {
                None => break,
                Some(ChannelMsg::Data { ref data }) => stdout.extend_from_slice(data),
                Some(ChannelMsg::ExtendedData { ref data, ext }) => {
                    if ext == 1 {
                        stderr.extend_from_slice(data);
                    }
                }
                Some(ChannelMsg::ExitStatus { exit_status: code }) => {
                    exit_status = Some(code);
                }
                Some(_) => {}
            }
        }
        let code = exit_status.unwrap_or(0);
        Ok::<_, SshError>(ExecResult {
            ok: code == 0,
            exit_status: code,
            stdout,
            stderr,
        })
    };
    match timeout_ms {
        Some(ms) => timeout(Duration::from_millis(ms.max(1)), fut)
            .await
            .map_err(|_| SshError::Timeout)?,
        None => fut.await,
    }
}

// ---------------------------------------------------------------------------
// Interactive shell
// ---------------------------------------------------------------------------

/// Start an interactive shell; returns a channel handle.
pub fn shell_open(session: i64, term: &str, cols: u32, rows: u32) -> SshResult<i64> {
    block_on(shell_open_async(
        session,
        term.to_string(),
        cols.max(1),
        rows.max(1),
    ))
}

async fn shell_open_async(session: i64, term: String, cols: u32, rows: u32) -> SshResult<i64> {
    let state = get_session(session)?;
    let handle = state.handle.lock().await;
    let channel = handle.channel_open_session().await?;
    drop(handle);
    channel
        .request_pty(false, &term, cols, rows, 0, 0, &[])
        .await?;
    channel.request_shell(true).await?;
    let id = alloc_id();
    shells()
        .lock()
        .map_err(|_| SshError::msg("shell lock poisoned"))?
        .insert(
            id,
            Arc::new(ShellState {
                channel: Mutex::new(channel),
                buf: Mutex::new(Vec::new()),
                eof: AtomicBool::new(false),
                session,
            }),
        );
    Ok(id)
}

/// Write bytes to a shell channel.
pub fn shell_write(channel: i64, data: &[u8]) -> SshResult<()> {
    block_on(shell_write_async(channel, data.to_vec()))
}

async fn shell_write_async(channel: i64, data: Vec<u8>) -> SshResult<()> {
    let state = get_shell(channel)?;
    let ch = state.channel.lock().await;
    ch.data(&data[..]).await?;
    Ok(())
}

/// Read available shell data. Returns `None` on EOF.
pub fn shell_read(
    channel: i64,
    timeout_ms: Option<u64>,
    max_bytes: usize,
) -> SshResult<Option<Vec<u8>>> {
    block_on(shell_read_async(channel, timeout_ms, max_bytes.max(1)))
}

async fn shell_read_async(
    channel: i64,
    timeout_ms: Option<u64>,
    max_bytes: usize,
) -> SshResult<Option<Vec<u8>>> {
    let state = get_shell(channel)?;
    let deadline = timeout_ms.map(|ms| Instant::now() + Duration::from_millis(ms.max(1)));

    loop {
        {
            let mut buf = state.buf.lock().await;
            if !buf.is_empty() {
                let n = buf.len().min(max_bytes);
                return Ok(Some(buf.drain(..n).collect()));
            }
        }
        if state.eof.load(Ordering::Relaxed) {
            return Ok(None);
        }

        if let Some(dl) = deadline {
            if Instant::now() >= dl {
                return Ok(Some(Vec::new())); // timeout → empty read (no EOF)
            }
        }

        let wait = async {
            let mut ch = state.channel.lock().await;
            ch.wait().await
        };

        let msg = if let Some(dl) = deadline {
            let left = dl.saturating_duration_since(Instant::now());
            match timeout(left, wait).await {
                Ok(m) => m,
                Err(_) => return Ok(Some(Vec::new())),
            }
        } else {
            wait.await
        };

        match msg {
            None => {
                state.eof.store(true, Ordering::Relaxed);
                return Ok(None);
            }
            Some(ChannelMsg::Data { ref data }) => {
                let mut buf = state.buf.lock().await;
                buf.extend_from_slice(data);
            }
            Some(ChannelMsg::Eof) => {
                state.eof.store(true, Ordering::Relaxed);
            }
            Some(_) => {}
        }
    }
}

/// Close a shell channel.
pub fn shell_close(channel: i64) -> SshResult<()> {
    block_on(shell_close_async(channel))
}

async fn shell_close_async(channel: i64) -> SshResult<()> {
    let state = {
        shells()
            .lock()
            .map_err(|_| SshError::msg("shell lock poisoned"))?
            .remove(&channel)
            .ok_or(SshError::InvalidHandle(channel))?
    };
    let ch = state.channel.lock().await;
    let _ = ch.eof().await;
    let _ = ch.close().await;
    Ok(())
}

// ---------------------------------------------------------------------------
// SFTP
// ---------------------------------------------------------------------------

/// Open an SFTP subsystem on the session.
pub fn sftp_open(session: i64) -> SshResult<i64> {
    block_on(sftp_open_async(session))
}

async fn sftp_open_async(session: i64) -> SshResult<i64> {
    let state = get_session(session)?;
    let handle = state.handle.lock().await;
    let channel = handle.channel_open_session().await?;
    drop(handle);
    channel.request_subsystem(true, "sftp").await?;
    let sftp = SftpSession::new(channel.into_stream()).await?;
    let id = alloc_id();
    sftps()
        .lock()
        .map_err(|_| SshError::msg("sftp lock poisoned"))?
        .insert(id, Arc::new(SftpState { sftp, session }));
    Ok(id)
}

/// Close an SFTP handle.
pub fn sftp_close(sftp: i64) -> SshResult<()> {
    block_on(sftp_close_async(sftp))
}

async fn sftp_close_async(sftp: i64) -> SshResult<()> {
    let state = sftps()
        .lock()
        .map_err(|_| SshError::msg("sftp lock poisoned"))?
        .remove(&sftp)
        .ok_or(SshError::InvalidHandle(sftp))?;
    let _ = state.sftp.close().await;
    Ok(())
}

/// List directory entries.
pub fn sftp_listdir(sftp: i64, path: &str) -> SshResult<Vec<SftpEntry>> {
    block_on(sftp_listdir_async(sftp, path.to_string()))
}

async fn sftp_listdir_async(sftp: i64, path: String) -> SshResult<Vec<SftpEntry>> {
    let state = get_sftp(sftp)?;
    let mut out = Vec::new();
    let dir = state.sftp.read_dir(path).await?;
    for entry in dir {
        let meta = entry.metadata();
        let ft = entry.file_type();
        out.push(SftpEntry {
            name: entry.file_name(),
            size: meta.size.unwrap_or(0),
            is_dir: ft.is_dir(),
            is_file: ft.is_file(),
        });
    }
    Ok(out)
}

/// Stat a remote path.
pub fn sftp_stat(sftp: i64, path: &str) -> SshResult<SftpStat> {
    block_on(sftp_stat_async(sftp, path.to_string()))
}

async fn sftp_stat_async(sftp: i64, path: String) -> SshResult<SftpStat> {
    let state = get_sftp(sftp)?;
    let meta = state.sftp.metadata(path).await?;
    let ft = meta.file_type();
    Ok(SftpStat {
        size: meta.size.unwrap_or(0),
        is_dir: ft.is_dir(),
        is_file: ft.is_file(),
        permissions: meta.permissions,
    })
}

/// Read a remote file into memory.
pub fn sftp_read(sftp: i64, path: &str) -> SshResult<Vec<u8>> {
    block_on(async {
        let state = get_sftp(sftp)?;
        Ok(state.sftp.read(path.to_string()).await?)
    })
}

/// Write bytes to a remote file (create/truncate).
pub fn sftp_write(sftp: i64, path: &str, data: &[u8]) -> SshResult<()> {
    block_on(sftp_write_async(sftp, path.to_string(), data.to_vec()))
}

async fn sftp_write_async(sftp: i64, path: String, data: Vec<u8>) -> SshResult<()> {
    use tokio::io::AsyncWriteExt;
    let state = get_sftp(sftp)?;
    // `SftpSession::write` opens with WRITE only (no CREATE); use create() for trunc+write.
    let mut file = state.sftp.create(path).await?;
    file.write_all(&data).await.map_err(SshError::from)?;
    file.flush().await.map_err(SshError::from)?;
    Ok(())
}

/// Create a remote directory.
pub fn sftp_mkdir(sftp: i64, path: &str) -> SshResult<()> {
    block_on(async {
        let state = get_sftp(sftp)?;
        state.sftp.create_dir(path.to_string()).await?;
        Ok(())
    })
}

/// Remove an empty remote directory.
pub fn sftp_rmdir(sftp: i64, path: &str) -> SshResult<()> {
    block_on(async {
        let state = get_sftp(sftp)?;
        state.sftp.remove_dir(path.to_string()).await?;
        Ok(())
    })
}

/// Remove a remote file.
pub fn sftp_remove(sftp: i64, path: &str) -> SshResult<()> {
    block_on(async {
        let state = get_sftp(sftp)?;
        state.sftp.remove_file(path.to_string()).await?;
        Ok(())
    })
}

/// Rename a remote path.
pub fn sftp_rename(sftp: i64, src: &str, dst: &str) -> SshResult<()> {
    block_on(async {
        let state = get_sftp(sftp)?;
        state.sftp.rename(src.to_string(), dst.to_string()).await?;
        Ok(())
    })
}

/// Download remote → local filesystem path.
pub fn sftp_get(sftp: i64, remote: &str, local: &str) -> SshResult<()> {
    let data = sftp_read(sftp, remote)?;
    std::fs::write(local, data)?;
    Ok(())
}

/// Upload local filesystem path → remote.
pub fn sftp_put(sftp: i64, local: &str, remote: &str) -> SshResult<()> {
    let data = std::fs::read(local)?;
    sftp_write(sftp, remote, &data)
}

// ---------------------------------------------------------------------------
// Local port forward
// ---------------------------------------------------------------------------

/// Listen on `127.0.0.1:bind_port` and forward to `remote_host:remote_port`
/// through the SSH session. `bind_port == 0` picks an ephemeral port.
pub fn forward_local(
    session: i64,
    bind_port: u16,
    remote_host: &str,
    remote_port: u16,
) -> SshResult<ForwardInfo> {
    block_on(forward_local_async(
        session,
        bind_port,
        remote_host.to_string(),
        remote_port,
    ))
}

async fn forward_local_async(
    session: i64,
    bind_port: u16,
    remote_host: String,
    remote_port: u16,
) -> SshResult<ForwardInfo> {
    let state = get_session(session)?;
    let listener = TcpListener::bind(("127.0.0.1", bind_port)).await?;
    let bind_addr = listener.local_addr()?;
    let (abort_tx, mut abort_rx) = oneshot::channel::<()>();
    let id = alloc_id();

    let session_state = state.clone();
    spawn(async move {
        loop {
            tokio::select! {
                _ = &mut abort_rx => break,
                acc = listener.accept() => {
                    let Ok((mut sock, peer)) = acc else { break };
                    let sess = session_state.clone();
                    let rh = remote_host.clone();
                    spawn(async move {
                        let handle = sess.handle.lock().await;
                        let channel = match handle
                            .channel_open_direct_tcpip(
                                rh,
                                remote_port as u32,
                                peer.ip().to_string(),
                                peer.port() as u32,
                            )
                            .await
                        {
                            Ok(c) => c,
                            Err(_) => return,
                        };
                        drop(handle);
                        let mut stream = channel.into_stream();
                        let _ = tokio::io::copy_bidirectional(&mut sock, &mut stream).await;
                    });
                }
            }
        }
    });

    forwards()
        .lock()
        .map_err(|_| SshError::msg("forward lock poisoned"))?
        .insert(
            id,
            ForwardState {
                abort: Some(abort_tx),
                bind_addr,
                session,
            },
        );

    Ok(ForwardInfo {
        id,
        bind_port: bind_addr.port(),
        bind_addr: bind_addr.to_string(),
    })
}

/// Stop a local forward listener.
pub fn forward_close(forward: i64) -> SshResult<()> {
    let mut fw = forwards()
        .lock()
        .map_err(|_| SshError::msg("forward lock poisoned"))?;
    let mut state = fw
        .remove(&forward)
        .ok_or(SshError::InvalidHandle(forward))?;
    if let Some(tx) = state.abort.take() {
        let _ = tx.send(());
    }
    Ok(())
}

/// Actual bound address for a forward handle.
pub fn forward_addr(forward: i64) -> SshResult<String> {
    let fw = forwards()
        .lock()
        .map_err(|_| SshError::msg("forward lock poisoned"))?;
    let state = fw.get(&forward).ok_or(SshError::InvalidHandle(forward))?;
    Ok(state.bind_addr.to_string())
}
