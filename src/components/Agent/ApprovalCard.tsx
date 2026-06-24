import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import DiffPreview from "./DiffPreview";

interface Props {
  callId: string;
  toolName: string;
  args: any;
  preview: string;
  sessionId: string | null;
  onApprove(callId: string): Promise<void>;
  onReject(callId: string, reason: string): Promise<void>;
}

function WarnIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" />
      <line x1="12" y1="9" x2="12" y2="13" />
      <line x1="12" y1="17" x2="12.01" y2="17" />
    </svg>
  );
}

function pathsFromArgs(toolName: string, args: any): string[] {
  if (!args || typeof args !== "object") return [];
  const out: string[] = [];
  const push = (v: any) => {
    if (typeof v === "string" && v.length > 0) out.push(v);
  };
  switch (toolName) {
    case "sftp_remove":
      push(args.path);
      break;
    case "sftp_make_dir":
      push(args.path);
      break;
    case "sftp_rename":
      push(args.from || args.from_path);
      push(args.to || args.to_path);
      break;
    case "sftp_download":
      push(args.remote_path || args.path);
      push(args.local_path);
      break;
    case "sftp_upload":
      push(args.local_path);
      push(args.remote_path || args.path);
      break;
    default:
      push(args.path);
  }
  return out;
}

export default function ApprovalCard({
  callId,
  toolName,
  args,
  preview,
  sessionId,
  onApprove,
  onReject,
}: Props) {
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState<"approve" | "reject" | null>(null);
  const [showReason, setShowReason] = useState(false);

  const [existing, setExisting] = useState<string | null>(null);
  const [existingLoaded, setExistingLoaded] = useState(false);
  const [existingError, setExistingError] = useState<string | null>(null);

  const isWrite = toolName === "sftp_write_file";
  const isShell = toolName === "shell_exec";

  useEffect(() => {
    let cancelled = false;
    if (!isWrite) return;
    if (!sessionId) {
      setExisting("");
      setExistingLoaded(true);
      return;
    }
    const path = typeof args?.path === "string" ? args.path : null;
    if (!path) {
      setExisting("");
      setExistingLoaded(true);
      return;
    }
    invoke<string>("read_file", { sessionId, path })
      .then((content) => {
        if (cancelled) return;
        setExisting(content);
        setExistingLoaded(true);
      })
      .catch((err) => {
        if (cancelled) return;
        const msg = String(err ?? "");
        // Treat not-found as empty (new file).
        if (/no such file|not found|enoent|does not exist/i.test(msg)) {
          setExisting("");
        } else {
          setExisting("");
          setExistingError(msg || "Failed to read current file");
        }
        setExistingLoaded(true);
      });
    return () => {
      cancelled = true;
    };
  }, [isWrite, sessionId, args?.path]);

  const handleApprove = async () => {
    if (busy) return;
    setBusy("approve");
    try {
      await onApprove(callId);
    } finally {
      setBusy(null);
    }
  };

  const handleReject = async () => {
    if (busy) return;
    setBusy("reject");
    try {
      await onReject(callId, reason.trim() || "No reason given");
    } finally {
      setBusy(null);
    }
  };

  const paths = pathsFromArgs(toolName, args);

  return (
    <div className="approval-card" role="region" aria-label={`Approval required for ${toolName}`}>
      <div className="approval-card__header">
        <span className="approval-card__warn" aria-hidden>
          <WarnIcon />
        </span>
        <span className="approval-card__title">Awaiting approval</span>
        <span className="approval-card__tool">{toolName}</span>
      </div>

      {preview ? <div className="approval-card__preview">{preview}</div> : null}

      {isShell ? (
        <pre className="approval-card__code" aria-label="Command to run">
          {typeof args?.command === "string" ? args.command : JSON.stringify(args)}
        </pre>
      ) : null}

      {!isWrite && !isShell && paths.length > 0 ? (
        <ul className="approval-card__paths" aria-label="Affected paths">
          {paths.map((p, i) => (
            <li key={i} className="approval-card__path">
              {p}
            </li>
          ))}
        </ul>
      ) : null}

      {isWrite ? (
        <div className="approval-card__diff">
          {!existingLoaded ? (
            <div className="approval-card__diff-loading">Loading current file…</div>
          ) : (
            <>
              {existingError ? (
                <div className="approval-card__diff-warning">
                  Could not read current contents: {existingError}. Showing diff against empty file.
                </div>
              ) : null}
              <DiffPreview
                path={typeof args?.path === "string" ? args.path : "(unknown)"}
                oldText={existing ?? ""}
                newText={typeof args?.content === "string" ? args.content : ""}
              />
            </>
          )}
        </div>
      ) : null}

      <div className="approval-card__footer">
        <button
          type="button"
          className="approval-card__btn approval-card__btn--primary"
          onClick={handleApprove}
          disabled={busy !== null}
          aria-label="Approve tool call"
        >
          {busy === "approve" ? "Approving…" : "Approve"}
        </button>
        <button
          type="button"
          className="approval-card__btn approval-card__btn--ghost"
          onClick={handleReject}
          disabled={busy !== null}
          aria-label="Reject tool call"
        >
          {busy === "reject" ? "Rejecting…" : "Reject"}
        </button>
        <button
          type="button"
          className="approval-card__reason-toggle"
          onClick={() => setShowReason((v) => !v)}
          aria-expanded={showReason}
          aria-controls={`approval-reason-${callId}`}
        >
          {showReason ? "Hide reason" : "Add reason"}
        </button>
      </div>

      {showReason ? (
        <textarea
          id={`approval-reason-${callId}`}
          className="approval-card__reason"
          placeholder="Optional reason for rejection…"
          value={reason}
          onChange={(e) => setReason(e.target.value)}
          rows={2}
          aria-label="Rejection reason"
        />
      ) : null}
    </div>
  );
}
