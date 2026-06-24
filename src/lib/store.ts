import { create } from "zustand";
import {
  api,
  Connection,
  DirEntry,
  SessionStatus,
  Settings,
  KnownHostEntry,
} from "./api";
import {
  onSessionStateChanged,
  onSessionHeartbeat,
  onTransferProgress,
} from "./events";

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
  state: "connecting" | "connected" | "degraded" | "closed";
}

export type TransferDirection = "upload" | "download";
export type TransferStatus =
  | "queued"
  | "active"
  | "paused"
  | "done"
  | "cancelled"
  | "error";

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

export type ToastKind = "info" | "success" | "warning" | "error";
export interface Toast {
  id: string;
  message: string;
  kind: ToastKind;
}

interface AppStore {
  // ---------- existing ----------
  connections: Connection[];
  tabs: Tab[];
  activeTabId: string | null;
  transfers: Transfer[];
  sidebarCollapsed: boolean;
  showConnectionForm: boolean;
  editingConnection: Connection | null;
  showTransfersPanel: boolean;

  // ---------- new ----------
  settings: Settings | null;
  knownHosts: KnownHostEntry[];
  toasts: Toast[];
  showSettings: boolean;
  showAbout: boolean;
  showKnownHosts: boolean;

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

  // ---------- new actions ----------
  loadSettings: () => Promise<void>;
  updateSettings: (patch: Partial<Settings>) => Promise<void>;
  loadKnownHosts: () => Promise<void>;
  removeKnownHost: (host: string, port: number) => Promise<void>;
  toast: (message: string, kind?: ToastKind) => string;
  dismissToast: (id: string) => void;
  openSettings: () => void;
  closeSettings: () => void;
  openAbout: () => void;
  closeAbout: () => void;
  openKnownHosts: () => void;
  closeKnownHosts: () => void;
  subscribeBackendEvents: () => Promise<() => void>;
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

  settings: null,
  knownHosts: [],
  toasts: [],
  showSettings: false,
  showAbout: false,
  showKnownHosts: false,

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
    const existing = get().tabs.find((t) => t.connectionId === connectionId);
    if (existing) {
      set({ activeTabId: existing.id });
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
      state: status.state,
    };
    set({ tabs: [...get().tabs, tab], activeTabId: tab.id });
    await get().refresh(tab.id);
  },

  async reconnect(tabId) {
    const tab = get().tabs.find((t) => t.id === tabId);
    if (!tab) return;
    const connectionId = tab.connectionId;
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

  // ====================== NEW ACTIONS ======================

  async loadSettings() {
    try {
      const settings = await api.getSettings();
      set({ settings });
    } catch (e: any) {
      console.error("loadSettings", e);
    }
  },

  async updateSettings(patch) {
    const current = get().settings;
    if (!current) return;
    const next: Settings = { ...current, ...patch };
    set({ settings: next });
    try {
      await api.saveSettings(next);
    } catch (e: any) {
      // roll back on failure
      set({ settings: current });
      get().toast(`Failed to save settings: ${e?.message ?? e}`, "error");
    }
  },

  async loadKnownHosts() {
    try {
      const knownHosts = await api.knownHostsList();
      set({ knownHosts });
    } catch (e: any) {
      console.error("loadKnownHosts", e);
    }
  },

  async removeKnownHost(host, port) {
    try {
      await api.knownHostsRemove(host, port);
      set({
        knownHosts: get().knownHosts.filter(
          (k) => !(k.host === host && k.port === port),
        ),
      });
      get().toast(`Removed ${host}:${port}`, "success");
    } catch (e: any) {
      get().toast(`Failed to remove host: ${e?.message ?? e}`, "error");
    }
  },

  toast(message, kind = "info") {
    const id = crypto.randomUUID();
    set({ toasts: [...get().toasts, { id, message, kind }] });
    const lifetime = kind === "error" ? 8000 : 4000;
    setTimeout(() => {
      const exists = get().toasts.some((t) => t.id === id);
      if (exists) get().dismissToast(id);
    }, lifetime);
    return id;
  },

  dismissToast(id) {
    set({ toasts: get().toasts.filter((t) => t.id !== id) });
  },

  openSettings() {
    set({ showSettings: true });
  },
  closeSettings() {
    set({ showSettings: false });
  },
  openAbout() {
    set({ showAbout: true });
  },
  closeAbout() {
    set({ showAbout: false });
  },
  openKnownHosts() {
    set({ showKnownHosts: true });
  },
  closeKnownHosts() {
    set({ showKnownHosts: false });
  },

  async subscribeBackendEvents() {
    const unState = await onSessionStateChanged((e) => {
      const connected = e.state !== "closed";
      set({
        tabs: get().tabs.map((t) =>
          t.id === e.sessionId
            ? { ...t, state: e.state, connected }
            : t,
        ),
      });
      if (e.state === "degraded") {
        get().toast(
          `Session degraded${e.reason ? `: ${e.reason}` : ""}`,
          "warning",
        );
      } else if (e.state === "closed" && e.reason) {
        get().toast(`Session closed: ${e.reason}`, "error");
      }
    });
    const unBeat = await onSessionHeartbeat(() => {
      /* reserved for future UI; ignore for now */
    });
    const unProg = await onTransferProgress((e) => {
      set({
        transfers: get().transfers.map((t) =>
          t.id === e.id
            ? {
                ...t,
                bytes: e.bytes,
                total: e.total,
                status: e.status as TransferStatus,
              }
            : t,
        ),
      });
    });
    return () => {
      unState();
      unBeat();
      unProg();
    };
  },
}));
