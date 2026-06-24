import { useMemo } from "react";
import {
  Upload,
  Download,
  Pause,
  Play,
  X,
  RotateCcw,
  CheckCircle2,
  AlertCircle,
} from "lucide-react";
import { useStore, Transfer, TransferStatus } from "../lib/store";
import { api } from "../lib/api";

// ---------- formatters ----------

function formatBytes(n: number): string {
  if (!isFinite(n) || n < 0) return "0 B";
  if (n < 1024) return `${n.toFixed(0)} B`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(1)} GB`;
}

function formatRate(bps: number): string {
  if (!isFinite(bps) || bps <= 0) return "0 B/s";
  if (bps < 1024) return `${bps.toFixed(0)} B/s`;
  if (bps < 1024 ** 2) return `${(bps / 1024).toFixed(1)} KB/s`;
  if (bps < 1024 ** 3) return `${(bps / 1024 ** 2).toFixed(1)} MB/s`;
  return `${(bps / 1024 ** 3).toFixed(1)} GB/s`;
}

function formatDuration(seconds: number): string {
  if (!isFinite(seconds) || seconds < 0) return "—";
  const s = Math.floor(seconds);
  if (s >= 99 * 3600) return "99h+";
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${sec}s`;
  return `${sec}s`;
}

function basename(p: string): string {
  if (!p) return "";
  // Handle both / and \ separators
  const norm = p.replace(/\\/g, "/").replace(/\/+$/, "");
  const idx = norm.lastIndexOf("/");
  return idx >= 0 ? norm.slice(idx + 1) || norm : norm;
}

// Truncate the middle of a long path to keep both ends visible.
function midTruncate(s: string, max = 64): string {
  if (s.length <= max) return s;
  const head = Math.ceil((max - 1) / 2);
  const tail = Math.floor((max - 1) / 2);
  return `${s.slice(0, head)}…${s.slice(s.length - tail)}`;
}

// ---------- sort helpers ----------

const GROUP: Record<TransferStatus, number> = {
  active: 0,
  paused: 0,
  queued: 1,
  done: 2,
  cancelled: 2,
  error: 2,
};

function sortTransfers(list: Transfer[]): Transfer[] {
  return list.slice().sort((a, b) => {
    const ga = GROUP[a.status];
    const gb = GROUP[b.status];
    if (ga !== gb) return ga - gb;
    return b.startedAt - a.startedAt;
  });
}

// ---------- component ----------

function StatusPill({ status }: { status: TransferStatus }) {
  return (
    <div className={`status-pill status-${status}`}>
      {status === "error" ? (
        <span style={{ display: "inline-flex", alignItems: "center", gap: 4 }}>
          <AlertCircle size={10} /> error
        </span>
      ) : status === "done" ? (
        <span style={{ display: "inline-flex", alignItems: "center", gap: 4 }}>
          <CheckCircle2 size={10} /> done
        </span>
      ) : (
        status
      )}
    </div>
  );
}

