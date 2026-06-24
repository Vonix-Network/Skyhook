import { useEffect } from "react";
import { X } from "lucide-react";
import { useStore } from "../lib/store";

interface Shortcut {
  keys: string;
  desc: string;
}
interface Group {
  title: string;
  items: Shortcut[];
}

const GROUPS: Group[] = [
  {
    title: "Global",
    items: [
      { keys: "?", desc: "Show this shortcuts overlay" },
      { keys: "Esc", desc: "Close dialogs / overlays" },
      { keys: "Ctrl+,", desc: "Open Settings" },
    ],
  },
  {
    title: "Browser",
    items: [
      { keys: "Ctrl+L", desc: "Focus path bar" },
      { keys: "F5", desc: "Refresh listing" },
      { keys: "Backspace", desc: "Go up one directory" },
      { keys: "Enter", desc: "Open selected entry" },
      { keys: "F2", desc: "Rename selected entry" },
      { keys: "Del", desc: "Delete selected entries" },
      { keys: "↑ / ↓", desc: "Move selection" },
      { keys: "Shift+Click", desc: "Range select" },
      { keys: "Ctrl+Click", desc: "Toggle one in selection" },
    ],
  },
  {
    title: "Editor",
    items: [
      { keys: "Ctrl+S", desc: "Save file" },
      { keys: "Ctrl+W", desc: "Close editor tab" },
      { keys: "Ctrl+F", desc: "Find in file" },
    ],
  },
  {
    title: "Terminal",
    items: [
      { keys: "Ctrl+C", desc: "Send SIGINT to remote process" },
      { keys: "Ctrl+D", desc: "EOF / logout" },
      { keys: "Ctrl+Shift+C", desc: "Copy selection" },
      { keys: "Ctrl+Shift+V", desc: "Paste" },
    ],
  },
  {
    title: "Transfers",
    items: [
      { keys: "Drag & drop", desc: "Upload local files to current dir" },
      { keys: "Right-click → Download", desc: "Save remote file locally" },
    ],
  },
];

export function Shortcuts() {
  const show = useStore((s) => s.showShortcuts);
  const close = useStore((s) => s.closeShortcuts);

  useEffect(() => {
    if (!show) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        close();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [show, close]);

  if (!show) return null;

  return (
    <div className="modal-backdrop" onClick={close}>
      <div
        className="modal modal-wide"
        style={{ width: 640 }}
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-label="Keyboard shortcuts"
      >
        <div className="modal-header">
          <div className="modal-title">Keyboard Shortcuts</div>
          <button className="btn btn-ghost btn-icon" onClick={close} title="Close">
            <X size={14} />
          </button>
        </div>
        <div className="modal-body shortcuts-grid">
          {GROUPS.map((g) => (
            <div key={g.title} className="shortcuts-section">
              <div className="shortcuts-section-title">{g.title}</div>
              <div className="shortcuts-list">
                {g.items.map((it) => (
                  <div key={it.keys + it.desc} className="shortcuts-row">
                    <kbd className="kbd">{it.keys}</kbd>
                    <span className="shortcuts-desc">{it.desc}</span>
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
