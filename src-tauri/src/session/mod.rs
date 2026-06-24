//! Session management — actor-per-session SFTP layer.
//!
//! # Architecture
//!
//! Each live SFTP connection is owned by exactly one [`actor::SessionActor`]
//! Tokio task. The actor exclusively holds:
//!
//! * the SSH transport (`russh::client::Handle`)
//! * the SFTP subsystem channel (`russh_sftp::client::SftpSession`)
//!
//! All operations are serialized through a tokio mpsc queue of
//! [`actor::SessionOp`] requests. This eliminates the race conditions and
//! lock-contention pitfalls of the v0.1.x `Mutex<SftpSession>` design — the
//! frontend can fire N concurrent `list_dir` calls and the actor processes
//! them strictly one at a time on the single SFTP channel.
//!
//! The [`SessionManager`] sits above the actors. It owns the
//! `connection_id -> SessionHandle` map and enforces the deduplication rule:
//! a second `connect(connection_id)` for a still-live session returns the
//! existing handle instead of opening a second SFTP subsystem. This kills the
//! Wings/Pterodactyl "too many concurrent SFTP sessions" bug at the source.
//!
//! # State machine
//!
//! See [`state::SessionState`] for the transitions. The actor emits two events:
//!
//! * `session-state-changed` — `{ sessionId, state, reason? }`
//! * `session-heartbeat`     — `{ sessionId, ok }`

pub mod actor;
pub mod heartbeat;
pub mod reconnect;
pub mod state;

use std::collections::HashMap;
use std::sync::Arc;

use tauri::AppHandle;
use tokio::sync::Mutex;

use crate::error::{Result, SkyhookError};
use crate::vault::Connection;

pub use actor::{SessionActor, SessionHandle};
pub use state::SessionInfo;

/// Top-level session registry. One per `AppState`.
///
/// Keyed by Skyhook session id (UUID) for direct lookups, with a parallel
/// `connection_id -> session_id` index for the dedup check.
pub struct SessionManager {
    inner: Mutex<Inner>,
    app: AppHandle,
}

struct Inner {
    by_session: HashMap<String, SessionHandle>,
    by_connection: HashMap<String, String>,
}

impl SessionManager {
    /// Build an empty manager. The `AppHandle` is used to emit session events.
    pub fn new(app: AppHandle) -> Self {
        Self {
            inner: Mutex::new(Inner {
                by_session: HashMap::new(),
                by_connection: HashMap::new(),
            }),
            app,
        }
    }

    /// Connect (or reuse) a session for `connection`.
    ///
    /// **Dedup rule:** if a live session already exists for
    /// `connection.id`, the existing [`SessionHandle`] is returned and **no
    /// new actor is spawned**. A session is "live" when its state is
    /// `Connecting`, `Connected`, or `Degraded`. `Closed` sessions are
    /// evicted lazily on the next `connect`.
    pub async fn connect(&self, connection: Connection) -> Result<SessionHandle> {
        let mut g = self.inner.lock().await;
        if let Some(sid) = g.by_connection.get(&connection.id).cloned() {
            if let Some(h) = g.by_session.get(&sid).cloned() {
                if h.is_live().await {
                    return Ok(h);
                }
                // Stale entry — fall through and spawn a fresh actor.
                g.by_session.remove(&sid);
                g.by_connection.remove(&connection.id);
            }
        }
        let handle = SessionActor::spawn(self.app.clone(), connection.clone());
        g.by_session.insert(handle.id.clone(), handle.clone());
        g.by_connection.insert(connection.id, handle.id.clone());
        Ok(handle)
    }

    /// Look up a session by its id.
    pub async fn get(&self, session_id: &str) -> Option<SessionHandle> {
        self.inner.lock().await.by_session.get(session_id).cloned()
    }

    /// Look up a session by its id, returning [`SkyhookError::SessionNotFound`]
    /// when missing.
    pub async fn require(&self, session_id: &str) -> Result<SessionHandle> {
        self.get(session_id)
            .await
            .ok_or_else(|| SkyhookError::SessionNotFound(session_id.into()))
    }

    /// Tear down the session and remove it from the registry. Idempotent.
    pub async fn disconnect(&self, session_id: &str) -> Result<()> {
        let handle = {
            let mut g = self.inner.lock().await;
            let h = g.by_session.remove(session_id);
            if let Some(h) = &h {
                g.by_connection.remove(&h.connection_id);
            }
            h
        };
        if let Some(h) = handle {
            h.disconnect().await?;
        }
        Ok(())
    }

    /// Snapshot every known session for the UI.
    pub async fn list(&self) -> Vec<SessionInfo> {
        let g = self.inner.lock().await;
        let handles: Vec<SessionHandle> = g.by_session.values().cloned().collect();
        drop(g);
        let mut out = Vec::with_capacity(handles.len());
        for h in handles {
            out.push(h.info().await);
        }
        out
    }
}

/// Convenient alias used across `commands.rs`.
#[allow(dead_code)]
pub type ManagedSessions = Arc<SessionManager>;
