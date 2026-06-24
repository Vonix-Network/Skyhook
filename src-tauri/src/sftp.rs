use crate::error::{Result, SkyhookError};
use crate::vault::{AuthMethod, Connection};
use async_trait::async_trait;
use russh::client;
use russh::keys::{decode_secret_key, key};
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::OpenFlags;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;

/// Normalize a remote SFTP path:
/// - Collapse runs of `/` into a single `/`
/// - Strip trailing `/` (except for root)
/// - Empty input becomes "/"
/// Wings/Pterodactyl SFTP rejects malformed paths like `//foo` and closes
/// the channel; OpenSSH tolerates them. Be safe everywhere.
pub fn normalize_remote_path(p: &str) -> String {
    if p.is_empty() {
        return "/".into();
    }
    let mut out = String::with_capacity(p.len());
    let mut prev_slash = false;
    for c in p.chars() {
        if c == '/' {
            if !prev_slash {
                out.push('/');
            }
            prev_slash = true;
        } else {
            out.push(c);
            prev_slash = false;
        }
    }
    // strip trailing slash unless root
    if out.len() > 1 && out.ends_with('/') {
        out.pop();
    }
    out
}

/// Join a remote directory and a name, normalized.
pub fn join_remote(dir: &str, name: &str) -> String {
    let combined = if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    };
    normalize_remote_path(&combined)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub modified: Option<i64>,
    pub mode: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionStatus {
    pub id: String,
    pub connection_id: String,
    pub connected: bool,
    pub cwd: String,
}

struct ClientHandler;

#[async_trait]
impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &key::PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        // TODO v2: trust-on-first-use known_hosts. v1 accepts all (warn user in UI).
        Ok(true)
    }
}

pub struct Session {
    pub id: String,
    pub connection_id: String,
    pub cwd: Mutex<String>,
    pub alive: AtomicBool,
    handle: Mutex<client::Handle<ClientHandler>>,
    sftp: Mutex<SftpSession>,
}

/// Returns true if an SFTP/SSH error looks like the channel/connection died
/// (vs. just a normal failure like ENOENT or EACCES). When this happens, the
/// session is unrecoverable and the UI should prompt to reconnect.
fn is_fatal_channel_error(msg: &str) -> bool {
    let m = msg.to_ascii_lowercase();
    m.contains("connection lost")
        || m.contains("disconnected")
        || m.contains("channel")
            && (m.contains("closed") || m.contains("eof") || m.contains("broken"))
        || m.contains("broken pipe")
        || m.contains("not connected")
        || m.contains("eof")
}

