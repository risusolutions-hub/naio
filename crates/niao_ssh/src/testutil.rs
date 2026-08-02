//! Embedded SSH server for integration tests and local benchmarks.

use rand_core::OsRng;
use russh::keys::{PrivateKey, PublicKey};
use russh::server::{Auth, Msg, Server as _, Session};
use russh::{Channel, ChannelId, CryptoVec};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

const USER: &str = "testuser";
const PASS: &str = "testpass";

/// Running test server — drop to tear down (task continues until process ends; use port only).
pub struct TestServer {
    pub addr: SocketAddr,
    pub host_key: PrivateKey,
    pub client_key: PrivateKey,
    _join: tokio::task::JoinHandle<()>,
}

impl TestServer {
    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    pub fn host(&self) -> String {
        "127.0.0.1".into()
    }
}

/// Start a password+pubkey SSH server on `127.0.0.1:0`.
pub async fn start_test_server() -> TestServer {
    let host_key = PrivateKey::random(&mut OsRng, russh::keys::Algorithm::Ed25519).unwrap();
    let client_key = PrivateKey::random(&mut OsRng, russh::keys::Algorithm::Ed25519).unwrap();
    let accepted_pubkey = client_key.public_key().clone();

    let config = russh::server::Config {
        inactivity_timeout: Some(Duration::from_secs(60)),
        auth_rejection_time: Duration::from_millis(10),
        auth_rejection_time_initial: Some(Duration::from_millis(0)),
        keys: vec![host_key.clone()],
        ..Default::default()
    };
    let config = Arc::new(config);
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let addr = listener.local_addr().unwrap();

    let mut server = TestSshServer {
        accepted_pubkey,
        clients: Arc::new(Mutex::new(HashMap::new())),
    };
    let join = tokio::spawn(async move {
        let _ = server.run_on_socket(config, &listener).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    TestServer {
        addr,
        host_key,
        client_key,
        _join: join,
    }
}

#[derive(Clone)]
struct TestSshServer {
    accepted_pubkey: PublicKey,
    clients: Arc<Mutex<HashMap<ChannelId, Channel<Msg>>>>,
}

impl russh::server::Server for TestSshServer {
    type Handler = Handler;
    fn new_client(&mut self, _: Option<SocketAddr>) -> Self::Handler {
        Handler {
            accepted_pubkey: self.accepted_pubkey.clone(),
            clients: self.clients.clone(),
        }
    }
}

struct Handler {
    accepted_pubkey: PublicKey,
    clients: Arc<Mutex<HashMap<ChannelId, Channel<Msg>>>>,
}

impl russh::server::Handler for Handler {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        if user == USER && password == PASS {
            Ok(Auth::Accept)
        } else {
            Ok(Auth::Reject {
                proceed_with_methods: None,
                partial_success: false,
            })
        }
    }

    async fn auth_publickey(&mut self, user: &str, key: &PublicKey) -> Result<Auth, Self::Error> {
        if user == USER && key == &self.accepted_pubkey {
            Ok(Auth::Accept)
        } else {
            Ok(Auth::Reject {
                proceed_with_methods: None,
                partial_success: false,
            })
        }
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        self.clients.lock().await.insert(channel.id(), channel);
        Ok(true)
    }

    async fn channel_open_direct_tcpip(
        &mut self,
        channel: Channel<Msg>,
        host_to_connect: &str,
        port_to_connect: u32,
        _originator_address: &str,
        _originator_port: u32,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        let addr = format!("{host_to_connect}:{port_to_connect}");
        let Ok(mut tcp) = tokio::net::TcpStream::connect(&addr).await else {
            return Ok(false);
        };
        let mut stream = channel.into_stream();
        tokio::spawn(async move {
            let _ = tokio::io::copy_bidirectional(&mut tcp, &mut stream).await;
        });
        Ok(true)
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        // Echo shell data.
        session.data(channel, CryptoVec::from(data.to_vec()))?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        session.data(channel, CryptoVec::from(b"$ ".to_vec()))?;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let cmd = String::from_utf8_lossy(data);
        let (stdout, stderr, code) = handle_exec(&cmd);
        if !stdout.is_empty() {
            session.data(channel, CryptoVec::from(stdout))?;
        }
        if !stderr.is_empty() {
            session.extended_data(channel, 1, CryptoVec::from(stderr))?;
        }
        session.exit_status_request(channel, code)?;
        session.eof(channel)?;
        session.close(channel)?;
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if name == "sftp" {
            let ch = {
                let mut clients = self.clients.lock().await;
                clients.remove(&channel)
            };
            if let Some(ch) = ch {
                session.channel_success(channel)?;
                let sftp = MemSftp::default();
                tokio::spawn(async move {
                    russh_sftp::server::run(ch.into_stream(), sftp).await;
                });
            } else {
                session.channel_failure(channel)?;
            }
        } else {
            session.channel_failure(channel)?;
        }
        Ok(())
    }
}

fn handle_exec(cmd: &str) -> (Vec<u8>, Vec<u8>, u32) {
    let cmd = cmd.trim();
    if let Some(rest) = cmd.strip_prefix("echo ") {
        (format!("{rest}\n").into_bytes(), Vec::new(), 0)
    } else if let Some(code) = cmd.strip_prefix("exit ") {
        let code: u32 = code.trim().parse().unwrap_or(1);
        (Vec::new(), Vec::new(), code)
    } else if cmd == "stderr-msg" {
        (Vec::new(), b"err-line\n".to_vec(), 0)
    } else if cmd.is_empty() {
        (Vec::new(), Vec::new(), 0)
    } else {
        (format!("{cmd}\n").into_bytes(), Vec::new(), 0)
    }
}

// ---------------------------------------------------------------------------
// Minimal in-memory SFTP
// ---------------------------------------------------------------------------

use russh_sftp::protocol::{
    File, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode, Version,
};
use std::collections::BTreeMap;

#[derive(Default)]
struct MemSftp {
    version: Option<u32>,
    files: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    dirs: Arc<Mutex<BTreeMap<String, ()>>>,
    handles: Arc<Mutex<HashMap<String, OpenHandle>>>,
    dir_pos: Arc<Mutex<HashMap<String, usize>>>,
}

enum OpenHandle {
    File { path: String },
    Dir { path: String },
}

impl MemSftp {
    fn normalize(path: &str) -> String {
        let p = path.trim_end_matches('/');
        if p.is_empty() || p == "." {
            "/".into()
        } else if p.starts_with('/') {
            p.to_string()
        } else {
            format!("/{p}")
        }
    }
}

impl russh_sftp::server::Handler for MemSftp {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn init(
        &mut self,
        version: u32,
        _extensions: HashMap<String, String>,
    ) -> Result<Version, Self::Error> {
        self.version = Some(version);
        {
            let mut d = self.dirs.lock().await;
            d.insert("/".into(), ());
        }
        Ok(Version::new())
    }

    async fn close(&mut self, id: u32, handle: String) -> Result<Status, Self::Error> {
        self.handles.lock().await.remove(&handle);
        self.dir_pos.lock().await.remove(&handle);
        Ok(ok_status(id))
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
        let path = Self::normalize(&path);
        if !self.dirs.lock().await.contains_key(&path) {
            return Err(StatusCode::NoSuchFile);
        }
        let h = format!("dir:{}", id);
        self.handles
            .lock()
            .await
            .insert(h.clone(), OpenHandle::Dir { path });
        self.dir_pos.lock().await.insert(h.clone(), 0);
        Ok(Handle { id, handle: h })
    }

    async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, Self::Error> {
        let path = {
            let handles = self.handles.lock().await;
            match handles.get(&handle) {
                Some(OpenHandle::Dir { path }) => path.clone(),
                _ => return Err(StatusCode::Failure),
            }
        };
        let mut pos_map = self.dir_pos.lock().await;
        let pos = pos_map.entry(handle.clone()).or_insert(0);
        if *pos > 0 {
            return Err(StatusCode::Eof);
        }
        *pos = 1;
        let files = self.files.lock().await;
        let dirs = self.dirs.lock().await;
        let mut entries = Vec::new();
        let prefix = if path == "/" { "/" } else { path.as_str() };
        for (p, data) in files.iter() {
            if parent_of(p) == prefix || (prefix == "/" && parent_of(p) == "/") {
                let name = leaf(p);
                if name == "." || name == ".." {
                    continue;
                }
                let mut attrs = FileAttributes::default();
                attrs.size = Some(data.len() as u64);
                attrs.permissions = Some(0o100644);
                entries.push(File::new(name, attrs));
            }
        }
        for d in dirs.keys() {
            if d == "/" {
                continue;
            }
            if parent_of(d) == prefix || (prefix == "/" && parent_of(d) == "/") {
                let name = leaf(d);
                let mut attrs = FileAttributes::default();
                attrs.permissions = Some(0o040755);
                entries.push(File::new(name, attrs));
            }
        }
        Ok(Name { id, files: entries })
    }

    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        let p = Self::normalize(&path);
        Ok(Name {
            id,
            files: vec![File::dummy(&p)],
        })
    }

    async fn mkdir(
        &mut self,
        id: u32,
        path: String,
        _attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        let path = Self::normalize(&path);
        self.dirs.lock().await.insert(path, ());
        Ok(ok_status(id))
    }

    async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, Self::Error> {
        let path = Self::normalize(&path);
        self.dirs.lock().await.remove(&path);
        Ok(ok_status(id))
    }

    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, Self::Error> {
        let path = Self::normalize(&filename);
        self.files.lock().await.remove(&path);
        Ok(ok_status(id))
    }

    async fn rename(
        &mut self,
        id: u32,
        oldpath: String,
        newpath: String,
    ) -> Result<Status, Self::Error> {
        let old = Self::normalize(&oldpath);
        let new = Self::normalize(&newpath);
        let mut files = self.files.lock().await;
        if let Some(data) = files.remove(&old) {
            files.insert(new, data);
            return Ok(ok_status(id));
        }
        let mut dirs = self.dirs.lock().await;
        if dirs.remove(&old).is_some() {
            dirs.insert(new, ());
            return Ok(ok_status(id));
        }
        Err(StatusCode::NoSuchFile)
    }

    async fn open(
        &mut self,
        id: u32,
        filename: String,
        pflags: OpenFlags,
        _attrs: FileAttributes,
    ) -> Result<Handle, Self::Error> {
        let path = Self::normalize(&filename);
        let create = pflags.contains(OpenFlags::CREATE)
            || pflags.contains(OpenFlags::WRITE)
            || pflags.contains(OpenFlags::TRUNCATE);
        let trunc = pflags.contains(OpenFlags::TRUNCATE) || pflags.contains(OpenFlags::CREATE);
        let mut files = self.files.lock().await;
        if create {
            // Ensure parent directory entries exist for listdir.
            if let Some(parent) = path.rsplit_once('/').map(|(p, _)| p) {
                let parent = if parent.is_empty() { "/" } else { parent };
                self.dirs.lock().await.insert(parent.to_string(), ());
            }
            files.entry(path.clone()).or_insert_with(Vec::new);
            if trunc {
                if let Some(v) = files.get_mut(&path) {
                    v.clear();
                }
            }
        } else if !files.contains_key(&path) {
            return Err(StatusCode::NoSuchFile);
        }
        let h = format!("file:{}:{}", id, path);
        self.handles
            .lock()
            .await
            .insert(h.clone(), OpenHandle::File { path });
        Ok(Handle { id, handle: h })
    }

    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> Result<russh_sftp::protocol::Data, Self::Error> {
        let path = {
            let handles = self.handles.lock().await;
            match handles.get(&handle) {
                Some(OpenHandle::File { path, .. }) => path.clone(),
                _ => return Err(StatusCode::Failure),
            }
        };
        let files = self.files.lock().await;
        let data = files.get(&path).ok_or(StatusCode::NoSuchFile)?;
        let start = offset as usize;
        if start >= data.len() {
            return Err(StatusCode::Eof);
        }
        let end = (start + len as usize).min(data.len());
        Ok(russh_sftp::protocol::Data {
            id,
            data: data[start..end].to_vec(),
        })
    }

    async fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, Self::Error> {
        let path = {
            let handles = self.handles.lock().await;
            match handles.get(&handle) {
                Some(OpenHandle::File { path, .. }) => path.clone(),
                _ => return Err(StatusCode::Failure),
            }
        };
        let mut files = self.files.lock().await;
        let file = files.entry(path).or_default();
        let start = offset as usize;
        if file.len() < start {
            file.resize(start, 0);
        }
        let end = start + data.len();
        if file.len() < end {
            file.resize(end, 0);
        }
        file[start..end].copy_from_slice(&data);
        Ok(ok_status(id))
    }

    async fn stat(
        &mut self,
        id: u32,
        path: String,
    ) -> Result<russh_sftp::protocol::Attrs, Self::Error> {
        self.lstat(id, path).await
    }

    async fn lstat(
        &mut self,
        id: u32,
        path: String,
    ) -> Result<russh_sftp::protocol::Attrs, Self::Error> {
        let path = Self::normalize(&path);
        if let Some(data) = self.files.lock().await.get(&path) {
            let mut attrs = FileAttributes::default();
            attrs.size = Some(data.len() as u64);
            attrs.permissions = Some(0o100644);
            return Ok(russh_sftp::protocol::Attrs { id, attrs });
        }
        if self.dirs.lock().await.contains_key(&path) {
            let mut attrs = FileAttributes::default();
            attrs.permissions = Some(0o040755);
            return Ok(russh_sftp::protocol::Attrs { id, attrs });
        }
        Err(StatusCode::NoSuchFile)
    }
}

fn ok_status(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: "Ok".into(),
        language_tag: "en-US".into(),
    }
}

fn parent_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) => "/",
        Some(i) => &path[..i],
        None => "/",
    }
}

fn leaf(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

pub const TEST_USER: &str = USER;
pub const TEST_PASS: &str = PASS;
