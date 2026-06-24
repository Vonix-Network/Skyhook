use super::job::{JobState, Transfer, TransferDirection, TransferRequest, TransferStatus};
use crate::error::{Result, SkyhookError};
use crate::session::{SessionHandle, SessionManager};
use crate::sftp::normalize_remote_path;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

/// Default cap on concurrently-active transfers. The rest sit Queued.
const MAX_CONCURRENT: usize = 2;
/// Cap on enqueued/active transfer entries to keep memory bounded across long
/// sessions. Older completed (Done/Cancelled/Error) jobs at the front are
/// evicted to make room; active/queued/paused jobs are preserved.
const MAX_JOBS_TRACKED: usize = 5000;

/// Public engine. Cheap to clone — internally `Arc`-shared.
#[derive(Clone)]
pub struct TransferEngine {
    inner: Arc<EngineInner>,
}

struct EngineInner {
    jobs: Mutex<HashMap<String, JobState>>,
    order: Mutex<Vec<String>>, // insertion order, drives list()
    sessions: Arc<SessionManager>,
    app: Mutex<Option<AppHandle>>,
    permits: Arc<tokio::sync::Semaphore>,
}

#[derive(serde::Serialize, Clone)]
struct ProgressEvent {
    id: String,
    bytes: u64,
    total: Option<u64>,
    status: TransferStatus,
}

