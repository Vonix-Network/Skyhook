//! Approval gate — synchronises the runner with the user's approve/reject
//! decisions for tool calls awaiting confirmation.
//!
//! Flow:
//! 1. The runner generates a `call_id` and calls [`ApprovalGate::request`]
//!    which inserts a `oneshot::Sender` into the pending map and awaits the
//!    matching receiver.
//! 2. The frontend, after the user clicks Approve / Reject, invokes the
//!    [`agent_approve_tool`] / [`agent_reject_tool`] Tauri command, which
//!    calls [`approve`](ApprovalGate::approve) /
//!    [`reject`](ApprovalGate::reject) here, removing the sender and
//!    delivering the decision.
//! 3. The runner unblocks and either dispatches the tool or records a
//!    rejection result back to the model.

use std::collections::HashMap;

use tokio::sync::{oneshot, Mutex};

/// Outcome of a single approval request.
#[derive(Debug, Clone)]
pub enum ApprovalDecision {
    /// User approved the call; proceed with dispatch.
    Approved,
    /// User rejected; reason is returned to the model as the tool result.
    Rejected(String),
}

/// Pending-approval registry shared across runner and Tauri commands.
#[derive(Default)]
pub struct ApprovalGate {
    pending: Mutex<HashMap<String, oneshot::Sender<ApprovalDecision>>>,
}

impl ApprovalGate {
    /// Construct an empty gate.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `call_id` as awaiting approval and block until the user
    /// decides (or the sender is dropped — which is treated as a rejection).
    pub async fn request(&self, call_id: String) -> ApprovalDecision {
        let (tx, rx) = oneshot::channel();
        {
            let mut g = self.pending.lock().await;
            // If a duplicate id appears, drop the old sender — frontend will
            // never see two competing prompts for the same id.
            g.insert(call_id, tx);
        }
        match rx.await {
            Ok(d) => d,
            Err(_) => ApprovalDecision::Rejected("approval gate dropped".into()),
        }
    }

    /// Approve the call with `call_id`. Returns `true` if a pending request
    /// existed and was resolved; `false` if no such call was registered.
    pub async fn approve(&self, call_id: &str) -> bool {
        self.resolve(call_id, ApprovalDecision::Approved).await
    }

    /// Reject the call with `call_id`. Returns `true` on success.
    pub async fn reject(&self, call_id: &str, reason: String) -> bool {
        self.resolve(call_id, ApprovalDecision::Rejected(reason)).await
    }

    async fn resolve(&self, call_id: &str, d: ApprovalDecision) -> bool {
        let sender = {
            let mut g = self.pending.lock().await;
            g.remove(call_id)
        };
        match sender {
            Some(tx) => tx.send(d).is_ok(),
            None => false,
        }
    }

    /// Drop any pending approvals (e.g. when a turn is cancelled). All
    /// outstanding `request` calls will receive `Rejected("cancelled")`.
    pub async fn cancel_all(&self) {
        let mut g = self.pending.lock().await;
        for (_, tx) in g.drain() {
            let _ = tx.send(ApprovalDecision::Rejected("cancelled".into()));
        }
    }
}
