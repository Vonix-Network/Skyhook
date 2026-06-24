import { useEffect, useRef } from "react";
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
import { Shortcuts } from "./components/Shortcuts";
import { Properties } from "./components/Properties";
import { Resizer } from "./components/Resizer";
import { AgentPanel } from "./components/Agent/AgentPanel";
import { AgentSettings } from "./components/Agent/AgentSettings";
import { useStore } from "./lib/store";
import "./styles/app.css";

export function App() {
  const loadConnections = useStore((s) => s.loadConnections);
  const loadSettings = useStore((s) => s.loadSettings);
  const loadKnownHosts = useStore((s) => s.loadKnownHosts);
  const subscribeBackendEvents = useStore((s) => s.subscribeBackendEvents);
  const openConnection = useStore((s) => s.openConnection);
  const openShortcuts = useStore((s) => s.openShortcuts);

  const tabs = useStore((s) => s.tabs);
  const activeTabId = useStore((s) => s.activeTabId);
  const activeTab = tabs.find((t) => t.id === activeTabId) ?? null;

  const showForm = useStore((s) => s.showConnectionForm);
  const showTransfers = useStore((s) => s.showTransfersPanel);
  const showSettings = useStore((s) => s.showSettings);
  const showAbout = useStore((s) => s.showAbout);
  const showKnownHosts = useStore((s) => s.showKnownHosts);
  const showShortcuts = useStore((s) => s.showShortcuts);
  const showAgent = useStore((s) => s.showAgent);
  const showAgentSettings = useStore((s) => s.showAgentSettings);
  const openAgentSettings = useStore((s) => s.openAgentSettings);
  const closeAgentSettings = useStore((s) => s.closeAgentSettings);

  const sidebarWidth = useStore((s) => s.sidebarWidth);
  const sidebarCollapsed = useStore((s) => s.sidebarCollapsed);
  const setSidebarWidth = useStore((s) => s.setSidebarWidth);
  const transferPanelHeight = useStore((s) => s.transferPanelHeight);
  const setTransferPanelHeight = useStore((s) => s.setTransferPanelHeight);

  const bootstrappedRef = useRef(false);

  useEffect(() => {
    loadConnections().catch((e) => console.error("load conns", e));
    loadKnownHosts().catch((e) => console.error("load known hosts", e));

    let unsub: (() => void) | undefined;
    subscribeBackendEvents()
      .then((u) => {
        unsub = u;
      })
      .catch((e) => console.error("subscribe", e));

    // Bootstrap: load settings, then auto-open last active connection if present.
    (async () => {
      try {
        await loadSettings();
        const st = useStore.getState();
        const lastId =
          (st.settings as any)?.last_active_connection_id as string | null;
        if (
          !bootstrappedRef.current &&
          lastId &&
          st.connections.some((c) => c.id === lastId) &&
          !st.tabs.some((t) => t.connectionId === lastId)
        ) {
          bootstrappedRef.current = true;
          await openConnection(lastId);
        }
      } catch (e) {
        console.error("bootstrap", e);
      }
    })();

    return () => {
      if (unsub) unsub();
    };
  }, [
    loadConnections,
    loadSettings,
    loadKnownHosts,
    subscribeBackendEvents,
    openConnection,
  ]);

  // Re-run the auto-open once connections finish loading (in case load_settings
  // resolved first and connections weren't yet known).
  const connections = useStore((s) => s.connections);
  useEffect(() => {
    if (bootstrappedRef.current) return;
    const st = useStore.getState();
    const lastId =
      (st.settings as any)?.last_active_connection_id as string | null;
    if (
      lastId &&
      connections.some((c) => c.id === lastId) &&
      !st.tabs.some((t) => t.connectionId === lastId)
    ) {
      bootstrappedRef.current = true;
      openConnection(lastId).catch((e) => console.error("auto-open", e));
    }
  }, [connections, openConnection]);

  // Listen for skyhook:open-agent-settings dispatched from AgentPanel.
  useEffect(() => {
    const onOpen = () => openAgentSettings();
    window.addEventListener("skyhook:open-agent-settings", onOpen as EventListener);
    return () => window.removeEventListener("skyhook:open-agent-settings", onOpen as EventListener);
  }, [openAgentSettings]);

  // Global '?' → Shortcuts modal.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "?") return;
      const tgt = e.target as HTMLElement | null;
      const inEditable =
        tgt &&
        (tgt.tagName === "INPUT" ||
          tgt.tagName === "TEXTAREA" ||
          tgt.tagName === "SELECT" ||
          (tgt as HTMLElement).isContentEditable);
      if (inEditable) return;
      const st = useStore.getState();
      if (
        st.showShortcuts ||
        st.showSettings ||
        st.showAbout ||
        st.showKnownHosts ||
        st.showConnectionForm
      ) {
        return;
      }
      e.preventDefault();
      openShortcuts();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [openShortcuts]);

  const sidebarStyle = sidebarCollapsed
    ? undefined
    : { width: sidebarWidth, flex: `0 0 ${sidebarWidth}px` };

  return (
    <div className="app">
      <div className="sidebar-wrap" style={sidebarStyle}>
        <Sidebar />
        {!sidebarCollapsed && (
          <Resizer
            direction="horizontal"
            onResize={(dx) => setSidebarWidth(sidebarWidth + dx)}
          />
        )}
      </div>
      <main className="main">
        <TabBar />
        {activeTab ? <Browser tab={activeTab} /> : <Welcome />}
        {showTransfers && (
          <div
            className="transfer-panel-wrap"
            style={{ height: transferPanelHeight }}
          >
            <Resizer
              direction="vertical"
              onResize={(dy) =>
                setTransferPanelHeight(transferPanelHeight - dy)
              }
            />
            <TransferPanel />
          </div>
        )}
      </main>
      {showAgent && <AgentPanel />}
      {showForm && <ConnectionForm />}
      {showSettings && <Settings />}
      {showAbout && <About />}
      {showKnownHosts && <KnownHostsManager />}
      {showShortcuts && <Shortcuts />}
      {showAgentSettings && <AgentSettings onClose={closeAgentSettings} />}
      <Properties />
      <ToastContainer />
    </div>
  );
}