function TransferRow({ t }: { t: Transfer }) {
  const toast = useStore((s) => s.toast);
  const updateTransfer = useStore((s) => s.updateTransfer);
  const enqueueTransfer = useStore((s) => s.enqueueTransfer);

  const name =
    t.direction === "upload" ? basename(t.local) : basename(t.remote);
  const src = t.direction === "upload" ? t.local : t.remote;
  const dst = t.direction === "upload" ? t.remote : t.local;
  const subtitle = midTruncate(`${src} → ${dst}`, 90);

  const pct =
    t.total && t.total > 0
      ? Math.min(100, Math.max(0, (t.bytes / t.total) * 100))
      : t.status === "done"
      ? 100
      : 0;

  const sizeText =
    t.total != null
      ? `${formatBytes(t.bytes)} / ${formatBytes(t.total)}`
      : formatBytes(t.bytes);

  const showRate = t.status === "active" && t.throughput_bps > 0;
  const showEta =
    t.status === "active" && t.eta_seconds != null && t.eta_seconds > 0;

  const onPause = async () => {
    try {
      await api.transferPause(t.id);
    } catch (e: any) {
      toast(`Pause failed: ${e?.message ?? e}`, "error");
    }
  };
  const onResume = async () => {
    try {
      await api.transferResume(t.id);
    } catch (e: any) {
      toast(`Resume failed: ${e?.message ?? e}`, "error");
    }
  };
  const onCancel = async () => {
    try {
      await api.transferCancel(t.id);
    } catch (e: any) {
      toast(`Cancel failed: ${e?.message ?? e}`, "error");
    }
  };
  const onDismiss = () => {
    useStore.setState((s) => ({
      transfers: s.transfers.filter((x) => x.id !== t.id),
    }));
  };
  const onRetry = () => {
    // Re-enqueue locally; backend will receive new id on next dispatch.
    // We also drop the errored row.
    enqueueTransfer({
      sessionId: t.sessionId,
      direction: t.direction,
      local: t.local,
      remote: t.remote,
      total: t.total,
    });
    // best-effort backend enqueue
    api
      .transferEnqueue(t.sessionId, [
        {
          direction: t.direction,
          local: t.local,
          remote: t.remote,
          recursive: false,
        },
      ])
      .catch((e: any) => toast(`Retry failed: ${e?.message ?? e}`, "error"));
    updateTransfer(t.id, { status: "cancelled" });
    onDismiss();
  };

  return (
    <div
      className={`transfer-row tp-row tp-row-${t.status}`}
      title={t.error ?? subtitle}
    >
      <div className="tp-row-head">
        <div className="tp-row-icon">
          {t.direction === "upload" ? (
            <Upload size={14} className="icon-file" />
          ) : (
            <Download size={14} className="icon-file" />
          )}
        </div>
        <div className="tp-row-name">
          <div className="tp-row-title" title={name}>
            {name || "(unnamed)"}
          </div>
          <div className="tp-row-sub dim" title={`${src} → ${dst}`}>
            {subtitle}
          </div>
        </div>
        <StatusPill status={t.status} />
        <div className="tp-row-actions">
          {(t.status === "queued" || t.status === "active") && (
            <>
              <button
                className="btn btn-ghost btn-icon"
                onClick={onPause}
                title="Pause"
                aria-label="Pause transfer"
              >
                <Pause size={13} />
              </button>
              <button
                className="btn btn-ghost btn-icon"
                onClick={onCancel}
                title="Cancel"
                aria-label="Cancel transfer"
              >
                <X size={13} />
              </button>
            </>
          )}
          {t.status === "paused" && (
            <>
              <button
                className="btn btn-ghost btn-icon"
                onClick={onResume}
                title="Resume"
                aria-label="Resume transfer"
              >
                <Play size={13} />
              </button>
              <button
                className="btn btn-ghost btn-icon"
                onClick={onCancel}
                title="Cancel"
                aria-label="Cancel transfer"
              >
                <X size={13} />
              </button>
            </>
          )}
          {t.status === "error" && (
            <>
              <button
                className="btn btn-ghost btn-icon"
                onClick={onRetry}
                title="Retry"
                aria-label="Retry transfer"
              >
                <RotateCcw size={13} />
              </button>
              <button
                className="btn btn-ghost btn-icon"
                onClick={onDismiss}
                title="Dismiss"
                aria-label="Dismiss"
              >
                <X size={13} />
              </button>
            </>
          )}
          {(t.status === "done" || t.status === "cancelled") && (
            <button
              className="btn btn-ghost btn-icon"
              onClick={onDismiss}
              title="Dismiss"
              aria-label="Dismiss"
            >
              <X size={13} />
            </button>
          )}
        </div>
      </div>

      <div className="tp-row-bar-wrap">
        <div
          className={`tp-bar tp-bar-${t.status}`}
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.round(pct)}
          aria-label={`${name} progress`}
        >
          <div
            className={`tp-bar-fill ${
              t.status === "active" ? "tp-bar-striped" : ""
            }`}
            style={{ width: `${pct}%` }}
          />
        </div>
      </div>

      <div className="tp-row-meta mono dim">
        <span className="tp-meta-size">{sizeText}</span>
        {showRate && (
          <span className="tp-meta-rate">{formatRate(t.throughput_bps)}</span>
        )}
        {showEta && (
          <span className="tp-meta-eta">
            ~ {formatDuration(t.eta_seconds as number)}
          </span>
        )}
        {t.status === "error" && t.error && (
          <span className="tp-meta-err" title={t.error}>
            {midTruncate(t.error, 60)}
          </span>
        )}
      </div>
    </div>
  );
}

export function TransferPanel() {
  const transfers = useStore((s) => s.transfers);
  const close = useStore((s) => s.toggleTransfersPanel);

  const sorted = useMemo(() => sortTransfers(transfers), [transfers]);

  const counts = useMemo(() => {
    let active = 0;
    let queued = 0;
    let completed = 0;
    let paused = 0;
    for (const t of transfers) {
      if (t.status === "active") active++;
      else if (t.status === "paused") paused++;
      else if (t.status === "queued") queued++;
      else completed++;
    }
    return { active, queued, completed, paused };
  }, [transfers]);

  const hasClearable = transfers.some(
    (t) => t.status === "done" || t.status === "cancelled" || t.status === "error",
  );

  const clearCompleted = () => {
    useStore.setState((s) => ({
      transfers: s.transfers.filter(
        (t) =>
          t.status !== "done" &&
          t.status !== "cancelled" &&
          t.status !== "error",
      ),
    }));
  };

  return (
    <div className="transfer-panel">
      <div className="transfer-header">
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <span className="label-xs">Transfers</span>
          <span className="tp-aggregate" style={{ color: "var(--text-3)", fontSize: 12 }}>
            {counts.active} active
            {counts.paused > 0 ? ` (${counts.paused} paused)` : ""} •{" "}
            {counts.queued} queued • {counts.completed} completed
          </span>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          <button
            className="btn btn-ghost btn-sm"
            onClick={clearCompleted}
            disabled={!hasClearable}
            title="Clear completed, cancelled, and errored transfers"
          >
            Clear completed
          </button>
          <button
            className="btn btn-ghost btn-icon"
            onClick={close}
            aria-label="Close transfer panel"
          >
            <X size={14} />
          </button>
        </div>
      </div>
      <div className="transfer-list">
        {sorted.length === 0 && (
          <div className="tp-empty">
            <div className="tp-empty-title">No transfers yet.</div>
            <div className="tp-empty-sub dim">
              Drop files here or use the Upload button.
            </div>
          </div>
        )}
        {sorted.map((t) => (
          <TransferRow t={t} key={t.id} />
        ))}
      </div>
    </div>
  );
}
