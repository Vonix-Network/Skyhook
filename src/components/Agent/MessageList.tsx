import { useEffect, useMemo, useRef } from "react";
import { useAgentStore, AgentMessage } from "../../lib/agent-store";
import { Message } from "./Message";
import ApprovalCard from "./ApprovalCard";
import { useStore } from "../../lib/store";

function ApprovalSlot() {
  const pending = useAgentStore((s) => s.pendingApproval);
  const approve = useAgentStore((s) => s.approve);
  const reject = useAgentStore((s) => s.reject);
  const activeTabId = useStore((s) => s.activeTabId);
  if (!pending) return null;
  return (
    <ApprovalCard
      callId={pending.call_id}
      toolName={pending.tool_name}
      args={pending.args}
      preview={pending.preview}
      sessionId={activeTabId}
      onApprove={(id: string) => approve(id)}
      onReject={(id: string, reason: string) => reject(id, reason)}
    />
  );
}

export function MessageList() {
  const conversation = useAgentStore((s) => s.conversation);
  const streamingText = useAgentStore((s) => s.streamingText);
  const streamingThinking = useAgentStore((s) => s.streamingThinking);
  const streaming = useAgentStore((s) => s.streaming);
  const showThinking = useAgentStore(
    (s) => s.settings?.show_thinking ?? false,
  );

  const scrollRef = useRef<HTMLDivElement>(null);
  const messages = conversation?.messages ?? [];

  // Auto-scroll to bottom on new content; respect user scroll-up.
  const stickyRef = useRef(true);
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const onScroll = () => {
      const near =
        el.scrollHeight - el.scrollTop - el.clientHeight < 40;
      stickyRef.current = near;
    };
    el.addEventListener("scroll", onScroll, { passive: true });
    return () => el.removeEventListener("scroll", onScroll);
  }, []);

  useEffect(() => {
    if (!stickyRef.current) return;
    const el = scrollRef.current;
    if (!el) return;
    // rAF batch to avoid jitter while streaming
    const id = requestAnimationFrame(() => {
      el.scrollTop = el.scrollHeight;
    });
    return () => cancelAnimationFrame(id);
  }, [messages, streamingText, streamingThinking]);

  const rendered = useMemo(() => messages, [messages]);

  if (!conversation) {
    return (
      <div className="agent-empty">
        <div className="agent-empty-title">No conversation</div>
        <div className="agent-empty-sub">
          Start a new conversation to chat with the agent.
        </div>
      </div>
    );
  }

  if (rendered.length === 0 && !streamingText && !streamingThinking) {
    return (
      <div className="agent-list" ref={scrollRef}>
        <div className="agent-empty">
          <div className="agent-empty-title">Ready</div>
          <div className="agent-empty-sub">
            Ask the agent to inspect, modify, or transfer files.
          </div>
        </div>
      </div>
    );
  }

  // Render full message list. If > 50 messages we apply a simple windowing:
  // show the last 50 + a "show earlier" placeholder.
  const useWindow = rendered.length > 50;
  const visible = useWindow ? rendered.slice(-50) : rendered;

  return (
    <div className="agent-list" ref={scrollRef}>
      {useWindow && (
        <div className="agent-window-note">
          {rendered.length - 50} earlier messages hidden
        </div>
      )}
      {visible.map((m: AgentMessage, idx: number) => (
        <Message key={idx} message={m} />
      ))}
      {(streaming || streamingText || streamingThinking) && (
        <Message
          message={{
            role: "assistant",
            timestamp: Date.now() / 1000,
            content: [
              ...(showThinking && streamingThinking
                ? ([{ type: "thinking", thinking: streamingThinking }] as any)
                : []),
              ...(streamingText
                ? ([{ type: "text", text: streamingText }] as any)
                : []),
            ],
          }}
          streaming={streaming}
        />
      )}
      <ApprovalSlot />
    </div>
  );
}
