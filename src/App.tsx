import { useEffect } from "react";
import { Sidebar } from "./components/Sidebar";
import { TabBar } from "./components/TabBar";
import { Browser } from "./components/Browser";
import { Welcome } from "./components/Welcome";
import { ConnectionForm } from "./components/ConnectionForm";
import { TransferPanel } from "./components/TransferPanel";
import { useStore } from "./lib/store";
import "./styles/app.css";

export function App() {
  const loadConnections = useStore((s) => s.loadConnections);
  const tabs = useStore((s) => s.tabs);
  const activeTabId = useStore((s) => s.activeTabId);
  const activeTab = tabs.find((t) => t.id === activeTabId) ?? null;
  const showForm = useStore((s) => s.showConnectionForm);
  const showTransfers = useStore((s) => s.showTransfersPanel);

  useEffect(() => {
    loadConnections().catch((e) => console.error("load conns", e));
  }, [loadConnections]);

  return (
    <div className="app">
      <Sidebar />
      <main className="main">
        <TabBar />
        {activeTab ? <Browser tab={activeTab} /> : <Welcome />}
        {showTransfers && <TransferPanel />}
      </main>
      {showForm && <ConnectionForm />}
    </div>
  );
}
