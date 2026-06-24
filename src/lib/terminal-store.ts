import { create } from "zustand";

export interface TerminalState {
  /** When the shell handshake completes, the backend-assigned shell id. */
  shellId: string | null;
  /** Optional exit code once the shell has closed. */
  exitCode: number | null;
}

interface TerminalStore {
  /** Map from tabId -> terminal state. Presence means a terminal is open for that tab. */
  terminals: Record<string, TerminalState>;
  openTerminal: (tabId: string) => void;
  closeTerminal: (tabId: string) => void;
  setShellId: (tabId: string, shellId: string | null) => void;
  setExitCode: (tabId: string, code: number | null) => void;
  getTerminal: (tabId: string) => TerminalState | undefined;
}

export const useTerminalStore = create<TerminalStore>((set, get) => ({
  terminals: {},
  openTerminal: (tabId) =>
    set((s) => {
      if (s.terminals[tabId]) return s;
      return {
        terminals: {
          ...s.terminals,
          [tabId]: { shellId: null, exitCode: null },
        },
      };
    }),
  closeTerminal: (tabId) =>
    set((s) => {
      if (!s.terminals[tabId]) return s;
      const next = { ...s.terminals };
      delete next[tabId];
      return { terminals: next };
    }),
  setShellId: (tabId, shellId) =>
    set((s) => {
      const cur = s.terminals[tabId];
      if (!cur) return s;
      return { terminals: { ...s.terminals, [tabId]: { ...cur, shellId } } };
    }),
  setExitCode: (tabId, code) =>
    set((s) => {
      const cur = s.terminals[tabId];
      if (!cur) return s;
      return { terminals: { ...s.terminals, [tabId]: { ...cur, exitCode: code } } };
    }),
  getTerminal: (tabId) => get().terminals[tabId],
}));
