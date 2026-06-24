use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SkyhookError {
    #[error("connection not found: {0}")]
    ConnectionNotFound(String),
    #[error("session not found: {0}")]
    SessionNotFound(String),
    #[error("ssh error: {0}")]
    Ssh(String),
    #[error("sftp error: {0}")]
    Sftp(String),
    #[error("auth failed")]
    AuthFailed,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("vault: {0}")]
    Vault(String),
    #[error("crypto: {0}")]
    Crypto(String),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

impl From<anyhow::Error> for SkyhookError {
    fn from(e: anyhow::Error) -> Self {
        SkyhookError::Other(e.to_string())
    }
}

// Tauri requires Serialize for command errors.
impl Serialize for SkyhookError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(Serialize)]
        struct Wire<'a> {
            kind: &'a str,
            message: String,
        }
        let kind = match self {
            SkyhookError::ConnectionNotFound(_) => "connection_not_found",
            SkyhookError::SessionNotFound(_) => "session_not_found",
            SkyhookError::Ssh(_) => "ssh",
            SkyhookError::Sftp(_) => "sftp",
            SkyhookError::AuthFailed => "auth_failed",
            SkyhookError::Io(_) => "io",
            SkyhookError::Vault(_) => "vault",
            SkyhookError::Crypto(_) => "crypto",
            SkyhookError::Serde(_) => "serde",
            SkyhookError::Other(_) => "other",
        };
        Wire {
            kind,
            message: self.to_string(),
        }
        .serialize(serializer)
    }
}

pub type Result<T> = std::result::Result<T, SkyhookError>;

// shorthand for deserialization in commands
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Empty {}
