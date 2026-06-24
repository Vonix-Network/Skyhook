//! Transfer engine: production-grade upload/download queue with progress events,
//! recursive folder support, and cooperative pause/resume/cancel.
//!
//! Frontend contract (must match exactly):
//!   - `Transfer` is what gets serialized to JS
//!   - `transfer-progress` event payload: { id, bytes, total, status }
//!
//! Concurrency: `MAX_CONCURRENT` active transfers at a time, the rest sit Queued.
//! Pause/cancel: checked between files (and, for download, by stopping the
//! progress poller). Within a single file the underlying SessionActor op is
//! atomic — finer-grained interruption will arrive once the actor exposes
//! chunked I/O.

pub mod engine;
pub mod job;

pub use engine::TransferEngine;
pub use job::{Transfer, TransferDirection, TransferRequest, TransferStatus};
