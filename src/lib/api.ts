import { invoke } from "@tauri-apps/api/core";

export type AuthMethod =
  | { password: { password: string } }
  | { key: { private_key: string; passphrase: string | null } }
  | { agent: Record<string, never> }
  | "agent";

export interface Connection {
  id: string;
  name: string;
  host: string;
  port: number;
  username: string;
  auth: AuthMethod;
  default_path: string | null;
  color: string | null;
  created_at: number;
}

export interface DirEntry {
  name: string;
  path: string;
  is_dir: boolean;
  is_symlink: boolean;
  size: number;
  modified: number | null;
  mode: number | null;
}

export type SessionLifecycleState =
  | "connecting"
  | "connected"
  | "degraded"
  | "closed";

/** Backend SessionInfo snapshot (camelCase from serde). */
export interface SessionInfo {
  id: string;
  connectionId: string;
  state: SessionLifecycleState;
  cwd: string;
  reason?: string | null;
}

/**
 * Legacy shape consumed by parts of the app that pre-date the
 * `state` lifecycle. Derived from SessionInfo; `connected` is true
 * for any non-closed state so the existing tab.connected logic
 * keeps working.
 */
export interface SessionStatus extends SessionInfo {
  connection_id: string;
  connected: boolean;
}

function toStatus(info: SessionInfo): SessionStatus {
  return {
    ...info,
    connection_id: info.connectionId,
    connected: info.state !== "closed",
  };
}

export type TransferStatus =
  | "queued"
  | "active"
  | "paused"
  | "done"
  | "cancelled"
  | "error";

export type TransferDirection = "upload" | "download";

export interface TransferRequest {
  direction: TransferDirection;
  local: string;
  remote: string;
  recursive: boolean;
}

export interface BackendTransfer {
  id: string;
  session_id: string;
  direction: TransferDirection;
  local: string;
  remote: string;
  bytes: number;
  total: number | null;
  status: TransferStatus;
  error: string | null;
  started_at: number;
}

export interface KnownHostEntry {
  host: string;
  port: number;
  algo: string;
  fingerprint: string;
  added_at: string;
}

export interface WindowState {
  width?: number | null;
  height?: number | null;
  x?: number | null;
  y?: number | null;
  maximized: boolean;
}

export interface Settings {
  theme: "system" | "dark" | "light" | string;
  confirm_on_delete: boolean;
  editor_word_wrap: boolean;
  transfer_concurrency: number;
  last_active_connection_id: string | null;
  window: WindowState;
  show_hidden_files: boolean;
}

export const api = {
  // ---------- Connections ----------
  listConnections: () => invoke<Connection[]>("list_connections"),
  saveConnection: (connection: Connection) =>
    invoke<Connection>("save_connection", { connection }),
  deleteConnection: (id: string) => invoke<void>("delete_connection", { id }),

  // ---------- Sessions ----------
  connect: async (connectionId: string): Promise<SessionStatus> => {
    const info = await invoke<SessionInfo>("connect", { connectionId });
    return toStatus(info);
  },
  disconnect: (sessionId: string) => invoke<void>("disconnect", { sessionId }),
  reconnectSession: (sessionId: string) =>
    invoke<void>("reconnect", { sessionId }),
  sessionStatus: async (): Promise<SessionStatus[]> => {
    const infos = await invoke<SessionInfo[]>("session_status");
    return infos.map(toStatus);
  },

  // ---------- Filesystem ----------
  listDir: (sessionId: string, path: string) =>
    invoke<DirEntry[]>("list_dir", { sessionId, path }),
  stat: (sessionId: string, path: string) =>
    invoke<DirEntry>("stat", { sessionId, path }),
  walk: (sessionId: string, root: string) =>
    invoke<DirEntry[]>("walk", { sessionId, root }),
  readFile: (sessionId: string, path: string) =>
    invoke<string>("read_file", { sessionId, path }),
  writeFile: (sessionId: string, path: string, content: string) =>
    invoke<void>("write_file", { sessionId, path, content }),
  makeDir: (sessionId: string, path: string) =>
    invoke<void>("make_dir", { sessionId, path }),
  remove: (sessionId: string, path: string) =>
    invoke<void>("remove_path", { sessionId, path }),
  rename: (sessionId: string, from: string, to: string) =>
    invoke<void>("rename", { sessionId, from, to }),
  downloadFile: (sessionId: string, remote: string, local: string) =>
    invoke<number>("download_file", { sessionId, remote, local }),
  uploadFile: (sessionId: string, local: string, remote: string) =>
    invoke<number>("upload_file", { sessionId, local, remote }),

  // ---------- Transfers ----------
  transferEnqueue: (sessionId: string, jobs: TransferRequest[]) =>
    invoke<string[]>("transfer_enqueue", { sessionId, jobs }),
  transferPause: (id: string) => invoke<void>("transfer_pause", { id }),
  transferResume: (id: string) => invoke<void>("transfer_resume", { id }),
  transferCancel: (id: string) => invoke<void>("transfer_cancel", { id }),
  transferList: () => invoke<BackendTransfer[]>("transfer_list"),

  // ---------- Known hosts ----------
  knownHostsList: () => invoke<KnownHostEntry[]>("known_hosts_list"),
  knownHostsRemove: (host: string, port: number) =>
    invoke<void>("known_hosts_remove", { host, port }),
  knownHostsTrust: (
    host: string,
    port: number,
    algo: string,
    fingerprint: string,
  ) => invoke<void>("known_hosts_trust", { host, port, algo, fingerprint }),

  // ---------- Settings ----------
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) =>
    invoke<void>("save_settings", { settings }),
};
