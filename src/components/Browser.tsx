import { useState, useEffect, useRef } from "react";
import { Tab, useStore } from "../lib/store";
import {
  ChevronLeft,
  ChevronRight,
  ArrowUp,
  RefreshCw,
  FolderPlus,
  Upload,
  Download as DownloadIcon,
  Trash2,
  Folder,
  File as FileIcon,
  Link as LinkIcon,
} from "lucide-react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/api";

function fmtSize(n: number) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(2)} GB`;
}

function fmtDate(ts: number | null) {
  if (!ts) return "—";
  const d = new Date(ts * 1000);
  const now = new Date();
  const sameYear = d.getFullYear() === now.getFullYear();
  return d.toLocaleString(undefined, {
    month: "short",
    day: "2-digit",
    year: sameYear ? undefined : "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function fmtMode(mode: number | null) {
  if (mode == null) return "—";
  const p = mode & 0o777;
  const out = [];
  for (let shift = 6; shift >= 0; shift -= 3) {
    const x = (p >> shift) & 7;
    out.push((x & 4 ? "r" : "-") + (x & 2 ? "w" : "-") + (x & 1 ? "x" : "-"));
  }
  return out.join("");
}

export function Browser({ tab }: { tab: Tab }) {
  const navigate = useStore((s) => s.navigate);
  const refresh = useStore((s) => s.refresh);
  const back = useStore((s) => s.goBack);
  const forward = useStore((s) => s.goForward);
  const up = useStore((s) => s.goUp);
  const toggleSelect = useStore((s) => s.toggleSelect);
  const enqueueTransfer = useStore((s) => s.enqueueTransfer);
  const updateTransfer = useStore((s) => s.updateTransfer);

  const [pathInput, setPathInput] = useState(tab.cwd);
  const pathRef = useRef<HTMLInputElement>(null);
  useEffect(() => setPathInput(tab.cwd), [tab.cwd]);

  const onPathSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    navigate(tab.id, pathInput);
  };

  const onRowDblClick = (e: any, entry: any) => {
    if (entry.is_dir) navigate(tab.id, entry.path);
  };

  const doUpload = async () => {
    const selected = await openDialog({ multiple: true, directory: false });
    if (!selected) return;
    const files = Array.isArray(selected) ? selected : [selected];
    for (const local of files) {
      const filename = local.split(/[\\/]/).pop()!;
      const remote = tab.cwd.endsWith("/")
        ? tab.cwd + filename
        : tab.cwd + "/" + filename;
      const id = crypto.randomUUID();
      enqueueTransfer({
        sessionId: tab.id,
        direction: "upload",
        local,
        remote,
        total: null,
      });
      // mark active + run
      // (enqueueTransfer doesn't return; small race: find last item we just added)
      // we'll patch by index — store sets startedAt unique-enough, search:
      const recent = useStore.getState().transfers.find((t) => t.local === local && t.remote === remote && t.status === "queued");
      if (recent) {
        updateTransfer(recent.id, { status: "active" });
        api
          .uploadFile(tab.id, local, remote)
          .then((bytes) => {
            updateTransfer(recent.id, { status: "done", bytes, total: bytes });
            refresh(tab.id);
          })
          .catch((err) =>
            updateTransfer(recent.id, { status: "error", error: err?.message ?? String(err) })
          );
      }
    }
  };

  const doDownload = async () => {
    if (tab.selected.size === 0) return alert("Select a file to download.");
    for (const remote of tab.selected) {
      const entry = tab.entries.find((e) => e.path === remote);
      if (!entry || entry.is_dir) continue;
      const local = await saveDialog({ defaultPath: entry.name });
      if (!local) continue;
      enqueueTransfer({
        sessionId: tab.id,
        direction: "download",
        local,
        remote,
        total: entry.size,
      });
      const recent = useStore.getState().transfers.find((t) => t.local === local && t.remote === remote && t.status === "queued");
      if (recent) {
        updateTransfer(recent.id, { status: "active" });
        api
          .downloadFile(tab.id, remote, local)
          .then((bytes) =>
            updateTransfer(recent.id, { status: "done", bytes, total: bytes })
          )
          .catch((err) =>
            updateTransfer(recent.id, { status: "error", error: err?.message ?? String(err) })
          );
      }
    }
  };

  const doMkdir = async () => {
    const name = prompt("New folder name:");
    if (!name) return;
    const full = tab.cwd.endsWith("/") ? tab.cwd + name : tab.cwd + "/" + name;
    try {
      await api.makeDir(tab.id, full);
      refresh(tab.id);
    } catch (e: any) {
      alert(e?.message ?? String(e));
    }
  };

  const doDelete = async () => {
    if (tab.selected.size === 0) return;
    if (!confirm(`Delete ${tab.selected.size} item(s)? This cannot be undone.`)) return;
    for (const p of tab.selected) {
      try {
        await api.remove(tab.id, p);
      } catch (e: any) {
        alert(`${p}: ${e?.message ?? e}`);
      }
    }
    refresh(tab.id);
  };

  return (
    <div className="browser">
      {!tab.connected && (
        <div className="reconnect-banner">
          <span>
            ⚠ Session closed by server. This can happen if too many SFTP
            sessions were opened simultaneously.
          </span>
          <button className="btn btn-primary" onClick={() => useStore.getState().reconnect(tab.id)}>
            Reconnect
          </button>
        </div>
      )}
      <div className="toolbar">
        <div className="nav-btns">
          <button
            className="btn btn-ghost btn-icon"
            disabled={tab.historyIndex === 0}
            onClick={() => back(tab.id)}
            title="Back"
          >
            <ChevronLeft size={16} />
          </button>
          <button
            className="btn btn-ghost btn-icon"
            disabled={tab.historyIndex >= tab.history.length - 1}
            onClick={() => forward(tab.id)}
            title="Forward"
          >
            <ChevronRight size={16} />
          </button>
          <button
            className="btn btn-ghost btn-icon"
            onClick={() => up(tab.id)}
            title="Parent directory"
          >
            <ArrowUp size={16} />
          </button>
          <button
            className="btn btn-ghost btn-icon"
            onClick={() => refresh(tab.id)}
            title="Refresh"
          >
            <RefreshCw size={14} />
          </button>
        </div>
        <form className="path-bar" onSubmit={onPathSubmit} style={{ flex: 1 }}>
          <input
            ref={pathRef}
            value={pathInput}
            onChange={(e) => setPathInput(e.target.value)}
            spellCheck={false}
          />
        </form>
        <button className="btn" onClick={doMkdir} title="New folder">
          <FolderPlus size={14} /> New
        </button>
        <button className="btn" onClick={doUpload} title="Upload from local">
          <Upload size={14} /> Upload
        </button>
        <button className="btn" onClick={doDownload} title="Download selected">
          <DownloadIcon size={14} /> Download
        </button>
        <button
          className="btn btn-danger"
          onClick={doDelete}
          disabled={tab.selected.size === 0}
          title="Delete selected"
        >
          <Trash2 size={14} />
        </button>
      </div>

      <div className="file-list-header">
        <div>Name</div>
        <div>Size</div>
        <div>Modified</div>
        <div>Perms</div>
      </div>

      <div className="file-list">
        {tab.loading && <div className="loading-state">Loading…</div>}
        {tab.error && <div className="error-state">Error: {tab.error}</div>}
        {!tab.loading && !tab.error && tab.entries.length === 0 && (
          <div className="empty-state">Empty directory</div>
        )}
        {!tab.loading &&
          !tab.error &&
          tab.entries.map((e) => {
            const selected = tab.selected.has(e.path);
            return (
              <div
                key={e.path}
                className={`file-row ${selected ? "selected" : ""}`}
                onClick={(ev) =>
                  toggleSelect(tab.id, e.path, ev.ctrlKey || ev.metaKey)
                }
                onDoubleClick={(ev) => onRowDblClick(ev, e)}
              >
                <div className="name-cell">
                  {e.is_symlink ? (
                    <LinkIcon size={15} className="icon-link" />
                  ) : e.is_dir ? (
                    <Folder size={15} className="icon-folder" />
                  ) : (
                    <FileIcon size={15} className="icon-file" />
                  )}
                  <span>{e.name}</span>
                </div>
                <div className="dim mono">{e.is_dir ? "—" : fmtSize(e.size)}</div>
                <div className="dim">{fmtDate(e.modified)}</div>
                <div className="dim mono">{fmtMode(e.mode)}</div>
              </div>
            );
          })}
      </div>
    </div>
  );
}
