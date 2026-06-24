use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

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

/// EMA smoothing factor for instantaneous throughput samples. Higher is more
/// reactive; lower is smoother. 0.3 keeps the bar responsive without jitter.
pub(crate) const THROUGHPUT_EMA_ALPHA: f64 = 0.3;
/// Hard cap on ETA emitted to the frontend (99 hours, in seconds).
pub(crate) const ETA_CAP_SECONDS: u64 = 99 * 60 * 60;

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
    /// Smoothed transfer rate in bytes per second (EMA, alpha = 0.3).
    /// Zero when the job hasn't moved bytes yet or has reached a terminal
    /// state. Omitted from JSON when zero to keep wire payloads light.
    #[serde(skip_serializing_if = "f64_is_zero")]
    pub throughput_bps: f64,
    /// Estimated seconds until completion, computed from
    /// `(total - bytes) / throughput_bps`. `None` when the total is unknown,
    /// the job hasn't started moving bytes, or it has reached a terminal
    /// state. Capped at 99 hours.
    pub eta_seconds: Option<u64>,
}

fn f64_is_zero(v: &f64) -> bool {
    *v == 0.0
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
    /// Smoothed bytes/sec rate (EMA). Updated by the engine on each progress
    /// emission via `record_throughput_sample`.
    pub throughput_bps: f64,
    /// Wall-clock instant of the last throughput sample. `None` until the
    /// first sample is taken.
    pub last_sample_at: Option<Instant>,
    /// `bytes` value captured at the last throughput sample.
    pub last_sample_bytes: u64,
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
            throughput_bps: 0.0,
            last_sample_at: None,
            last_sample_bytes: 0,
        }
    }

    /// Fold a new bytes/time observation into the EMA. `now` and the current
    /// `self.bytes` are the right-hand side of the sample. Safe to call even
    /// when bytes are unchanged — it just decays the rate toward zero.
    pub fn record_throughput_sample(&mut self, now: Instant) {
        match self.last_sample_at {
            None => {
                // First sample: just anchor the baseline, no throughput yet.
                self.last_sample_at = Some(now);
                self.last_sample_bytes = self.bytes;
            }
            Some(prev_at) => {
                let dt = now.duration_since(prev_at).as_secs_f64();
                if dt <= 0.0 {
                    return;
                }
                let delta = self.bytes.saturating_sub(self.last_sample_bytes) as f64;
                let instant = delta / dt;
                if self.throughput_bps == 0.0 {
                    self.throughput_bps = instant;
                } else {
                    self.throughput_bps = THROUGHPUT_EMA_ALPHA * instant
                        + (1.0 - THROUGHPUT_EMA_ALPHA) * self.throughput_bps;
                }
                self.last_sample_at = Some(now);
                self.last_sample_bytes = self.bytes;
            }
        }
    }

    /// True iff bytes have not advanced since the last throughput sample.
    /// Used by the poller to decide whether to emit a stall heartbeat.
    pub fn is_stalled_since_last_sample(&self) -> bool {
        self.bytes == self.last_sample_bytes
    }

    /// Wall-clock duration since the last throughput sample, if any.
    pub fn time_since_last_sample(&self, now: Instant) -> Option<std::time::Duration> {
        self.last_sample_at.map(|t| now.duration_since(t))
    }

    pub fn snapshot(&self) -> Transfer {
        let terminal = matches!(
            self.status,
            TransferStatus::Done | TransferStatus::Cancelled | TransferStatus::Error
        );
        let throughput_bps = if terminal { 0.0 } else { self.throughput_bps };
        let eta_seconds = if terminal {
            None
        } else {
            match (self.total, throughput_bps) {
                (Some(total), tp) if tp > 0.0 && total > self.bytes => {
                    let remaining = (total - self.bytes) as f64;
                    let eta = (remaining / tp).round();
                    if eta.is_finite() && eta >= 0.0 {
                        Some((eta as u64).min(ETA_CAP_SECONDS))
                    } else {
                        None
                    }
                }
                _ => None,
            }
        };
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
            throughput_bps,
            eta_seconds,
        }
    }
}
