import { useStore } from "../lib/store";
import { X, Upload, Download, AlertCircle } from "lucide-react";

function fmtBytes(n: number) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(2)} GB`;
}

export function TransferPanel() {
  const transfers = useStore((s) => s.transfers);
  const close = useStore((s) => s.toggleTransfersPanel);

  return (
    <div className="transfer-panel">
      <div className="transfer-header">
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <span className="label-xs">Transfers</span>
          <span style={{ color: "var(--text-3)", fontSize: 12 }}>
            {transfers.length} item(s)
          </span>
        </div>
        <button className="btn btn-ghost btn-icon" onClick={close}>
          <X size={14} />
        </button>
      </div>
      <div className="transfer-list">
        {transfers.length === 0 && (
          <div style={{ padding: 40, textAlign: "center", color: "var(--text-3)" }}>
            No transfers yet.
          </div>
        )}
        {transfers
          .slice()
          .reverse()
          .map((t) => (
            <div className="transfer-row" key={t.id} title={t.error ?? undefined}>
              {t.direction === "upload" ? (
                <Upload size={14} className="icon-file" />
              ) : (
                <Download size={14} className="icon-file" />
              )}
              <div className="transfer-name">
                {t.direction === "upload" ? t.local : t.remote}
                <div className="dim">
                  → {t.direction === "upload" ? t.remote : t.local}
                </div>
              </div>
              <div className="mono dim" style={{ textAlign: "right" }}>
                {t.status === "done"
                  ? fmtBytes(t.bytes)
                  : t.total != null
                  ? fmtBytes(t.total)
                  : "—"}
              </div>
              <div className={`status-pill status-${t.status}`}>
                {t.status === "error" ? (
                  <span style={{ display: "inline-flex", alignItems: "center", gap: 4 }}>
                    <AlertCircle size={10} /> err
                  </span>
                ) : (
                  t.status
                )}
              </div>
            </div>
          ))}
      </div>
    </div>
  );
}
