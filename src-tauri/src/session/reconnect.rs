//! Reconnect backoff schedule.
//!
//! The [`super::actor::SessionActor`] walks this slice on every reconnect
//! attempt, sleeping the listed seconds before re-dialing SSH + SFTP. If every
//! attempt fails the session transitions to
//! [`super::state::SessionState::Closed`] with the last error as the reason.
//!
//! Backoff is intentionally short: SFTP sessions on Wings/Pterodactyl panels
//! often bounce briefly, and the UI should recover quickly without spamming
//! the server.

/// Sleep durations (seconds) between successive reconnect attempts.
pub const RECONNECT_BACKOFF: [u64; 3] = [1, 2, 5];
