import { create } from "zustand";
import { api, Connection, DirEntry, SessionStatus } from "./api";

export interface Tab {
  id: string;             // == sessionId
  connectionId: string;
  name: string;
  cwd: string;
  entries: DirEntry[];
  selected: Set<string>;  // selected entry paths
  loading: boolean;
  error: string | null;
  history: string[];
  historyIndex: number;
  connected: boolean;     // false = backend marked the session dead
}

export type TransferDirection = "upload" | "download";
export type TransferStatus = "queued" | "active" | "done" | "error";

export interface Transfer {
  id: string;
  sessionId: string;
  direction: TransferDirection;
  local: string;
  remote: string;
  bytes: number;
  total: number | null;
  status: TransferStatus;
  error: string | null;
  startedAt: number;
}

interface AppStore {
  connections: Connection[];
  tabs: Tab[];
  activeTabId: string | null;
  transfers: Transfer[];
  sidebarCollapsed: boolean;
  showConnectionForm: boolean;
  editingConnection: Connection | null;
  showTransfersPanel: boolean;

  loadConnections: () => Promise<void>;
  saveConnection: (c: Connection) => Promise<void>;
  deleteConnection: (id: string) => Promise<void>;
  openConnection: (id: string) => Promise<void>;
  reconnect: (tabId: string) => Promise<void>;
  closeTab: (id: string) => Promise<void>;
  setActiveTab: (id: string) => void;
  navigate: (tabId: string, path: string, pushHistory?: boolean) => Promise<void>;
  refresh: (tabId: string) => Promise<void>;
  goBack: (tabId: string) => Promise<void>;
  goForward: (tabId: string) => Promise<void>;
  goUp: (tabId: string) => Promise<void>;
  toggleSelect: (tabId: string, path: string, additive: boolean) => void;
  clearSelection: (tabId: string) => void;
  toggleSidebar: () => void;
  openConnectionForm: (c?: Connection | null) => void;
  closeConnectionForm: () => void;
  toggleTransfersPanel: () => void;
  enqueueTransfer: (t: Omit<Transfer, "id" | "startedAt" | "status" | "bytes" | "error">) => void;
  updateTransfer: (id: string, patch: Partial<Transfer>) => void;
}

