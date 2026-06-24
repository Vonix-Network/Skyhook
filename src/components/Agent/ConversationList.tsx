import { useEffect, useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Plus, RefreshCw, Pencil, Trash2 } from "lucide-react";

interface ConversationMeta {
  id: string;
  connection_id: string;
  title: string | null;
  provider: string;
  model: string;
  updated_at: number;
  created_at: number;
}

interface Props {
  connectionId: string | null;
  activeConversationId: string | null;
  onSelect(id: string): void;
  onNew(): void;
}

function relTime(ts: number): string {
  const now = Date.now() / 1000;
  const d = Math.max(0, now - ts);
  if (d < 60) return "just now";
  if (d < 3600) return `${Math.floor(d / 60)}m ago`;
  if (d < 86400) return `${Math.floor(d / 3600)}h ago`;
  if (d < 86400 * 7) return `${Math.floor(d / 86400)}d ago`;
  return new Date(ts * 1000).toLocaleDateString();
}

export function ConversationList({
  connectionId,
  activeConversationId,
  onSelect,
  onNew,
}: Props) {
  const [items, setItems] = useState<ConversationMeta[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editValue, setEditValue] = useState("");
  const editRef = useRef<HTMLInputElement | null>(null);

  const refresh = useCallback(async () => {
    if (!connectionId) {
      setItems([]);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const list = await invoke<ConversationMeta[]>("agent_list_conversations", {
        connectionId,
      });
      list.sort((a, b) => b.updated_at - a.updated_at);
      setItems(list);
    } catch (e: any) {
      setError(e?.message ?? String(e));
    } finally {
      setLoading(false);
    }
  }, [connectionId]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    if (editingId && editRef.current) {
      editRef.current.focus();
      editRef.current.select();
    }
  }, [editingId]);

  const commitRename = async (id: string) => {
    const title = editValue.trim();
    setEditingId(null);
    if (!title) return;
    try {
      await invoke("agent_rename_conversation", { conversationId: id, title });
      setItems((prev) =>
        prev.map((c) => (c.id === id ? { ...c, title } : c)),
      );
    } catch (e: any) {
      setError(e?.message ?? String(e));
    }
  };

  const doDelete = async (id: string, title: string) => {
    if (!window.confirm(`Delete conversation "${title || "Untitled"}"?`)) return;
    try {
      await invoke("agent_delete_conversation", { conversationId: id });
      setItems((prev) => prev.filter((c) => c.id !== id));
    } catch (e: any) {
      setError(e?.message ?? String(e));
    }
  };

  return (
    <div className="conv-list">
      <div className="conv-list-header">
        <span className="label-xs">Conversations</span>
        <div style={{ display: "flex", gap: 2, marginLeft: "auto" }}>
          <button
            className="btn btn-ghost btn-icon"
            onClick={refresh}
            title="Refresh"
            disabled={loading || !connectionId}
          >
            <RefreshCw size={13} className={loading ? "spin" : ""} />
          </button>
          <button
            className="btn btn-ghost btn-icon"
            onClick={onNew}
            title="New conversation"
            disabled={!connectionId}
          >
            <Plus size={14} />
          </button>
        </div>
      </div>

      {error && <div className="conv-error">{error}</div>}

      <div className="conv-list-body">
        {!connectionId && (
          <div className="conv-empty">Select a connection to see conversations.</div>
        )}
        {connectionId && items.length === 0 && !loading && (
          <div className="conv-empty">
            No conversations yet. Start one to get help on this server.
          </div>
        )}
        {items.map((c) => {
          const active = c.id === activeConversationId;
          const title = c.title || "New conversation";
          return (
            <div
              key={c.id}
              className={`conv-item ${active ? "active" : ""}`}
              onClick={() => {
                if (editingId !== c.id) onSelect(c.id);
              }}
              title={title}
            >
              <div className="conv-item-main">
                {editingId === c.id ? (
                  <input
                    ref={editRef}
                    className="conv-rename-input"
                    value={editValue}
                    onChange={(e) => setEditValue(e.target.value)}
                    onBlur={() => commitRename(c.id)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") commitRename(c.id);
                      else if (e.key === "Escape") setEditingId(null);
                      e.stopPropagation();
                    }}
                    onClick={(e) => e.stopPropagation()}
                  />
                ) : (
                  <div className="conv-title">{title}</div>
                )}
                <div className="conv-meta">
                  <span>{c.provider}</span>
                  <span className="conv-sep">·</span>
                  <span>{c.model}</span>
                  <span className="conv-sep">·</span>
                  <span>{relTime(c.updated_at)}</span>
                </div>
              </div>
              <div className="conv-actions">
                <button
                  className="btn btn-ghost btn-icon"
                  onClick={(e) => {
                    e.stopPropagation();
                    setEditingId(c.id);
                    setEditValue(c.title || "");
                  }}
                  title="Rename"
                >
                  <Pencil size={12} />
                </button>
                <button
                  className="btn btn-ghost btn-icon btn-danger"
                  onClick={(e) => {
                    e.stopPropagation();
                    doDelete(c.id, c.title || "");
                  }}
                  title="Delete"
                >
                  <Trash2 size={12} />
                </button>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
