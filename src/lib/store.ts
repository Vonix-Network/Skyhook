import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
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

// debounced save_settings caller (single-flight, 400ms tail)
let _settingsSaveTimer: ReturnType<typeof setTimeout> | null = null;
function scheduleSettingsSave(get: () => AppStore) {
  if (_settingsSaveTimer) clearTimeout(_settingsSaveTimer);
  _settingsSaveTimer = setTimeout(() => {
    const s = get().settings;
    if (s) {
      api.saveSettings(s).catch((e) => console.error("save_settings", e));
    }
  }, 400);
}

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
  throughput_bps: number;
  eta_seconds: number | null;
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
  showAgentSettings: boolean;

  // ---------- v0.3.0 polish ----------
  showShortcuts: boolean;
  sidebarWidth: number;
  transferPanelHeight: number;

  // ---------- v0.6.0 agent ----------
  showAgent: boolean;
  toggleAgent: () => void;

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
  enqueueTransfer: (t: Omit<Transfer, "id" | "startedAt" | "status" | "bytes" | "error" | "throughput_bps" | "eta_seconds">) => void;
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
  openAgentSettings: () => void;
  closeAgentSettings: () => void;
  subscribeBackendEvents: () => Promise<() => void>;

  // ---------- v0.3.0 polish actions ----------
  openShortcuts: () => void;
  closeShortcuts: () => void;
  setSidebarWidth: (w: number) => void;
  setTransferPanelHeight: (h: number) => void;
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
  showAgentSettings: false,

  showShortcuts: false,
  sidebarWidth: 260,
  transferPanelHeight: 240,

  showAgent: false,
  toggleAgent: () => set({ showAgent: !get().showAgent }),

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
    const tab = get().tabs.find((t) => t.id === id);
    if (tab) {
      invoke("set_last_active_connection", { connectionId: tab.connectionId })
        .catch(() => {/* command may not exist yet; ignore */});
    }
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
          throughput_bps: 0,
          eta_seconds: null,
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
      const patch: Partial<AppStore> = { settings };
      const sw = (settings as any).sidebar_width;
      const tph = (settings as any).transfer_panel_height;
      if (typeof sw === "number" && sw > 0) {
        patch.sidebarWidth = Math.max(200, Math.min(500, Math.round(sw)));
      }
      if (typeof tph === "number" && tph > 0) {
        patch.transferPanelHeight = Math.max(140, Math.min(600, Math.round(tph)));
      }
      set(patch as any);
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
  openAgentSettings() {
    set({ showAgentSettings: true });
  },
  closeAgentSettings() {
    set({ showAgentSettings: false });
  },

  openShortcuts() {
    set({ showShortcuts: true });
  },
  closeShortcuts() {
    set({ showShortcuts: false });
  },
  setSidebarWidth(w) {
    const next = Math.max(200, Math.min(500, Math.round(w)));
    if (get().sidebarWidth === next) return;
    set({ sidebarWidth: next });
    const cur = get().settings;
    if (cur) {
      set({ settings: { ...cur, sidebar_width: next } as Settings });
      scheduleSettingsSave(get);
    }
  },
  setTransferPanelHeight(h) {
    const next = Math.max(140, Math.min(600, Math.round(h)));
    if (get().transferPanelHeight === next) return;
    set({ transferPanelHeight: next });
    const cur = get().settings;
    if (cur) {
      set({ settings: { ...cur, transfer_panel_height: next } as Settings });
      scheduleSettingsSave(get);
    }
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
    const unProg = await onTransferProgress((e: any) => {
      set({
        transfers: get().transfers.map((t) =>
          t.id === e.id
            ? {
                ...t,
                bytes: e.bytes,
                total: e.total,
                status: e.status as TransferStatus,
                throughput_bps:
                  typeof e.throughput_bps === "number" ? e.throughput_bps : 0,
                eta_seconds:
                  typeof e.eta_seconds === "number" ? e.eta_seconds : null,
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
