import { useStore } from "../lib/store";
import { useState, useEffect, useRef } from "react";
import {
  Server,
  Plus,
  Settings2,
  PanelLeftClose,
  PanelLeftOpen,
  Trash2,
  Pencil,
  Download,
  Upload as UploadIcon,
  Download as DownloadFileIcon,
  ShieldCheck,
} from "lucide-react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";

async function readTextFile(path: string): Promise<string> {
  // Backend exposes a generic file read (used elsewhere for connection JSON).
  // Use a small Tauri command if available; otherwise fall back to fetch().
  try {
    return await invoke<string>("read_local_text_file", { path });
  } catch {
    const res = await fetch(`file://${path}`);
    return await res.text();
  }
}

async function writeTextFile(path: string, contents: string): Promise<void> {
  try {
    await invoke<void>("write_local_text_file", { path, contents });
  } catch (e) {
    // Last-resort: trigger a browser download.
    const blob = new Blob([contents], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = path.split(/[\\/]/).pop() || "skyhook-connections.json";
    a.click();
    URL.revokeObjectURL(url);
    throw e;
  }
}

type ApprovalOverride = "default" | "manual" | "auto_read" | "yolo";

function getApprovalOverride(connectionId: string): ApprovalOverride {
  try {
    const v = localStorage.getItem(`skyhook.approval.${connectionId}`);
    if (v === "manual" || v === "auto_read" || v === "yolo") return v;
  } catch {
    /* ignore */
  }
  return "default";
}

function setApprovalOverride(connectionId: string, v: ApprovalOverride) {
  try {
    if (v === "default") localStorage.removeItem(`skyhook.approval.${connectionId}`);
    else localStorage.setItem(`skyhook.approval.${connectionId}`, v);
  } catch {
    /* ignore */
  }
}

function ApprovalModeMenu({ connectionId }: { connectionId: string }) {
  const [open, setOpen] = useState(false);
  const [current, setCurrent] = useState<ApprovalOverride>(() =>
    getApprovalOverride(connectionId),
  );
  const ref = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, [open]);

  const pick = (v: ApprovalOverride) => {
    if (v === "yolo") {
      const typed = window.prompt(
        "Yolo mode auto-approves ALL tool calls.\nType YOLO to confirm:",
      );
      if (typed !== "YOLO") {
        setOpen(false);
        return;
      }
    }
    setApprovalOverride(connectionId, v);
    setCurrent(v);
    setOpen(false);
  };

  const labelOf = (v: ApprovalOverride) =>
    v === "default"
      ? "Default"
      : v === "manual"
      ? "Manual"
      : v === "auto_read"
      ? "Auto-read"
      : "Yolo";

  return (
    <div className="approval-menu-wrap" ref={ref}>
      <button
        className={`btn btn-ghost btn-icon ${current !== "default" ? "approval-active" : ""}`}
        onClick={(e) => {
          e.stopPropagation();
          setOpen((o) => !o);
        }}
        title={`Approval mode: ${labelOf(current)}`}
      >
        <ShieldCheck size={13} />
      </button>
      {open && (
        <div
          className="approval-menu"
          onClick={(e) => e.stopPropagation()}
          role="menu"
        >
          <div className="approval-menu-title">Approval mode</div>
          {(["default", "manual", "auto_read", "yolo"] as ApprovalOverride[]).map(
            (v) => (
              <button
                key={v}
                className={`approval-menu-item ${current === v ? "selected" : ""} ${
                  v === "yolo" ? "danger" : ""
                }`}
                onClick={() => pick(v)}
                role="menuitemradio"
                aria-checked={current === v}
              >
                {labelOf(v)}
                {v === "default" && (
                  <span className="approval-menu-hint">use global</span>
                )}
              </button>
            ),
          )}
        </div>
      )}
    </div>
  );
}

