//! [`SessionActor`] — owns one SSH transport + one SFTP channel, serializes
//! all SFTP operations through an mpsc queue.
//!
//! # Why an actor?
//!
//! v0.1.x guarded the `SftpSession` with a `Mutex` and allowed every Tauri
//! command to lock it directly. That worked for one op at a time but had two
//! latent bugs:
//!
//! 1. There was no single owner of the SSH `Handle` and the `SftpSession`, so
//!    teardown / reconnect had to coordinate across many call sites.
//! 2. Heartbeat and reconnect had no clean place to live: they would race
//!    against user-initiated ops on the same mutex.
//!
//! Switching to an actor fixes both: a single Tokio task owns the transport,
//! pulls one [`SessionOp`] off the queue at a time, and runs heartbeat /
//! reconnect inline using `tokio::select!`. The actor is the *only* code that
//! touches the SFTP channel.
//!
//! # State transitions
//!
//! See [`crate::session::state`] for the state diagram. Transitions happen in
//! exactly one place — [`SessionActor::set_state`] — so events are emitted
//! deterministically.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use russh::client;
use russh::keys::{decode_secret_key, key};
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::OpenFlags;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio::time::{interval, MissedTickBehavior};

use crate::error::{Result, SkyhookError};
use crate::sftp::{is_fatal_channel_error, join_remote, normalize_remote_path, DirEntry};
use crate::vault::{AuthMethod, Connection};

use super::heartbeat::{HEARTBEAT_FAIL_THRESHOLD, HEARTBEAT_INTERVAL};
use super::reconnect::RECONNECT_BACKOFF;
use super::state::{SessionInfo, SessionState};

const OP_QUEUE_DEPTH: usize = 64;

/// One request handled by [`SessionActor`].
///
/// Each variant carries a `oneshot::Sender` so callers get back exactly one
/// result and there is no shared mutable state outside the actor.
pub enum SessionOp {
    ListDir { path: String, resp: oneshot::Sender<Result<Vec<DirEntry>>> },
    Stat { path: String, resp: oneshot::Sender<Result<DirEntry>> },
    Walk { root: String, resp: oneshot::Sender<Result<Vec<DirEntry>>> },
    ReadFile { path: String, resp: oneshot::Sender<Result<Vec<u8>>> },
    WriteFile { path: String, data: Vec<u8>, resp: oneshot::Sender<Result<()>> },
    Download { remote: String, local: PathBuf, resp: oneshot::Sender<Result<u64>> },
    Upload { local: PathBuf, remote: String, resp: oneshot::Sender<Result<u64>> },
    Mkdir { path: String, resp: oneshot::Sender<Result<()>> },
    Remove { path: String, resp: oneshot::Sender<Result<()>> },
    Rename { from: String, to: String, resp: oneshot::Sender<Result<()>> },
    /// Force a reconnect now (user-initiated or recovery from Degraded).
    Reconnect { resp: oneshot::Sender<Result<()>> },
    /// Tear down the SSH transport and exit the actor task.
    Disconnect { resp: oneshot::Sender<Result<()>> },
}

/// Cheap, cloneable handle to one [`SessionActor`].
///
/// Holds the mpsc sender and a shared [`SessionInfo`] snapshot. The frontend
/// reaches the actor through these.
#[derive(Clone)]
pub struct SessionHandle {
    pub id: String,
    pub connection_id: String,
    tx: mpsc::Sender<SessionOp>,
    info: Arc<RwLock<SessionInfo>>,
}

impl SessionHandle {
    /// Current snapshot of session state — cheap to call from any task.
    pub async fn info(&self) -> SessionInfo {
        self.info.read().await.clone()
    }

    /// True while the actor task is still running and able to accept ops.
    pub async fn is_live(&self) -> bool {
        self.info.read().await.state.is_live()
    }

    async fn send(&self, op: SessionOp) -> Result<()> {
        self.tx
            .send(op)
            .await
            .map_err(|_| SkyhookError::Other("session actor stopped".into()))
    }

    async fn call<T, F>(&self, build: F) -> Result<T>
    where
        F: FnOnce(oneshot::Sender<Result<T>>) -> SessionOp,
    {
        let (tx, rx) = oneshot::channel();
        self.send(build(tx)).await?;
        rx.await
            .map_err(|_| SkyhookError::Other("session actor dropped response".into()))?
    }

    /// List the contents of a remote directory (canonicalized first).
    pub async fn list_dir(&self, path: String) -> Result<Vec<DirEntry>> {
        self.call(|resp| SessionOp::ListDir { path, resp }).await
    }

