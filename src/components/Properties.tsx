import { useEffect, useState } from "react";
import { X, Folder, File as FileIcon, Link as LinkIcon } from "lucide-react";
import type { DirEntry } from "../lib/api";

interface PropertiesPayload {
  tab: { id: string; name?: string };
  entry: DirEntry;
}

function fmtSize(n: number) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(2)} GB`;
}

function fmtDate(ts: number | null) {
  if (!ts) return "—";
  const d = new Date(ts * 1000);
  return d.toLocaleString();
}

function fmtPermsSymbolic(mode: number | null) {
  if (mode == null) return "—";
  const p = mode & 0o777;
  const out = [];
  for (let shift = 6; shift >= 0; shift -= 3) {
    const x = (p >> shift) & 7;
    out.push((x & 4 ? "r" : "-") + (x & 2 ? "w" : "-") + (x & 1 ? "x" : "-"));
  }
  return out.join("");
}
function fmtPermsOctal(mode: number | null) {
  if (mode == null) return "—";
  return (mode & 0o777).toString(8).padStart(3, "0");
}

export function Properties() {
  const [data, setData] = useState<PropertiesPayload | null>(null);

  useEffect(() => {
    const onShow = (ev: Event) => {
      const detail = (ev as CustomEvent<PropertiesPayload>).detail;
      if (detail?.entry) setData(detail);
    };
    window.addEventListener("skyhook:show-properties", onShow as EventListener);
    return () =>
      window.removeEventListener(
        "skyhook:show-properties",
        onShow as EventListener,
      );
  }, []);

  useEffect(() => {
    if (!data) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        setData(null);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [data]);

  if (!data) return null;
  const { entry } = data;
  const kind = entry.is_symlink
    ? "Symlink"
    : entry.is_dir
      ? "Folder"
      : "File";
  const Icon = entry.is_symlink
    ? LinkIcon
    : entry.is_dir
      ? Folder
      : FileIcon;

  const close = () => setData(null);

  return (
    <div className="modal-backdrop" onClick={close}>
      <div
        className="modal"
        style={{ width: 480 }}
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-label="File properties"
      >
        <div className="modal-header">
          <div
            className="modal-title"
            style={{ display: "flex", alignItems: "center", gap: 10 }}
          >
            <Icon size={18} />
            <span style={{ wordBreak: "break-all" }}>{entry.name}</span>
          </div>
          <button className="btn btn-ghost btn-icon" onClick={close} title="Close">
            <X size={14} />
          </button>
        </div>
        <div className="modal-body">
          <div className="properties-grid">
            <div className="properties-label">Type</div>
            <div className="properties-value">{kind}</div>

            <div className="properties-label">Path</div>
            <div className="properties-value mono">{entry.path}</div>

            {!entry.is_dir && (
              <>
                <div className="properties-label">Size</div>
                <div className="properties-value">
                  {fmtSize(entry.size)}{" "}
                  <span style={{ color: "var(--text-3)" }}>
                    ({entry.size.toLocaleString()} bytes)
                  </span>
                </div>
              </>
            )}

            <div className="properties-label">Modified</div>
            <div className="properties-value">{fmtDate(entry.modified)}</div>

            <div className="properties-label">Permissions</div>
            <div className="properties-value mono">
              {fmtPermsSymbolic(entry.mode)}{" "}
              <span style={{ color: "var(--text-3)" }}>
                ({fmtPermsOctal(entry.mode)})
              </span>
            </div>
          </div>
        </div>
        <div className="modal-footer" style={{ justifyContent: "flex-end" }}>
          <button className="btn btn-primary" onClick={close}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
