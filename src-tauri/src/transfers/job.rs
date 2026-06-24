use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TransferDirection {
    Upload,
    Download,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TransferStatus {
    Queued,
    Active,
    Paused,
    Done,
    Cancelled,
    Error,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TransferRequest {
    pub direction: TransferDirection,
    pub local: String,
    pub remote: String,
    pub recursive: bool,
}

/// Snapshot of a transfer, serialized to the frontend.
#[derive(Serialize, Clone, Debug)]
pub struct Transfer {
    pub id: String,
    pub session_id: String,
    pub direction: TransferDirection,
    pub local: String,
    pub remote: String,
    pub bytes: u64,
    pub total: Option<u64>,
    pub status: TransferStatus,
    pub error: Option<String>,
    pub started_at: i64,
}

/// Internal state held by the engine for each transfer. The public-facing
/// `Transfer` is derived from this via `snapshot()`.
pub(crate) struct JobState {
    pub id: String,
    pub session_id: String,
    pub direction: TransferDirection,
    pub local: String,
    pub remote: String,
    pub bytes: u64,
    pub total: Option<u64>,
    pub status: TransferStatus,
    pub error: Option<String>,
    pub started_at: i64,
    pub paused: Arc<AtomicBool>,
    pub cancelled: Arc<AtomicBool>,
}

impl JobState {
    pub fn new(
        id: String,
        session_id: String,
        req: &TransferRequest,
        now: i64,
    ) -> Self {
        Self {
            id,
            session_id,
            direction: req.direction,
            local: req.local.clone(),
            remote: req.remote.clone(),
            bytes: 0,
            total: None,
            status: TransferStatus::Queued,
            error: None,
            started_at: now,
            paused: Arc::new(AtomicBool::new(false)),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn snapshot(&self) -> Transfer {
        Transfer {
            id: self.id.clone(),
            session_id: self.session_id.clone(),
            direction: self.direction,
            local: self.local.clone(),
            remote: self.remote.clone(),
            bytes: self.bytes,
            total: self.total,
            status: self.status,
            error: self.error.clone(),
            started_at: self.started_at,
        }
    }
}
