import { listen, UnlistenFn } from "@tauri-apps/api/event";
import type { SessionLifecycleState, TransferStatus } from "./api";

export interface SessionStateChangedEvent {
  sessionId: string;
  state: SessionLifecycleState;
  reason?: string | null;
}

export interface SessionHeartbeatEvent {
  sessionId: string;
  ok: boolean;
}

export interface TransferProgressEvent {
  id: string;
  bytes: number;
  total: number | null;
  status: TransferStatus;
}

export function onSessionStateChanged(
  handler: (e: SessionStateChangedEvent) => void,
): Promise<UnlistenFn> {
  return listen<SessionStateChangedEvent>("session-state-changed", (evt) =>
    handler(evt.payload),
  );
}

export function onSessionHeartbeat(
  handler: (e: SessionHeartbeatEvent) => void,
): Promise<UnlistenFn> {
  return listen<SessionHeartbeatEvent>("session-heartbeat", (evt) =>
    handler(evt.payload),
  );
}

export function onTransferProgress(
  handler: (e: TransferProgressEvent) => void,
): Promise<UnlistenFn> {
  return listen<TransferProgressEvent>("transfer-progress", (evt) =>
    handler(evt.payload),
  );
}
