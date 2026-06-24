//! SFTP helpers: path normalization, shared types, and fatal-error detection.
//!
//! Session lifecycle and SFTP I/O live in [`crate::session`]. This module is
//! intentionally tiny — only the bits the rest of the crate needs to share.

use serde::{Deserialize, Serialize};

/// Normalize a remote SFTP path:
/// - Collapse runs of `/` into a single `/`
/// - Strip trailing `/` (except for root)
/// - Empty input becomes `/`
///
/// Wings/Pterodactyl SFTP rejects malformed paths like `//foo` and closes the
/// channel; OpenSSH tolerates them. Be safe everywhere.
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

/// One entry returned from a directory listing or stat call.
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

/// Heuristic: does this error string look like the SSH channel / TCP transport
/// died (vs. a normal SFTP failure like ENOENT or EACCES)?
///
/// When this returns true, the [`crate::session::SessionActor`] transitions to
/// [`crate::session::state::SessionState::Degraded`] and triggers reconnect.
pub fn is_fatal_channel_error(msg: &str) -> bool {
    let m = msg.to_ascii_lowercase();
    m.contains("connection lost")
        || m.contains("disconnected")
        || m.contains("channel") && (m.contains("closed") || m.contains("eof") || m.contains("broken"))
        || m.contains("broken pipe")
        || m.contains("not connected")
        || m.contains("eof")
}
