//! Heartbeat config + helper.
//!
//! The heartbeat is driven by the [`super::actor::SessionActor`] event loop:
//! every [`HEARTBEAT_INTERVAL`] the actor calls `metadata(".")` on its SFTP
//! channel. After [`HEARTBEAT_FAIL_THRESHOLD`] consecutive failures the actor
//! transitions to [`super::state::SessionState::Degraded`] and triggers
//! reconnect.
//!
//! Implementing the heartbeat inside the actor loop (instead of a separate
//! task) keeps it serialized with user-issued ops — no risk of two callers
//! sharing the SFTP channel at the same time.

use std::time::Duration;

/// How often the actor pings the SFTP channel with `metadata(".")`.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// Consecutive failed heartbeats before transitioning to `Degraded`.
pub const HEARTBEAT_FAIL_THRESHOLD: u8 = 2;
