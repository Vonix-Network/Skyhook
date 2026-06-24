import { useStore } from "../lib/store";
import { X } from "lucide-react";

export function TabBar() {
  const tabs = useStore((s) => s.tabs);
  const activeTabId = useStore((s) => s.activeTabId);
  const setActive = useStore((s) => s.setActiveTab);
  const close = useStore((s) => s.closeTab);

  if (tabs.length === 0) return null;

  return (
    <div className="tabbar">
      {tabs.map((t) => (
        <div
          key={t.id}
          className={`tab ${t.id === activeTabId ? "active" : ""}`}
          onClick={() => setActive(t.id)}
          onMouseDown={(e) => {
            // Middle-click closes the tab
            if (e.button === 1) {
              e.preventDefault();
              close(t.id);
            }
          }}
          title={t.name}
        >
          <span className={`conn-dot ${t.connected ? "online" : "offline"}`} />
          <span className="tab-title">{t.name}</span>
          <span
            className="tab-close"
            title="Close tab"
            onClick={(e) => {
              e.stopPropagation();
              close(t.id);
            }}
          >
            <X size={12} />
          </span>
        </div>
      ))}
    </div>
  );
}