export const useStore = create<AppStore>((set, get) => ({
  connections: [],
  tabs: [],
  activeTabId: null,
  transfers: [],
  sidebarCollapsed: false,
  showConnectionForm: false,
  editingConnection: null,
  showTransfersPanel: false,

  async loadConnections() {
    const connections = await api.listConnections();
    set({ connections });
  },

  async saveConnection(c) {
    const saved = await api.saveConnection(c);
    const conns = await api.listConnections();
    set({ connections: conns, showConnectionForm: false, editingConnection: null });
    return saved as unknown as void;
  },

  async deleteConnection(id) {
    await api.deleteConnection(id);
    set({ connections: get().connections.filter((c) => c.id !== id) });
  },

  async openConnection(connectionId) {
    // Dedup: if a tab already exists for this connection, just focus it.
    // Prevents accidental stacking of SFTP sessions (Wings/Pterodactyl
    // caps concurrent SFTP sessions per user and kills the channel when
    // the limit is exceeded — manifests as "session closed on reload").
    const existing = get().tabs.find((t) => t.connectionId === connectionId);
    if (existing) {
      set({ activeTabId: existing.id });
      // Refresh in case it was stale
      await get().refresh(existing.id);
      return;
    }
    const status: SessionStatus = await api.connect(connectionId);
    const conn = get().connections.find((c) => c.id === connectionId);
    const tab: Tab = {
      id: status.id,
      connectionId,
      name: conn?.name ?? "Session",
      cwd: status.cwd,
      entries: [],
      selected: new Set(),
      loading: true,
      error: null,
      history: [status.cwd],
      historyIndex: 0,
      connected: status.connected,
    };
    set({ tabs: [...get().tabs, tab], activeTabId: tab.id });
    await get().refresh(tab.id);
  },

  async reconnect(tabId) {
    const tab = get().tabs.find((t) => t.id === tabId);
    if (!tab) return;
    const connectionId = tab.connectionId;
    // Drop the dead session locally + close the dead one on the backend
    try {
      await api.disconnect(tab.id);
    } catch {
      /* ignore — it's already dead */
    }
    set({ tabs: get().tabs.filter((t) => t.id !== tabId) });
    await get().openConnection(connectionId);
  },

  async closeTab(id) {
    try {
      await api.disconnect(id);
    } catch {
      /* ignore */
    }
    const tabs = get().tabs.filter((t) => t.id !== id);
    const active =
      get().activeTabId === id ? tabs[tabs.length - 1]?.id ?? null : get().activeTabId;
    set({ tabs, activeTabId: active });
  },

  setActiveTab(id) {
    set({ activeTabId: id });
  },

  async navigate(tabId, path, pushHistory = true) {
    const tab = get().tabs.find((t) => t.id === tabId);
    if (!tab) return;
    set({
      tabs: get().tabs.map((t) =>
        t.id === tabId ? { ...t, loading: true, error: null } : t
      ),
    });
    try {
      const entries = await api.listDir(tabId, path);
      // canonicalize cwd from a non-empty result if possible
      const cwd =
        entries.length > 0
          ? entries[0].path.replace(/\/[^/]*$/, "") || "/"
          : path;
      const history = pushHistory
        ? [...tab.history.slice(0, tab.historyIndex + 1), cwd]
        : tab.history;
      const historyIndex = pushHistory ? history.length - 1 : tab.historyIndex;
      set({
        tabs: get().tabs.map((t) =>
          t.id === tabId
            ? {
                ...t,
                entries,
                cwd,
                loading: false,
                selected: new Set(),
                history,
                historyIndex,
              }
            : t
        ),
      });
    } catch (err: any) {
      const msg = err?.message ?? String(err);
      // If the failure mentions a dead channel, mark tab disconnected so
      // the UI shows the Reconnect banner instead of just a red error.
      const fatal = /closed|disconnect|broken|eof|not connected|lost/i.test(msg);
      set({
        tabs: get().tabs.map((t) =>
          t.id === tabId
            ? { ...t, loading: false, error: msg, connected: fatal ? false : t.connected }
            : t
        ),
      });
    }
  },

  async refresh(tabId) {
    const tab = get().tabs.find((t) => t.id === tabId);
    if (tab) await get().navigate(tabId, tab.cwd, false);
  },

  async goBack(tabId) {
    const tab = get().tabs.find((t) => t.id === tabId);
    if (!tab || tab.historyIndex <= 0) return;
    const newIdx = tab.historyIndex - 1;
    set({
      tabs: get().tabs.map((t) =>
        t.id === tabId ? { ...t, historyIndex: newIdx } : t
      ),
    });
    await get().navigate(tabId, tab.history[newIdx], false);
  },

  async goForward(tabId) {
    const tab = get().tabs.find((t) => t.id === tabId);
    if (!tab || tab.historyIndex >= tab.history.length - 1) return;
    const newIdx = tab.historyIndex + 1;
    set({
      tabs: get().tabs.map((t) =>
        t.id === tabId ? { ...t, historyIndex: newIdx } : t
      ),
    });
    await get().navigate(tabId, tab.history[newIdx], false);
  },

  async goUp(tabId) {
    const tab = get().tabs.find((t) => t.id === tabId);
    if (!tab) return;
    if (tab.cwd === "/" || tab.cwd === "") return;
    const parent = tab.cwd.replace(/\/[^/]+\/?$/, "") || "/";
    await get().navigate(tabId, parent);
  },

  toggleSelect(tabId, path, additive) {
    set({
      tabs: get().tabs.map((t) => {
        if (t.id !== tabId) return t;
        const sel = new Set(t.selected);
        if (additive) {
          sel.has(path) ? sel.delete(path) : sel.add(path);
        } else {
          sel.clear();
          sel.add(path);
        }
        return { ...t, selected: sel };
      }),
    });
  },

  clearSelection(tabId) {
    set({
      tabs: get().tabs.map((t) =>
        t.id === tabId ? { ...t, selected: new Set() } : t
      ),
    });
  },

  toggleSidebar() {
    set({ sidebarCollapsed: !get().sidebarCollapsed });
  },

  openConnectionForm(c) {
    set({ showConnectionForm: true, editingConnection: c ?? null });
  },

  closeConnectionForm() {
    set({ showConnectionForm: false, editingConnection: null });
  },

  toggleTransfersPanel() {
    set({ showTransfersPanel: !get().showTransfersPanel });
  },

  enqueueTransfer(t) {
    const id = crypto.randomUUID();
    set({
      transfers: [
        ...get().transfers,
        {
          ...t,
          id,
          status: "queued",
          bytes: 0,
          error: null,
          startedAt: Date.now(),
        },
      ],
    });
  },

  updateTransfer(id, patch) {
    set({
      transfers: get().transfers.map((t) => (t.id === id ? { ...t, ...patch } : t)),
    });
  },
}));
