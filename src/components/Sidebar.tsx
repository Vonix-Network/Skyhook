import { useStore } from "../lib/store";
import {
  Server,
  Plus,
  Settings2,
  PanelLeftClose,
  PanelLeftOpen,
  Trash2,
  Pencil,
  Download,
} from "lucide-react";

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
  const activeCount = transfers.filter((t) => t.status === "active" || t.status === "queued").length;

  const onlineConnIds = new Set(tabs.map((t) => t.connectionId));

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
          <button
            className="btn btn-ghost btn-icon"
            onClick={() => openForm(null)}
            title="New connection"
          >
            <Plus size={14} />
          </button>
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
          <button className="btn btn-ghost btn-icon" title="Settings (soon)">
            <Settings2 size={15} />
          </button>
        )}
      </div>
    </aside>
  );
}
