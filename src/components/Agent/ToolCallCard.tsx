import { useState, useMemo } from "react";

interface Props {
  toolName: string;
  args: any;
  status: "running" | "awaiting_approval" | "done" | "error" | "rejected";
  output?: string;
  error?: string;
  collapsed?: boolean;
}

type IconKind = "folder" | "file" | "trash" | "terminal" | "check" | "download" | "upload" | "edit" | "tool";

function iconFor(tool: string): IconKind {
  if (tool === "task_complete") return "check";
  if (tool.startsWith("sftp_list") || tool.startsWith("sftp_walk") || tool === "sftp_make_dir") return "folder";
  if (tool === "sftp_remove") return "trash";
  if (tool === "sftp_rename") return "edit";
  if (tool === "sftp_download") return "download";
  if (tool === "sftp_upload") return "upload";
  if (tool === "shell_exec") return "terminal";
  if (tool.startsWith("sftp_")) return "file";
  return "tool";
}

function Icon({ kind }: { kind: IconKind }) {
  const common = {
    width: 14,
    height: 14,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 2,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
  };
  switch (kind) {
    case "folder":
      return (
        <svg {...common}>
          <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7z" />
        </svg>
      );
    case "file":
      return (
        <svg {...common}>
          <path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8l-5-5z" />
          <path d="M14 3v5h5" />
        </svg>
      );
    case "trash":
      return (
        <svg {...common}>
          <path d="M3 6h18" />
          <path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
          <path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6" />
        </svg>
      );
    case "terminal":
      return (
        <svg {...common}>
          <path d="M4 17l5-5-5-5" />
          <path d="M12 19h8" />
        </svg>
      );
    case "check":
      return (
        <svg {...common}>
          <path d="M20 6L9 17l-5-5" />
        </svg>
      );
    case "download":
      return (
        <svg {...common}>
          <path d="M12 3v12" />
          <path d="M7 10l5 5 5-5" />
          <path d="M5 21h14" />
        </svg>
      );
    case "upload":
      return (
        <svg {...common}>
          <path d="M12 21V9" />
          <path d="M7 14l5-5 5 5" />
          <path d="M5 3h14" />
        </svg>
      );
    case "edit":
      return (
        <svg {...common}>
          <path d="M12 20h9" />
          <path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4 12.5-12.5z" />
        </svg>
      );
    case "tool":
    default:
      return (
        <svg {...common}>
          <circle cx="12" cy="12" r="3" />
          <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1-1.5 1.7 1.7 0 0 0-1.9.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.9 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1 1.7 1.7 0 0 0-.3-1.9l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.9.3h0a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5h0a1.7 1.7 0 0 0 1.9-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.9v0a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" />
        </svg>
      );
  }
}

function summaryArg(tool: string, args: any): string | null {
  if (!args || typeof args !== "object") return null;
  const pick = (k: string) => (typeof args[k] === "string" ? args[k] : null);
  if (tool === "shell_exec") return pick("command");
  if (tool === "sftp_rename") {
    const from = pick("from") || pick("from_path");
    const to = pick("to") || pick("to_path");
    return from && to ? `${from} → ${to}` : from || to;
  }
  return (
    pick("path") ||
    pick("remote_path") ||
    pick("dir") ||
    pick("directory") ||
    pick("command") ||
    pick("message") ||
    null
  );
}

function truncate(s: string, n = 80): string {
  if (s.length <= n) return s;
  return s.slice(0, n - 1) + "…";
}

const STATUS_LABEL: Record<Props["status"], string> = {
  running: "running",
  awaiting_approval: "awaiting approval",
  done: "done",
  error: "error",
  rejected: "rejected",
};

export default function ToolCallCard({
  toolName,
  args,
  status,
  output,
  error,
  collapsed = true,
}: Props) {
  const [open, setOpen] = useState(!collapsed);
  const argSummary = useMemo(() => summaryArg(toolName, args), [toolName, args]);
  const argsJson = useMemo(() => {
    try {
      return JSON.stringify(args, null, 2);
    } catch {
      return String(args);
    }
  }, [args]);

  const headerId = `tool-${toolName}-${Math.random().toString(36).slice(2, 8)}`;

  return (
    <div className={`tool-card tool-card--${status}`}>
      <button
        type="button"
        className="tool-card__header"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        aria-controls={headerId}
      >
        <span className="tool-card__icon" aria-hidden>
          <Icon kind={iconFor(toolName)} />
        </span>
        <span className="tool-card__name">{toolName}</span>
        {argSummary ? (
          <span className="tool-card__arg-summary" title={argSummary}>
            {truncate(argSummary, 80)}
          </span>
        ) : null}
        <span className={`tool-card__status tool-card__status--${status}`}>
          {STATUS_LABEL[status]}
        </span>
        <span className={`tool-card__chevron ${open ? "is-open" : ""}`} aria-hidden>
          ▸
        </span>
      </button>
      {open ? (
        <div id={headerId} className="tool-card__body">
          <div className="tool-card__section">
            <div className="tool-card__section-label">arguments</div>
            <pre className="tool-card__code">{argsJson}</pre>
          </div>
          {error ? (
            <div className="tool-card__section">
              <div className="tool-card__section-label tool-card__section-label--error">error</div>
              <pre className="tool-card__code tool-card__code--error">{error}</pre>
            </div>
          ) : null}
          {output ? (
            <div className="tool-card__section">
              <div className="tool-card__section-label">output</div>
              <pre className="tool-card__code tool-card__code--output">{output}</pre>
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