export function Sidebar() {
  const collapsed = useStore((s) => s.sidebarCollapsed);
  const toggle = useStore((s) => s.toggleSidebar);
  const connections = useStore((s) => s.connections);
  const tabs = useStore((s) => s.tabs);
  const open = useStore((s) => s.openConnection);
  const openForm = useStore((s) => s.openConnectionForm);
  const remove = useStore((s) => s.deleteConnection);
  const toggleTransfers = useStore((s) => s.toggleTransfersPanel);
  const transfers = useStore((s) => s.transfers);
  const loadConnections = useStore((s) => s.loadConnections);
  const toast = useStore((s) => s.toast);
  const openSettings = useStore((s) => s.openSettings);
  const activeCount = transfers.filter((t) => t.status === "active" || t.status === "queued").length;

  const onlineConnIds = new Set(tabs.map((t) => t.connectionId));

  const doExport = async () => {
    try {
      const json = await invoke<string>("export_connections");
      const target = await saveDialog({
        defaultPath: "skyhook-connections.json",
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!target) return;
      await writeTextFile(target, json);
      toast("Connections exported", "success");
    } catch (e: any) {
      toast(`Export failed: ${e?.message ?? e}`, "error");
    }
  };

  const doImport = async () => {
    try {
      const picked = await openDialog({
        multiple: false,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!picked || Array.isArray(picked)) return;
      const json = await readTextFile(picked as string);
      const result = await invoke<{ added: number; skipped: number }>(
        "import_connections",
        { json },
      );
      await loadConnections();
      toast(
        `${result.added} added, ${result.skipped} skipped`,
        "success",
      );
    } catch (e: any) {
      toast(`Import failed: ${e?.message ?? e}`, "error");
    }
  };

  return (
    <aside className={`sidebar ${collapsed ? "collapsed" : ""}`}>
      <div className="sidebar-header">
        <div className="brand-mark">S</div>
        <div className="brand-name">Skyhook</div>
        <button
          className="btn btn-ghost btn-icon"
          style={{ marginLeft: "auto" }}
          onClick={toggle}
          title={collapsed ? "Expand" : "Collapse"}
        >
          {collapsed ? <PanelLeftOpen size={16} /> : <PanelLeftClose size={16} />}
        </button>
      </div>

      <div className="sidebar-section" style={{ flex: 1, overflowY: "auto" }}>
        <div className="sidebar-section-header">
          <span className="label-xs">Connections</span>
          <div style={{ display: "flex", gap: 2, marginLeft: "auto" }}>
            <button
              className="btn btn-ghost btn-icon"
              onClick={doImport}
              title="Import connections from JSON"
            >
              <UploadIcon size={13} />
            </button>
            <button
              className="btn btn-ghost btn-icon"
              onClick={doExport}
              title="Export connections to JSON"
            >
              <DownloadFileIcon size={13} />
            </button>
            <button
              className="btn btn-ghost btn-icon"
              onClick={() => openForm(null)}
              title="New connection"
            >
              <Plus size={14} />
            </button>
          </div>
        </div>
        <div className="conn-list">
          {connections.length === 0 && !collapsed && (
            <div style={{ padding: "12px 10px", color: "var(--text-3)", fontSize: 12 }}>
              No connections yet.
              <br />
              Click <Plus size={11} style={{ verticalAlign: "middle" }} /> to add one.
            </div>
          )}
          {connections.map((c) => (
            <div
              key={c.id}
              className={`conn-item ${onlineConnIds.has(c.id) ? "active" : ""}`}
              onDoubleClick={() => open(c.id).catch((e) => alert(e?.message ?? e))}
              title={`${c.username}@${c.host}:${c.port}`}
            >
              <span
                className={`conn-dot ${onlineConnIds.has(c.id) ? "online" : ""}`}
              />
              <div className="conn-info">
                <div className="name">{c.name}</div>
                <div className="host">
                  {c.username}@{c.host}
                </div>
              </div>
              <div className="conn-actions">
                <ApprovalModeMenu connectionId={c.id} />
                <button
                  className="btn btn-ghost btn-icon"
                  onClick={(e) => {
                    e.stopPropagation();
                    open(c.id).catch((err) => alert(err?.message ?? err));
                  }}
                  title="Connect"
                >
                  <Server size={13} />
                </button>
                <button
                  className="btn btn-ghost btn-icon"
                  onClick={(e) => {
                    e.stopPropagation();
                    openForm(c);
                  }}
                  title="Edit"
                >
                  <Pencil size={13} />
                </button>
                <button
                  className="btn btn-ghost btn-icon btn-danger"
                  onClick={(e) => {
                    e.stopPropagation();
                    if (confirm(`Delete connection "${c.name}"?`)) remove(c.id);
                  }}
                  title="Delete"
                >
                  <Trash2 size={13} />
                </button>
              </div>
            </div>
          ))}
        </div>
      </div>

      <div className="sidebar-footer">
        <button
          className="btn btn-ghost"
          onClick={toggleTransfers}
          title="Transfers"
          style={{ flex: 1, justifyContent: collapsed ? "center" : "flex-start" }}
        >
          <Download size={15} />
          {!collapsed && (
            <span style={{ marginLeft: 8 }}>
              Transfers{activeCount > 0 ? ` (${activeCount})` : ""}
            </span>
          )}
        </button>
        {!collapsed && (
          <button
            className="btn btn-ghost btn-icon"
            onClick={openSettings}
            title="Settings"
          >
            <Settings2 size={15} />
          </button>
        )}
      </div>
    </aside>
  );
}