    /// Stat a single path.
    pub async fn stat(&self, path: String) -> Result<DirEntry> {
        self.call(|resp| SessionOp::Stat { path, resp }).await
    }

    /// Recursive depth-first walk (single-channel; slow on deep trees by design).
    pub async fn walk(&self, root: String) -> Result<Vec<DirEntry>> {
        self.call(|resp| SessionOp::Walk { root, resp }).await
    }

    /// Read a small remote file into memory.
    pub async fn read_file(&self, path: String) -> Result<Vec<u8>> {
        self.call(|resp| SessionOp::ReadFile { path, resp }).await
    }

    /// Overwrite a remote file with `data`.
    pub async fn write_file(&self, path: String, data: Vec<u8>) -> Result<()> {
        self.call(|resp| SessionOp::WriteFile { path, data, resp }).await
    }

    /// Stream `remote` down to `local`. Returns bytes copied.
    pub async fn download(&self, remote: String, local: PathBuf) -> Result<u64> {
        self.call(|resp| SessionOp::Download { remote, local, resp }).await
    }

    /// Stream `local` up to `remote`. Returns bytes copied.
    pub async fn upload(&self, local: PathBuf, remote: String) -> Result<u64> {
        self.call(|resp| SessionOp::Upload { local, remote, resp }).await
    }

    /// Create a directory at `path` (parents not auto-created).
    pub async fn mkdir(&self, path: String) -> Result<()> {
        self.call(|resp| SessionOp::Mkdir { path, resp }).await
    }

    /// Remove a file or directory at `path`.
    pub async fn remove(&self, path: String) -> Result<()> {
        self.call(|resp| SessionOp::Remove { path, resp }).await
    }

    /// Rename / move `from` to `to`.
    pub async fn rename(&self, from: String, to: String) -> Result<()> {
        self.call(|resp| SessionOp::Rename { from, to, resp }).await
    }

    /// Trigger an immediate reconnect attempt.
    pub async fn reconnect(&self) -> Result<()> {
        self.call(|resp| SessionOp::Reconnect { resp }).await
    }

    /// User-initiated teardown.
    pub async fn disconnect(&self) -> Result<()> {
        self.call(|resp| SessionOp::Disconnect { resp }).await
    }
}

/// russh client handler. Currently accepts any host key (TOFU known_hosts is
/// planned for v0.3).
struct ClientHandler;

#[async_trait]
impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &key::PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        Ok(true)
    }
}

/// The owning task for one SFTP session. Construct with [`SessionActor::spawn`].
pub struct SessionActor {
    id: String,
    connection: Connection,
    cwd: String,
    handle: Option<client::Handle<ClientHandler>>,
    sftp: Option<SftpSession>,
    info: Arc<RwLock<SessionInfo>>,
    app: AppHandle,
    heartbeat_failures: u8,
}

impl SessionActor {
    /// Build an actor + spawn its task. Returns a [`SessionHandle`] immediately;
    /// the actor finishes connecting in the background and emits
    /// `session-state-changed` on every transition.
    pub fn spawn(app: AppHandle, connection: Connection) -> SessionHandle {
        let id = uuid::Uuid::new_v4().to_string();
        let info = SessionInfo {
            id: id.clone(),
            connection_id: connection.id.clone(),
            state: SessionState::Connecting,
            cwd: "/".into(),
            reason: None,
        };
        let info = Arc::new(RwLock::new(info));
        let (tx, rx) = mpsc::channel(OP_QUEUE_DEPTH);

        let handle = SessionHandle {
            id: id.clone(),
            connection_id: connection.id.clone(),
            tx,
            info: info.clone(),
        };

        let actor = SessionActor {
            id,
            connection,
            cwd: "/".into(),
            handle: None,
            sftp: None,
            info,
            app,
            heartbeat_failures: 0,
        };

        tokio::spawn(actor.run(rx));
        handle
    }

    /// Emit `session-state-changed` and update the shared snapshot.
    async fn set_state(&mut self, state: SessionState, reason: Option<String>) {
        {
            let mut g = self.info.write().await;
            g.state = state;
            g.reason = reason.clone();
            g.cwd = self.cwd.clone();
        }
        let payload = serde_json::json!({
            "sessionId": self.id,
            "state": state,
            "reason": reason,
        });
        let _ = self.app.emit("session-state-changed", payload);
    }