impl Session {
    fn mark_dead_if_fatal(&self, err: &SkyhookError) {
        let msg = err.to_string();
        if is_fatal_channel_error(&msg) {
            self.alive.store(false, Ordering::Relaxed);
        }
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }
    pub async fn connect(conn: &Connection) -> Result<Self> {
        let config = Arc::new(client::Config {
            inactivity_timeout: Some(std::time::Duration::from_secs(60)),
            ..Default::default()
        });

        let sh = ClientHandler;
        let addr = (conn.host.as_str(), conn.port);
        let mut handle = client::connect(config, addr, sh)
            .await
            .map_err(|e| SkyhookError::Ssh(format!("connect: {e}")))?;

        let authenticated = match &conn.auth {
            AuthMethod::Password { password } => handle
                .authenticate_password(conn.username.clone(), password.clone())
                .await
                .map_err(|e| SkyhookError::Ssh(format!("auth: {e}")))?,
            AuthMethod::Key {
                private_key,
                passphrase,
            } => {
                let kp = decode_secret_key(private_key, passphrase.as_deref())
                    .map_err(|e| SkyhookError::Ssh(format!("key parse: {e}")))?;
                handle
                    .authenticate_publickey(conn.username.clone(), Arc::new(kp))
                    .await
                    .map_err(|e| SkyhookError::Ssh(format!("auth: {e}")))?
            }
            AuthMethod::Agent => {
                #[cfg(unix)]
                {
                    let mut agent = russh::keys::agent::client::AgentClient::connect_env()
                        .await
                        .map_err(|e| SkyhookError::Ssh(format!("agent: {e}")))?;
                    let identities = agent
                        .request_identities()
                        .await
                        .map_err(|e| SkyhookError::Ssh(format!("agent ids: {e}")))?;
                    let mut ok = false;
                    for pubkey in identities {
                        let (agent_back, res) = handle
                            .authenticate_future(conn.username.clone(), pubkey, agent)
                            .await;
                        agent = agent_back;
                        if matches!(res, Ok(true)) {
                            ok = true;
                            break;
                        }
                    }
                    ok
                }
                #[cfg(not(unix))]
                {
                    return Err(SkyhookError::Ssh(
                        "SSH agent auth on Windows is not yet supported — use key or password".into(),
                    ));
                }
            }
        };

        if !authenticated {
            return Err(SkyhookError::AuthFailed);
        }

        let channel = handle
            .channel_open_session()
            .await
            .map_err(|e| SkyhookError::Ssh(format!("channel: {e}")))?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|e| SkyhookError::Ssh(format!("subsystem: {e}")))?;
        let sftp = SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| SkyhookError::Sftp(format!("init: {e}")))?;

        let cwd = sftp
            .canonicalize(conn.default_path.clone().unwrap_or_else(|| ".".into()))
            .await
            .unwrap_or_else(|_| "/".into());

        Ok(Self {
            id: uuid::Uuid::new_v4().to_string(),
            connection_id: conn.id.clone(),
            cwd: Mutex::new(cwd),
            alive: AtomicBool::new(true),
            handle: Mutex::new(handle),
            sftp: Mutex::new(sftp),
        })
    }

    pub async fn list_dir(&self, path: &str) -> Result<Vec<DirEntry>> {
        let sftp = self.sftp.lock().await;
        let path = normalize_remote_path(path);
        let resolved = sftp.canonicalize(path).await.map_err(|e| {
            let err = SkyhookError::Sftp(e.to_string());
            self.mark_dead_if_fatal(&err);
            err
        })?;
        let resolved = normalize_remote_path(&resolved);
        let read_dir = sftp.read_dir(&resolved).await.map_err(|e| {
            let err = SkyhookError::Sftp(e.to_string());
            self.mark_dead_if_fatal(&err);
            err
        })?;
        let mut out = Vec::new();
        for entry in read_dir {
            let name = entry.file_name();
            if name == "." || name == ".." {
                continue;
            }
            let meta = entry.metadata();
            let full = join_remote(&resolved, &name);
            out.push(DirEntry {
                name,
                path: full,
                is_dir: meta.is_dir(),
                is_symlink: meta.is_symlink(),
                size: meta.size.unwrap_or(0),
                modified: meta.mtime.map(|t| t as i64),
                mode: meta.permissions,
            });
        }
        out.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        });
        *self.cwd.lock().await = resolved;
        Ok(out)
    }

    pub async fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        let sftp = self.sftp.lock().await;
        let path = normalize_remote_path(path);
        let mut f = sftp.open_with_flags(path, OpenFlags::READ).await.map_err(|e| {
            let err = SkyhookError::Sftp(e.to_string());
            self.mark_dead_if_fatal(&err);
            err
        })?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).await.map_err(|e| {
            let err = SkyhookError::Sftp(e.to_string());
            self.mark_dead_if_fatal(&err);
            err
        })?;
        Ok(buf)
    }

    pub async fn write_file(&self, path: &str, data: &[u8]) -> Result<()> {
        let sftp = self.sftp.lock().await;
        let path = normalize_remote_path(path);
        let mut f = sftp
            .open_with_flags(path, OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE)
            .await
            .map_err(|e| {
                let err = SkyhookError::Sftp(e.to_string());
                self.mark_dead_if_fatal(&err);
                err
            })?;
        f.write_all(data).await.map_err(|e| {
            let err = SkyhookError::Sftp(e.to_string());
            self.mark_dead_if_fatal(&err);
            err
        })?;
        f.shutdown().await.map_err(|e| {
            let err = SkyhookError::Sftp(e.to_string());
            self.mark_dead_if_fatal(&err);
            err
        })?;
        Ok(())
    }

    pub async fn download(&self, remote: &str, local: &std::path::Path) -> Result<u64> {
        let sftp = self.sftp.lock().await;
        let remote = normalize_remote_path(remote);
        let mut rf = sftp
            .open_with_flags(remote, OpenFlags::READ)
            .await
            .map_err(|e| {
                let err = SkyhookError::Sftp(e.to_string());
                self.mark_dead_if_fatal(&err);
                err
            })?;
        if let Some(parent) = local.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut lf = tokio::fs::File::create(local).await?;
        let n = tokio::io::copy(&mut rf, &mut lf).await?;
        lf.shutdown().await?;
        Ok(n)
    }

    pub async fn upload(&self, local: &std::path::Path, remote: &str) -> Result<u64> {
        let sftp = self.sftp.lock().await;
        let remote = normalize_remote_path(remote);
        let mut lf = tokio::fs::File::open(local).await?;
        let mut rf = sftp
            .open_with_flags(
                remote,
                OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE,
            )
            .await
            .map_err(|e| {
                let err = SkyhookError::Sftp(e.to_string());
                self.mark_dead_if_fatal(&err);
                err
            })?;
        let n = tokio::io::copy(&mut lf, &mut rf).await?;
        rf.shutdown().await.map_err(|e| {
            let err = SkyhookError::Sftp(e.to_string());
            self.mark_dead_if_fatal(&err);
            err
        })?;
        Ok(n)
    }

    pub async fn mkdir(&self, path: &str) -> Result<()> {
        let sftp = self.sftp.lock().await;
        let path = normalize_remote_path(path);
        sftp.create_dir(path).await.map_err(|e| {
            let err = SkyhookError::Sftp(e.to_string());
            self.mark_dead_if_fatal(&err);
            err
        })
    }

    pub async fn remove(&self, path: &str) -> Result<()> {
        let sftp = self.sftp.lock().await;
        let path = normalize_remote_path(path);
        let meta = sftp.metadata(&path).await.map_err(|e| {
            let err = SkyhookError::Sftp(e.to_string());
            self.mark_dead_if_fatal(&err);
            err
        })?;
        let res = if meta.is_dir() {
            sftp.remove_dir(&path).await
        } else {
            sftp.remove_file(&path).await
        };
        res.map_err(|e| {
            let err = SkyhookError::Sftp(e.to_string());
            self.mark_dead_if_fatal(&err);
            err
        })
    }

    pub async fn rename(&self, from: &str, to: &str) -> Result<()> {
        let sftp = self.sftp.lock().await;
        let from = normalize_remote_path(from);
        let to = normalize_remote_path(to);
        sftp.rename(from, to).await.map_err(|e| {
            let err = SkyhookError::Sftp(e.to_string());
            self.mark_dead_if_fatal(&err);
            err
        })
    }

    pub async fn disconnect(&self) -> Result<()> {
        let _ = self.sftp.lock().await;
        let h = self.handle.lock().await;
        let _ = h
            .disconnect(russh::Disconnect::ByApplication, "bye", "en")
            .await;
        Ok(())
    }
}

pub struct SessionRegistry {
    sessions: HashMap<String, Arc<Session>>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }
    pub fn insert(&mut self, s: Arc<Session>) -> String {
        let id = s.id.clone();
        self.sessions.insert(id.clone(), s);
        id
    }
    pub fn get(&self, id: &str) -> Option<Arc<Session>> {
        self.sessions.get(id).cloned()
    }
    pub fn remove(&mut self, id: &str) -> Option<Arc<Session>> {
        self.sessions.remove(id)
    }
    pub fn list(&self) -> Vec<SessionStatus> {
        self.sessions
            .values()
            .map(|s| {
                let cwd = s.cwd.try_lock().map(|g| g.clone()).unwrap_or_default();
                SessionStatus {
                    id: s.id.clone(),
                    connection_id: s.connection_id.clone(),
                    connected: s.is_alive(),
                    cwd,
                }
            })
            .collect()
    }
}
