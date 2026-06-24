import { useEffect } from "react";
import { Sidebar } from "./components/Sidebar";
import { TabBar } from "./components/TabBar";
import { Browser } from "./components/Browser";
import { Welcome } from "./components/Welcome";
import { ConnectionForm } from "./components/ConnectionForm";
import { TransferPanel } from "./components/TransferPanel";
import { ToastContainer } from "./components/Toast";
import { Settings } from "./components/Settings";
import { About } from "./components/About";
import { KnownHostsManager } from "./components/KnownHostsManager";
import { useStore } from "./lib/store";
import "./styles/app.css";

export function App() {
  const loadConnections = useStore((s) => s.loadConnections);
  const loadSettings = useStore((s) => s.loadSettings);
  const loadKnownHosts = useStore((s) => s.loadKnownHosts);
  const subscribeBackendEvents = useStore((s) => s.subscribeBackendEvents);

  const tabs = useStore((s) => s.tabs);
  const activeTabId = useStore((s) => s.activeTabId);
  const activeTab = tabs.find((t) => t.id === activeTabId) ?? null;

  const showForm = useStore((s) => s.showConnectionForm);
  const showTransfers = useStore((s) => s.showTransfersPanel);
  const showSettings = useStore((s) => s.showSettings);
  const showAbout = useStore((s) => s.showAbout);
  const showKnownHosts = useStore((s) => s.showKnownHosts);

  useEffect(() => {
    loadConnections().catch((e) => console.error("load conns", e));
    loadSettings().catch((e) => console.error("load settings", e));
    loadKnownHosts().catch((e) => console.error("load known hosts", e));
    let unsub: (() => void) | undefined;
    subscribeBackendEvents()
      .then((u) => {
        unsub = u;
      })
      .catch((e) => console.error("subscribe", e));
    return () => {
      if (unsub) unsub();
    };
  }, [loadConnections, loadSettings, loadKnownHosts, subscribeBackendEvents]);

  return (
    <div className="app">
      <Sidebar />
      <main className="main">
        <TabBar />
        {activeTab ? <Browser tab={activeTab} /> : <Welcome />}
        {showTransfers && <TransferPanel />}
      </main>
      {showForm && <ConnectionForm />}
      {showSettings && <Settings />}
      {showAbout && <About />}
      {showKnownHosts && <KnownHostsManager />}
      <ToastContainer />
    </div>
  );
}