    /// Open SSH + SFTP. Returns (Handle, SftpSession, cwd) on success.
    async fn dial(
        connection: &Connection,
    ) -> Result<(client::Handle<ClientHandler>, SftpSession, String)> {
        let config = Arc::new(client::Config {
            inactivity_timeout: Some(Duration::from_secs(60)),
            ..Default::default()
        });

        let addr = (connection.host.as_str(), connection.port);
        let mut handle = client::connect(config, addr, ClientHandler)
            .await
            .map_err(|e| SkyhookError::Ssh(format!("connect: {e}")))?;

        let authenticated = match &connection.auth {
            AuthMethod::Password { password } => handle
                .authenticate_password(connection.username.clone(), password.clone())
                .await
                .map_err(|e| SkyhookError::Ssh(format!("auth: {e}")))?,
            AuthMethod::Key { private_key, passphrase } => {
                let kp = decode_secret_key(private_key, passphrase.as_deref())
                    .map_err(|e| SkyhookError::Ssh(format!("key parse: {e}")))?;
                handle
                    .authenticate_publickey(connection.username.clone(), Arc::new(kp))
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
                            .authenticate_future(connection.username.clone(), pubkey, agent)
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
                        "SSH agent auth on Windows is not yet supported — use key or password"
                            .into(),
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
            .canonicalize(connection.default_path.clone().unwrap_or_else(|| ".".into()))
            .await
            .unwrap_or_else(|_| "/".into());

        Ok((handle, sftp, normalize_remote_path(&cwd)))
    }

    /// Attempt reconnect with backoff. Returns Ok on success, Err after the
    /// last backoff window elapses without a successful dial.
    async fn try_reconnect(&mut self) -> Result<()> {
        // Drop the dead transport before we start the next attempt.
        self.sftp = None;
        if let Some(h) = self.handle.take() {
            let _ = h
                .disconnect(russh::Disconnect::ByApplication, "reconnect", "en")
                .await;
        }

        let mut last_err: Option<SkyhookError> = None;
        for (i, secs) in RECONNECT_BACKOFF.iter().enumerate() {
            tracing::info!(session = %self.id, attempt = i + 1, "reconnect attempt");
            tokio::time::sleep(Duration::from_secs(*secs)).await;
            match Self::dial(&self.connection).await {
                Ok((h, s, cwd)) => {
                    self.handle = Some(h);
                    self.sftp = Some(s);
                    self.cwd = cwd;
                    self.heartbeat_failures = 0;
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!(session = %self.id, error = %e, "reconnect failed");
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| SkyhookError::Ssh("reconnect gave up".into())))
    }

    /// Main loop. Owns the SSH transport for its entire lifetime.
    async fn run(mut self, mut rx: mpsc::Receiver<SessionOp>) {
        // Initial dial.
        match Self::dial(&self.connection).await {
            Ok((h, s, cwd)) => {
                self.handle = Some(h);
                self.sftp = Some(s);
                self.cwd = cwd;
                self.set_state(SessionState::Connected, None).await;
            }
            Err(e) => {
                let reason = e.to_string();
                self.set_state(SessionState::Closed, Some(reason)).await;
                return;
            }
        }

        let mut hb = interval(HEARTBEAT_INTERVAL);
        hb.set_missed_tick_behavior(MissedTickBehavior::Delay);
        // Skip the immediate tick.
        hb.tick().await;

        loop {
            tokio::select! {
                maybe_op = rx.recv() => {
                    let Some(op) = maybe_op else { break; };
                    if matches!(op, SessionOp::Disconnect { .. }) {
                        self.handle_op(op).await;
                        break;
                    }
                    self.handle_op(op).await;
                }
                _ = hb.tick() => {
                    self.run_heartbeat().await;
                }
            }
        }

        // Best-effort cleanup.
        if let Some(h) = self.handle.take() {
            let _ = h.disconnect(russh::Disconnect::ByApplication, "bye", "en").await;
        }
        self.set_state(SessionState::Closed, None).await;
    }

    async fn run_heartbeat(&mut self) {
        if !matches!(self.info.read().await.state, SessionState::Connected) {
            return;
        }
        let ok = match self.sftp.as_ref() {
            Some(s) => s.metadata(".").await.is_ok(),
            None => false,
        };
        let payload = serde_json::json!({ "sessionId": self.id, "ok": ok });
        let _ = self.app.emit("session-heartbeat", payload);

        if ok {
            self.heartbeat_failures = 0;
            return;
        }
        self.heartbeat_failures = self.heartbeat_failures.saturating_add(1);
        if self.heartbeat_failures >= HEARTBEAT_FAIL_THRESHOLD {
            self.transition_to_degraded("heartbeat failed".into()).await;
        }
    }

    async fn transition_to_degraded(&mut self, reason: String) {
        self.set_state(SessionState::Degraded, Some(reason)).await;
        match self.try_reconnect().await {
            Ok(()) => self.set_state(SessionState::Connected, None).await,
            Err(e) => {
                self.set_state(SessionState::Closed, Some(e.to_string())).await;
            }
        }
    }

    /// Marks the session degraded and triggers reconnect if `err` indicates
    /// the channel/transport died.
    async fn check_fatal(&mut self, err: &SkyhookError) {
        if is_fatal_channel_error(&err.to_string()) {
            self.transition_to_degraded(err.to_string()).await;
        }
    }

    async fn sftp_ref(&self) -> Result<&SftpSession> {
        self.sftp
            .as_ref()
            .ok_or_else(|| SkyhookError::Sftp("sftp channel not open".into()))
    }

    /// Dispatch one queued op. The actor runs strictly one at a time.
    async fn handle_op(&mut self, op: SessionOp) {
        match op {
            SessionOp::ListDir { path, resp } => {
                let r = self.do_list_dir(path).await;
                if let Err(e) = &r { self.check_fatal(e).await; }
                let _ = resp.send(r);
            }
            SessionOp::Stat { path, resp } => {
                let r = self.do_stat(path).await;
                if let Err(e) = &r { self.check_fatal(e).await; }
                let _ = resp.send(r);
            }
            SessionOp::Walk { root, resp } => {
                let r = self.do_walk(root).await;
                if let Err(e) = &r { self.check_fatal(e).await; }
                let _ = resp.send(r);
            }
            SessionOp::ReadFile { path, resp } => {
                let r = self.do_read_file(path).await;
                if let Err(e) = &r { self.check_fatal(e).await; }
                let _ = resp.send(r);
            }
            SessionOp::WriteFile { path, data, resp } => {
                let r = self.do_write_file(path, data).await;
                if let Err(e) = &r { self.check_fatal(e).await; }
                let _ = resp.send(r);
            }
            SessionOp::Download { remote, local, resp } => {
                let r = self.do_download(remote, local).await;
                if let Err(e) = &r { self.check_fatal(e).await; }
                let _ = resp.send(r);
            }
            SessionOp::Upload { local, remote, resp } => {
                let r = self.do_upload(local, remote).await;
                if let Err(e) = &r { self.check_fatal(e).await; }
                let _ = resp.send(r);
            }
            SessionOp::Mkdir { path, resp } => {
                let r = self.do_mkdir(path).await;
                if let Err(e) = &r { self.check_fatal(e).await; }
                let _ = resp.send(r);
            }
            SessionOp::Remove { path, resp } => {
                let r = self.do_remove(path).await;
                if let Err(e) = &r { self.check_fatal(e).await; }
                let _ = resp.send(r);
            }
            SessionOp::Rename { from, to, resp } => {
                let r = self.do_rename(from, to).await;
                if let Err(e) = &r { self.check_fatal(e).await; }
                let _ = resp.send(r);
            }
            SessionOp::Reconnect { resp } => {
                self.set_state(SessionState::Degraded, Some("user requested".into())).await;
                let r = self.try_reconnect().await;
                match &r {
                    Ok(()) => self.set_state(SessionState::Connected, None).await,
                    Err(e) => self.set_state(SessionState::Closed, Some(e.to_string())).await,
                }
                let _ = resp.send(r);
            }
            SessionOp::Disconnect { resp } => {
                let _ = resp.send(Ok(()));
            }
        }
    }

    // ---- individual op impls ----------------------------------------------

    async fn do_list_dir(&mut self, path: String) -> Result<Vec<DirEntry>> {
        let sftp = self.sftp_ref().await?;
        let path = normalize_remote_path(&path);
        let resolved = sftp
            .canonicalize(path)
            .await
            .map_err(|e| SkyhookError::Sftp(e.to_string()))?;
        let resolved = normalize_remote_path(&resolved);
        let read_dir = sftp
            .read_dir(&resolved)
            .await
            .map_err(|e| SkyhookError::Sftp(e.to_string()))?;
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
        self.cwd = resolved;
        // refresh cwd in info snapshot
        self.info.write().await.cwd = self.cwd.clone();
        Ok(out)
    }

    async fn do_stat(&mut self, path: String) -> Result<DirEntry> {
        let sftp = self.sftp_ref().await?;
        let path = normalize_remote_path(&path);
        let meta = sftp
            .metadata(&path)
            .await
            .map_err(|e| SkyhookError::Sftp(e.to_string()))?;
        let name = path.rsplit('/').next().unwrap_or("").to_string();
        Ok(DirEntry {
            name,
            path: path.clone(),
            is_dir: meta.is_dir(),
            is_symlink: meta.is_symlink(),
            size: meta.size.unwrap_or(0),
            modified: meta.mtime.map(|t| t as i64),
            mode: meta.permissions,
        })
    }

    async fn do_walk(&mut self, root: String) -> Result<Vec<DirEntry>> {
        let sftp = self.sftp_ref().await?;
        let root = normalize_remote_path(&root);
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            let entries = sftp
                .read_dir(&dir)
                .await
                .map_err(|e| SkyhookError::Sftp(e.to_string()))?;
            for entry in entries {
                let name = entry.file_name();
                if name == "." || name == ".." {
                    continue;
                }
                let meta = entry.metadata();
                let full = join_remote(&dir, &name);
                let is_dir = meta.is_dir();
                out.push(DirEntry {
                    name,
                    path: full.clone(),
                    is_dir,
                    is_symlink: meta.is_symlink(),
                    size: meta.size.unwrap_or(0),
                    modified: meta.mtime.map(|t| t as i64),
                    mode: meta.permissions,
                });
                if is_dir && !meta.is_symlink() {
                    stack.push(full);
                }
            }
        }
        Ok(out)
    }

    async fn do_read_file(&mut self, path: String) -> Result<Vec<u8>> {
        let sftp = self.sftp_ref().await?;
        let path = normalize_remote_path(&path);
        let mut f = sftp
            .open_with_flags(path, OpenFlags::READ)
            .await
            .map_err(|e| SkyhookError::Sftp(e.to_string()))?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)
            .await
            .map_err(|e| SkyhookError::Sftp(e.to_string()))?;
        Ok(buf)
    }

    async fn do_write_file(&mut self, path: String, data: Vec<u8>) -> Result<()> {
        let sftp = self.sftp_ref().await?;
        let path = normalize_remote_path(&path);
        let mut f = sftp
            .open_with_flags(path, OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE)
            .await
            .map_err(|e| SkyhookError::Sftp(e.to_string()))?;
        f.write_all(&data)
            .await
            .map_err(|e| SkyhookError::Sftp(e.to_string()))?;
        f.shutdown()
            .await
            .map_err(|e| SkyhookError::Sftp(e.to_string()))?;
        Ok(())
    }

    async fn do_download(&mut self, remote: String, local: PathBuf) -> Result<u64> {
        let sftp = self.sftp_ref().await?;
        let remote = normalize_remote_path(&remote);
        let mut rf = sftp
            .open_with_flags(remote, OpenFlags::READ)
            .await
            .map_err(|e| SkyhookError::Sftp(e.to_string()))?;
        if let Some(parent) = local.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut lf = tokio::fs::File::create(&local).await?;
        let n = tokio::io::copy(&mut rf, &mut lf).await?;
        lf.shutdown().await?;
        Ok(n)
    }

    async fn do_upload(&mut self, local: PathBuf, remote: String) -> Result<u64> {
        let sftp = self.sftp_ref().await?;
        let remote = normalize_remote_path(&remote);
        let mut lf = tokio::fs::File::open(&local).await?;
        let mut rf = sftp
            .open_with_flags(remote, OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE)
            .await
            .map_err(|e| SkyhookError::Sftp(e.to_string()))?;
        let n = tokio::io::copy(&mut lf, &mut rf).await?;
        rf.shutdown()
            .await
            .map_err(|e| SkyhookError::Sftp(e.to_string()))?;
        Ok(n)
    }

    async fn do_mkdir(&mut self, path: String) -> Result<()> {
        let sftp = self.sftp_ref().await?;
        let path = normalize_remote_path(&path);
        sftp.create_dir(path)
            .await
            .map_err(|e| SkyhookError::Sftp(e.to_string()))
    }

    async fn do_remove(&mut self, path: String) -> Result<()> {
        let sftp = self.sftp_ref().await?;
        let path = normalize_remote_path(&path);
        let meta = sftp
            .metadata(&path)
            .await
            .map_err(|e| SkyhookError::Sftp(e.to_string()))?;
        if meta.is_dir() {
            sftp.remove_dir(&path).await
        } else {
            sftp.remove_file(&path).await
        }
        .map_err(|e| SkyhookError::Sftp(e.to_string()))
    }

    async fn do_rename(&mut self, from: String, to: String) -> Result<()> {
        let sftp = self.sftp_ref().await?;
        let from = normalize_remote_path(&from);
        let to = normalize_remote_path(&to);
        sftp.rename(from, to)
            .await
            .map_err(|e| SkyhookError::Sftp(e.to_string()))
    }
}
