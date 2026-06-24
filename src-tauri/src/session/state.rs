//! Session state machine types, serializable to the frontend.
//!
//! State transitions (driven by [`super::actor::SessionActor`]):
//!
//! ```text
//!   Connecting --(auth ok + sftp ok)--> Connected
//!   Connecting --(fail)--> Closed (with reason)
//!   Connected --(heartbeat fail x2)--> Degraded
//!   Connected --(op fails fatally)--> Degraded
//!   Degraded --(reconnect ok)--> Connected
//!   Degraded --(reconnect give up)--> Closed
//!   any --(user disconnect)--> Closed
//! ```

use serde::{Deserialize, Serialize};

/// Lifecycle phase of a [`super::SessionActor`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    /// Initial state: SSH transport + SFTP subsystem coming up.
    Connecting,
    /// Ready to serve SFTP ops.
    Connected,
    /// SFTP channel died; reconnect attempts in progress.
    Degraded,
    /// Terminal state — actor task has exited or is exiting.
    Closed,
}

impl SessionState {
    /// True for any state where the session can still be used or recover.
    pub fn is_live(self) -> bool {
        matches!(self, SessionState::Connecting | SessionState::Connected | SessionState::Degraded)
    }
}

/// Snapshot of session state, safe to send to the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    pub connection_id: String,
    pub state: SessionState,
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
