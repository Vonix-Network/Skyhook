import { useEffect, useRef, useState } from "react";
import { Send, Square } from "lucide-react";
import { useAgentStore, ApprovalMode } from "../../lib/agent-store";

export function Composer() {
  const conversation = useAgentStore((s) => s.conversation);
  const streaming = useAgentStore((s) => s.streaming);
  const settings = useAgentStore((s) => s.settings);
  const send = useAgentStore((s) => s.send);
  const cancel = useAgentStore((s) => s.cancel);

  const [text, setText] = useState("");
  const ref = useRef<HTMLTextAreaElement>(null);

  // Autosize
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = Math.min(220, el.scrollHeight) + "px";
  }, [text]);

  const disabled = !conversation || streaming;
  const approvalMode: ApprovalMode =
    settings?.default_approval_mode ?? "manual";

  const submit = () => {
    const value = text.trim();
    if (!value || !conversation) return;
    send(value, approvalMode).catch(() => {});
    setText("");
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      submit();
    }
  };

  return (
    <div className="agent-composer">
      <textarea
        ref={ref}
        value={text}
        placeholder={
          conversation
            ? "Message the agent…  (Ctrl+Enter to send)"
            : "Start a new conversation first"
        }
        rows={2}
        spellCheck
        onChange={(e) => setText(e.target.value)}
        onKeyDown={onKeyDown}
        disabled={!conversation}
      />
      <div className="agent-composer-row">
        <span className="agent-mode-chip" title="Approval mode">
          {approvalMode === "manual"
            ? "manual"
            : approvalMode === "auto_read"
              ? "auto-read"
              : "yolo"}
        </span>
        <div style={{ flex: 1 }} />
        {streaming ? (
          <button
            className="btn btn-danger"
            onClick={() => cancel()}
            title="Cancel current turn"
          >
            <Square size={14} /> Cancel
          </button>
        ) : (
          <button
            className="btn btn-primary"
            onClick={submit}
            disabled={disabled || !text.trim()}
            title="Send (Ctrl+Enter)"
          >
            <Send size={14} /> Send
          </button>
        )}
      </div>
    </div>
  );
}