impl TransferEngine {
    pub fn new(sessions: Arc<SessionManager>) -> Self {
        Self {
            inner: Arc::new(EngineInner {
                jobs: Mutex::new(HashMap::new()),
                order: Mutex::new(Vec::new()),
                sessions,
                app: Mutex::new(None),
                permits: Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT)),
            }),
        }
    }

    /// Attach the Tauri app handle once it's available (called from `setup`).
    pub async fn set_app_handle(&self, app: AppHandle) {
        *self.inner.app.lock().await = Some(app);
    }

    /// Enqueue a batch of transfer requests. Expands recursive folder requests
    /// into one job per regular file. Returns the resulting job ids in order.
    pub async fn enqueue(
        &self,
        session_id: String,
        reqs: Vec<TransferRequest>,
    ) -> Result<Vec<String>> {
        let session = self
            .inner
            .sessions
            .get(&session_id)
            .await
            .ok_or_else(|| SkyhookError::SessionNotFound(session_id.clone()))?;

        let mut ids = Vec::new();
        for req in reqs {
            match self.expand_request(&session, &req).await {
                Ok(expanded) => {
                    for one in expanded {
                        let id = self.insert_job(&session_id, &one).await;
                        ids.push(id.clone());
                        self.spawn_runner(id, session.clone()).await;
                    }
                }
                Err(e) => {
                    // Per-job failure rather than catastrophic batch failure.
                    let id = self
                        .insert_failed_job(&session_id, &req, e.to_string())
                        .await;
                    ids.push(id);
                }
            }
        }
        Ok(ids)
    }

    pub async fn pause(&self, id: &str) {
        let snap_opt = {
            let mut jobs = self.inner.jobs.lock().await;
            jobs.get_mut(id).map(|j| {
                j.paused.store(true, Ordering::SeqCst);
                if matches!(j.status, TransferStatus::Active) {
                    j.status = TransferStatus::Paused;
                }
                j.snapshot()
            })
        };
        if let Some(s) = snap_opt {
            self.emit(&s).await;
        }
    }

    pub async fn resume(&self, id: &str) {
        let snap_opt = {
            let mut jobs = self.inner.jobs.lock().await;
            jobs.get_mut(id).map(|j| {
                j.paused.store(false, Ordering::SeqCst);
                if matches!(j.status, TransferStatus::Paused) {
                    j.status = TransferStatus::Active;
                }
                j.snapshot()
            })
        };
        if let Some(s) = snap_opt {
            self.emit(&s).await;
        }
    }

    pub async fn cancel(&self, id: &str) {
        let snap_opt = {
            let mut jobs = self.inner.jobs.lock().await;
            jobs.get_mut(id).map(|j| {
                j.cancelled.store(true, Ordering::SeqCst);
                if matches!(j.status, TransferStatus::Queued | TransferStatus::Paused) {
                    j.status = TransferStatus::Cancelled;
                }
                j.snapshot()
            })
        };
        if let Some(s) = snap_opt {
            self.emit(&s).await;
        }
    }

    pub async fn list(&self) -> Vec<Transfer> {
        let jobs = self.inner.jobs.lock().await;
        let order = self.inner.order.lock().await;
        order
            .iter()
            .filter_map(|id| jobs.get(id).map(|j| j.snapshot()))
            .collect()
    }

    // -- internals --------------------------------------------------------

    async fn insert_job(&self, session_id: &str, req: &TransferRequest) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();
        let state = JobState::new(id.clone(), session_id.to_string(), req, now);
        let mut jobs = self.inner.jobs.lock().await;
        let mut order = self.inner.order.lock().await;
        jobs.insert(id.clone(), state);
        order.push(id.clone());
        if order.len() > MAX_JOBS_TRACKED {
            let mut evicted = 0;
            let to_remove = order.len() - MAX_JOBS_TRACKED;
            order.retain(|jid| {
                if evicted >= to_remove {
                    return true;
                }
                let drop_it = matches!(
                    jobs.get(jid).map(|j| j.status),
                    Some(TransferStatus::Done)
                        | Some(TransferStatus::Cancelled)
                        | Some(TransferStatus::Error)
                );
                if drop_it {
                    jobs.remove(jid);
                    evicted += 1;
                    false
                } else {
                    true
                }
            });
        }
        id
    }

    async fn insert_failed_job(
        &self,
        session_id: &str,
        req: &TransferRequest,
        err: String,
    ) -> String {
        let id = self.insert_job(session_id, req).await;
        let snap = {
            let mut jobs = self.inner.jobs.lock().await;
            jobs.get_mut(&id).map(|j| {
                j.status = TransferStatus::Error;
                j.error = Some(err);
                j.snapshot()
            })
        };
        if let Some(s) = snap {
            self.emit(&s).await;
        }
        id
    }

    async fn snapshot(&self, id: &str) -> Option<Transfer> {
        let jobs = self.inner.jobs.lock().await;
        jobs.get(id).map(|j| j.snapshot())
    }

    async fn emit(&self, snap: &Transfer) {
        let app = self.inner.app.lock().await.clone();
        if let Some(app) = app {
            let payload = ProgressEvent {
                id: snap.id.clone(),
                bytes: snap.bytes,
                total: snap.total,
                status: snap.status,
            };
            let _ = app.emit("transfer-progress", payload);
        }
    }

    /// Expand a single TransferRequest into the leaf-file requests we'll run.
    async fn expand_request(
        &self,
        session: &SessionHandle,
        req: &TransferRequest,
    ) -> Result<Vec<TransferRequest>> {
        if !req.recursive {
            return Ok(vec![req.clone()]);
        }
        match req.direction {
            TransferDirection::Upload => {
                let local = PathBuf::from(&req.local);
                let meta = match std::fs::metadata(&local) {
                    Ok(m) => m,
                    Err(_) => return Ok(vec![req.clone()]),
                };
                if !meta.is_dir() {
                    return Ok(vec![req.clone()]);
                }
                let mut out = Vec::new();
                let mut stack = vec![local.clone()];
                while let Some(dir) = stack.pop() {
                    let rd = match std::fs::read_dir(&dir) {
                        Ok(rd) => rd,
                        Err(_) => continue,
                    };
                    for entry in rd.flatten() {
                        let p = entry.path();
                        let ft = match entry.file_type() {
                            Ok(ft) => ft,
                            Err(_) => continue,
                        };
                        if ft.is_symlink() {
                            continue;
                        }
                        if ft.is_dir() {
                            stack.push(p);
                        } else if ft.is_file() {
                            let rel = match p.strip_prefix(&local) {
                                Ok(r) => r,
                                Err(_) => continue,
                            };
                            let rel_str = rel.to_string_lossy().replace('\\', "/");
                            let remote = format!(
                                "{}/{}",
                                req.remote.trim_end_matches('/'),
                                rel_str
                            );
                            out.push(TransferRequest {
                                direction: TransferDirection::Upload,
                                local: p.to_string_lossy().to_string(),
                                remote: normalize_remote_path(&remote),
                                recursive: false,
                            });
                        }
                    }
                }
                Ok(out)
            }
            TransferDirection::Download => {
                let root = normalize_remote_path(&req.remote);
                // Walk via the session actor (single remote round-trip).
                let walked = match session.walk(root.clone()).await {
                    Ok(w) => w,
                    Err(_) => return Ok(vec![req.clone()]),
                };
                let mut out = Vec::new();
                for e in walked {
                    if e.is_dir || e.is_symlink {
                        continue;
                    }
                    let rel = e
                        .path
                        .strip_prefix(&root)
                        .unwrap_or(&e.path)
                        .trim_start_matches('/');
                    let local_path = Path::new(&req.local).join(rel);
                    out.push(TransferRequest {
                        direction: TransferDirection::Download,
                        local: local_path.to_string_lossy().to_string(),
                        remote: e.path.clone(),
                        recursive: false,
                    });
                }
                Ok(out)
            }
        }
    }

    async fn spawn_runner(&self, id: String, session: SessionHandle) {
        let engine = self.clone();
        tokio::spawn(async move {
            engine.run_job(id, session).await;
        });
    }

    async fn run_job(&self, id: String, session: SessionHandle) {
        let permit = match self.inner.permits.clone().acquire_owned().await {
            Ok(p) => p,
            Err(_) => return,
        };
        let _permit = permit;

        // Snapshot what we need; bail early if already cancelled.
        let (direction, local, remote, paused, cancelled) = {
            let mut jobs = self.inner.jobs.lock().await;
            let j = match jobs.get_mut(&id) {
                Some(j) => j,
                None => return,
            };
            if j.cancelled.load(Ordering::SeqCst) {
                j.status = TransferStatus::Cancelled;
                let snap = j.snapshot();
                drop(jobs);
                self.emit(&snap).await;
                return;
            }
            if matches!(
                j.status,
                TransferStatus::Done | TransferStatus::Cancelled | TransferStatus::Error
            ) {
                return;
            }
            j.status = TransferStatus::Active;
            (
                j.direction,
                j.local.clone(),
                j.remote.clone(),
                j.paused.clone(),
                j.cancelled.clone(),
            )
        };

        if let Some(s) = self.snapshot(&id).await {
            self.emit(&s).await;
        }

        // Cooperative pause before heavy I/O.
        while paused.load(Ordering::SeqCst) && !cancelled.load(Ordering::SeqCst) {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        if cancelled.load(Ordering::SeqCst) {
            self.finalize(&id, TransferStatus::Cancelled, None).await;
            return;
        }

        // Resolve total best-effort.
        let total = self
            .precompute_total(&session, direction, &local, &remote)
            .await;
        if let Some(t) = total {
            let mut jobs = self.inner.jobs.lock().await;
            if let Some(j) = jobs.get_mut(&id) {
                j.total = Some(t);
            }
        }

        let progress_handle = self
            .start_progress_poller(id.clone(), direction, local.clone(), cancelled.clone())
            .await;

        let result: Result<u64> = match direction {
            TransferDirection::Upload => {
                let local_path = PathBuf::from(&local);
                if let Some(parent) = parent_of_remote(&remote) {
                    let _ = ensure_remote_dir(&session, &parent).await;
                }
                session.upload(local_path, remote.clone()).await
            }
            TransferDirection::Download => {
                let local_path = PathBuf::from(&local);
                if let Some(parent) = local_path.parent() {
                    let _ = tokio::fs::create_dir_all(parent).await;
                }
                session.download(remote.clone(), local_path).await
            }
        };

        progress_handle.abort();

        if cancelled.load(Ordering::SeqCst) {
            self.finalize(&id, TransferStatus::Cancelled, None).await;
            return;
        }

        match result {
            Ok(n) => {
                {
                    let mut jobs = self.inner.jobs.lock().await;
                    if let Some(j) = jobs.get_mut(&id) {
                        j.bytes = n;
                        if j.total.is_none() {
                            j.total = Some(n);
                        }
                    }
                }
                self.finalize(&id, TransferStatus::Done, None).await;
            }
            Err(e) => {
                self.finalize(&id, TransferStatus::Error, Some(e.to_string()))
                    .await;
            }
        }
    }

    async fn finalize(&self, id: &str, status: TransferStatus, err: Option<String>) {
        let snap = {
            let mut jobs = self.inner.jobs.lock().await;
            jobs.get_mut(id).map(|j| {
                j.status = status;
                j.error = err;
                j.snapshot()
            })
        };
        if let Some(s) = snap {
            self.emit(&s).await;
        }
    }

    async fn precompute_total(
        &self,
        session: &SessionHandle,
        direction: TransferDirection,
        local: &str,
        remote: &str,
    ) -> Option<u64> {
        match direction {
            TransferDirection::Upload => std::fs::metadata(local).ok().map(|m| m.len()),
            TransferDirection::Download => {
                session.stat(remote.to_string()).await.ok().map(|e| e.size)
            }
        }
    }

    /// Spawn a poller that emits progress events roughly every 100ms during an
    /// active transfer. For download we poll the local file size on disk; for
    /// upload there is no cheap mid-flight progress source against the actor,
    /// so we only emit start (status=Active) + the final snapshot.
    async fn start_progress_poller(
        &self,
        id: String,
        direction: TransferDirection,
        local: String,
        cancelled: Arc<AtomicBool>,
    ) -> tokio::task::JoinHandle<()> {
        let engine = self.clone();
        tokio::spawn(async move {
            let mut last_emit: u64 = 0;
            if let Some(s) = engine.snapshot(&id).await {
                engine.emit(&s).await;
            }
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                if cancelled.load(Ordering::SeqCst) {
                    break;
                }
                let still_active = {
                    let jobs = engine.inner.jobs.lock().await;
                    matches!(
                        jobs.get(&id).map(|j| j.status),
                        Some(TransferStatus::Active)
                    )
                };
                if !still_active {
                    break;
                }
                if matches!(direction, TransferDirection::Download) {
                    let observed = std::fs::metadata(&local).map(|m| m.len()).unwrap_or(0);
                    let should_emit = observed != last_emit
                        && (last_emit == 0
                            || observed.saturating_sub(last_emit) >= 256 * 1024);
                    let snap = {
                        let mut jobs = engine.inner.jobs.lock().await;
                        jobs.get_mut(&id).map(|j| {
                            j.bytes = observed;
                            j.snapshot()
                        })
                    };
                    if should_emit {
                        last_emit = observed;
                        if let Some(s) = snap {
                            engine.emit(&s).await;
                        }
                    }
                }
            }
        })
    }
}

fn parent_of_remote(p: &str) -> Option<String> {
    let p = normalize_remote_path(p);
    if p == "/" {
        return None;
    }
    let idx = p.rfind('/')?;
    if idx == 0 {
        Some("/".into())
    } else {
        Some(p[..idx].to_string())
    }
}

/// Walks the path, creating each segment. Errors from existing-dir creation
/// are swallowed.
async fn ensure_remote_dir(session: &SessionHandle, path: &str) -> Result<()> {
    let p = normalize_remote_path(path);
    if p == "/" || p.is_empty() {
        return Ok(());
    }
    let mut cur = String::new();
    for part in p.trim_start_matches('/').split('/') {
        if part.is_empty() {
            continue;
        }
        cur.push('/');
        cur.push_str(part);
        let _ = session.mkdir(cur.clone()).await;
    }
    Ok(())
}
