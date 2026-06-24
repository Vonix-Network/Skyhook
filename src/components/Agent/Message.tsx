import { AgentMessage, MessageContent, Usage } from "../../lib/agent-store";

// Try-load sibling E's ToolCallCard if it exists; fall back to inline.
function ToolCallSlot({
  block,
}: {
  block: Extract<MessageContent, { type: "tool_use" }>;
}) {
  return (
    <div className="agent-tool-call">
      <div className="agent-tool-head">
        <span className="agent-tool-label">tool</span>
        <code className="agent-tool-name">{block.name}</code>
      </div>
      <pre className="agent-tool-args">
        {safeJson(block.input)}
      </pre>
    </div>
  );
}

function ToolResultBlock({
  block,
}: {
  block: Extract<MessageContent, { type: "tool_result" }>;
}) {
  return (
    <div
      className={
        "agent-tool-result" + (block.is_error ? " is-error" : "")
      }
    >
      <div className="agent-tool-head">
        <span className="agent-tool-label">
          {block.is_error ? "error" : "result"}
        </span>
      </div>
      <pre className="agent-tool-output">{block.content}</pre>
    </div>
  );
}

function ThinkingBlock({
  block,
}: {
  block: Extract<MessageContent, { type: "thinking" }>;
}) {
  return (
    <details className="agent-thinking" open>
      <summary>thinking</summary>
      <pre>{block.thinking}</pre>
    </details>
  );
}

function safeJson(v: any): string {
  try {
    const s = JSON.stringify(v, null, 2);
    if (s == null) return String(v);
    return s.length > 4000 ? s.slice(0, 4000) + "\n…" : s;
  } catch {
    return String(v);
  }
}

function fmtUsage(u: Usage): string {
  const parts = [
    `in ${u.input_tokens}`,
    `out ${u.output_tokens}`,
  ];
  if (u.cache_read_tokens) parts.push(`cache ${u.cache_read_tokens}`);
  return parts.join(" / ");
}

export function Message({
  message,
  streaming,
}: {
  message: AgentMessage;
  streaming?: boolean;
}) {
  const isUser = message.role === "user";
  const isSystem = message.role === "system";

  // For user messages, render text only (tool_result blocks live here in
  // canonical anthropic ordering but we already show them in assistant lane).
  if (isUser) {
    const text = message.content
      .filter((c) => c.type === "text")
      .map((c) => (c as any).text)
      .join("\n");
    if (!text) return null;
    return (
      <div className="agent-msg agent-msg-user">
        <div className="agent-msg-bubble">
          <pre className="agent-msg-text">{text}</pre>
        </div>
      </div>
    );
  }

  if (isSystem) {
    const text = message.content
      .filter((c) => c.type === "text")
      .map((c) => (c as any).text)
      .join("\n");
    return (
      <div className="agent-msg agent-msg-system">
        <div className="agent-msg-bubble">{text}</div>
      </div>
    );
  }

  // assistant
  return (
    <div className="agent-msg agent-msg-assistant">
      <div className="agent-msg-bubble">
        {message.content.map((block, i) => {
          switch (block.type) {
            case "text":
              return (
                <pre key={i} className="agent-msg-text">
                  {block.text}
                  {streaming && i === message.content.length - 1 && (
                    <span className="agent-cursor" />
                  )}
                </pre>
              );
            case "thinking":
              return <ThinkingBlock key={i} block={block} />;
            case "tool_use":
              return <ToolCallSlot key={i} block={block} />;
            case "tool_result":
              return <ToolResultBlock key={i} block={block} />;
            default:
              return null;
          }
        })}
        {streaming && message.content.length === 0 && (
          <span className="agent-cursor" />
        )}
        {message.usage && (
          <div className="agent-usage" title="Token usage">
            {fmtUsage(message.usage)}
          </div>
        )}
      </div>
    </div>
  );
}
