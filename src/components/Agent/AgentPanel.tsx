import { useEffect, useRef, useState } from "react";
import { Plus, Settings as SettingsIcon, X, AlertTriangle } from "lucide-react";
import { useAgentStore } from "../../lib/agent-store";
import { useStore } from "../../lib/store";
import { Resizer } from "../Resizer";
import { MessageList } from "./MessageList";
import { Composer } from "./Composer";

const MIN_W = 300;
const MAX_W = 720;
const DEFAULT_W = 380;

export function AgentPanel() {
  const toggleAgent = useStore((s) => s.toggleAgent);
  const activeTabId = useStore((s) => s.activeTabId);
  const tabs = useStore((s) => s.tabs);
  const activeTab = tabs.find((t) => t.id === activeTabId) ?? null;
  const connections = useStore((s) => s.connections);
  const conn = activeTab
    ? connections.find((c) => c.id === activeTab.connectionId)
    : null;
  const connectionId = activeTab?.connectionId ?? null;

  const conversation = useAgentStore((s) => s.conversation);
  const error = useAgentStore((s) => s.error);
  const clearError = useAgentStore((s) => s.clearError);
  const loadSettings = useAgentStore((s) => s.loadSettings);
  const loadConversations = useAgentStore((s) => s.loadConversations);
  const newConversation = useAgentStore((s) => s.newConversation);
  const renameConversation = useAgentStore((s) => s.renameConversation);
  const subscribeAgentEvents = useAgentStore((s) => s.subscribeAgentEvents);

  // Width persistence (local; sibling F may eventually persist via settings).
  const [width, setWidth] = useState<number>(() => {
    const raw = localStorage.getItem("skyhook.agentPanelWidth");
    const n = raw ? Number(raw) : NaN;
    return Number.isFinite(n) && n >= MIN_W && n <= MAX_W ? n : DEFAULT_W;
  });
  useEffect(() => {
    localStorage.setItem("skyhook.agentPanelWidth", String(width));
  }, [width]);

  // Subscribe to backend events once panel mounts.
  const subbedRef = useRef(false);
  useEffect(() => {
    loadSettings().catch(() => {});
    if (subbedRef.current) return;
    subbedRef.current = true;
    const unsub = subscribeAgentEvents();
    return () => {
      subbedRef.current = false;
      unsub();
    };
  }, [loadSettings, subscribeAgentEvents]);

  // Refresh conversation list when active connection changes.
  useEffect(() => {
    if (connectionId) loadConversations(connectionId).catch(() => {});
  }, [connectionId, loadConversations]);

  // Title editing.
  const [editingTitle, setEditingTitle] = useState(false);
  const [titleDraft, setTitleDraft] = useState("");
  useEffect(() => {
    setTitleDraft(conversation?.title ?? "");
    setEditingTitle(false);
  }, [conversation?.id]);

  const onTitleCommit = () => {
    setEditingTitle(false);
    const t = titleDraft.trim();
    if (conversation && t && t !== conversation.title) {
      renameConversation(conversation.id, t).catch(() => {});
    } else {
      setTitleDraft(conversation?.title ?? "");
    }
  };

  const onNew = () => {
    if (!connectionId) return;
    newConversation(connectionId).catch(() => {});
  };

  const openSettings = () => {
    // Sibling F owns AgentSettings — they listen for this event.
    window.dispatchEvent(new CustomEvent("skyhook:open-agent-settings"));
  };

  return (
    <div className="agent-panel-wrap" style={{ width }}>
      <Resizer
        direction="horizontal"
        onResize={(dx) =>
          setWidth((w) => Math.max(MIN_W, Math.min(MAX_W, w - dx)))
        }
      />
      <div className="agent-panel">
        <div className="agent-topbar">
          <div className="agent-topbar-left">
            <div className="agent-conn" title={conn?.name ?? "No connection"}>
              {conn?.name ?? "—"}
            </div>
            {conversation ? (
              editingTitle ? (
                <input
                  className="agent-title-input"
                  autoFocus
                  value={titleDraft}
                  onChange={(e) => setTitleDraft(e.target.value)}
                  onBlur={onTitleCommit}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      onTitleCommit();
                    } else if (e.key === "Escape") {
                      setTitleDraft(conversation.title);
                      setEditingTitle(false);
                    }
                  }}
                />
              ) : (
                <div
                  className="agent-title"
                  title="Click to rename"
                  onClick={() => setEditingTitle(true)}
                >
                  {conversation.title || "Untitled"}
                </div>
              )
            ) : (
              <div className="agent-title agent-title-muted">
                No conversation
              </div>
            )}
          </div>
          <div className="agent-topbar-right">
            <button
              className="btn btn-ghost btn-icon"
              onClick={onNew}
              disabled={!connectionId}
              title="New conversation"
            >
              <Plus size={14} />
            </button>
            <button
              className="btn btn-ghost btn-icon"
              onClick={openSettings}
              title="Agent settings"
            >
              <SettingsIcon size={14} />
            </button>
            <button
              className="btn btn-ghost btn-icon"
              onClick={() => toggleAgent()}
              title="Close panel"
            >
              <X size={14} />
            </button>
          </div>
        </div>

        {error && (
          <div className="agent-error-banner">
            <AlertTriangle size={14} />
            <span>{error}</span>
            <button
              className="btn btn-ghost btn-icon"
              onClick={clearError}
              title="Dismiss"
            >
              <X size={12} />
            </button>
          </div>
        )}

        {!connectionId ? (
          <div className="agent-empty">
            <div className="agent-empty-title">No active session</div>
            <div className="agent-empty-sub">
              Open a connection to chat with the agent about it.
            </div>
          </div>
        ) : !conversation ? (
          <div className="agent-empty">
            <div className="agent-empty-title">No conversation yet</div>
            <div className="agent-empty-sub">
              Start one to talk to the agent about <b>{conn?.name}</b>.
            </div>
            <button
              className="btn btn-primary"
              style={{ marginTop: 12 }}
              onClick={onNew}
            >
              <Plus size={14} /> New conversation
            </button>
          </div>
        ) : (
          <MessageList />
        )}

        <Composer />
      </div>
    </div>
  );
}
