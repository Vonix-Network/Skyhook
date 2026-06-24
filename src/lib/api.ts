import { invoke } from "@tauri-apps/api/core";

export type AuthMethod =
  | { password: { password: string } }
  | { key: { private_key: string; passphrase: string | null } }
  | { agent: Record<string, never> }
  // Tauri serializes our enum as either { password: {...} } or string "agent" depending on serde:
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

export interface SessionStatus {
  id: string;
  connection_id: string;
  connected: boolean;
  cwd: string;
}

export const api = {
  listConnections: () => invoke<Connection[]>("list_connections"),
  saveConnection: (connection: Connection) =>
    invoke<Connection>("save_connection", { connection }),
  deleteConnection: (id: string) => invoke<void>("delete_connection", { id }),
  connect: (connectionId: string) =>
    invoke<SessionStatus>("connect", { connectionId }),
  disconnect: (sessionId: string) =>
    invoke<void>("disconnect", { sessionId }),
  sessionStatus: () => invoke<SessionStatus[]>("session_status"),
  listDir: (sessionId: string, path: string) =>
    invoke<DirEntry[]>("list_dir", { sessionId, path }),
  readFile: (sessionId: string, path: string) =>
    invoke<string>("read_file", { sessionId, path }),
  writeFile: (sessionId: string, path: string, content: string) =>
    invoke<void>("write_file", { sessionId, path, content }),
  downloadFile: (sessionId: string, remote: string, local: string) =>
    invoke<number>("download_file", { sessionId, remote, local }),
  uploadFile: (sessionId: string, local: string, remote: string) =>
    invoke<number>("upload_file", { sessionId, local, remote }),
  makeDir: (sessionId: string, path: string) =>
    invoke<void>("make_dir", { sessionId, path }),
  remove: (sessionId: string, path: string) =>
    invoke<void>("remove_path", { sessionId, path }),
  rename: (sessionId: string, from: string, to: string) =>
    invoke<void>("rename", { sessionId, from, to }),
};
