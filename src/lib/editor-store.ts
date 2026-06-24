import { create } from "zustand";

export interface EditorState {
  path: string;
  dirty: boolean;
}

interface EditorStore {
  // Map from tabId -> editor state. null means no editor open for that tab.
  editors: Record<string, EditorState>;
  openEditor: (tabId: string, path: string) => void;
  closeEditor: (tabId: string) => void;
  setDirty: (tabId: string, dirty: boolean) => void;
  getEditor: (tabId: string) => EditorState | undefined;
}

export const useEditorStore = create<EditorStore>((set, get) => ({
  editors: {},
  openEditor: (tabId, path) =>
    set((s) => ({ editors: { ...s.editors, [tabId]: { path, dirty: false } } })),
  closeEditor: (tabId) =>
    set((s) => {
      const next = { ...s.editors };
      delete next[tabId];
      return { editors: next };
    }),
  setDirty: (tabId, dirty) =>
    set((s) => {
      const cur = s.editors[tabId];
      if (!cur) return s;
      if (cur.dirty === dirty) return s;
      return { editors: { ...s.editors, [tabId]: { ...cur, dirty } } };
    }),
  getEditor: (tabId) => get().editors[tabId],
}));

export function languageForPath(path: string): string {
  const lower = path.toLowerCase();
  const dot = lower.lastIndexOf(".");
  const ext = dot >= 0 ? lower.slice(dot + 1) : "";
  switch (ext) {
    case "json":
      return "json";
    case "yaml":
    case "yml":
      return "yaml";
    case "toml":
      return "toml";
    case "properties":
    case "conf":
    case "ini":
    case "cfg":
      return "ini";
    case "sh":
    case "bash":
    case "zsh":
      return "shell";
    case "js":
    case "mjs":
    case "cjs":
      return "javascript";
    case "jsx":
      return "javascript";
    case "ts":
      return "typescript";
    case "tsx":
      return "typescript";
    case "py":
      return "python";
    case "md":
    case "markdown":
      return "markdown";
    case "html":
    case "htm":
      return "html";
    case "css":
      return "css";
    case "xml":
      return "xml";
    case "rs":
      return "rust";
    case "go":
      return "go";
    case "java":
      return "java";
    case "c":
    case "h":
      return "c";
    case "cpp":
    case "cc":
    case "hpp":
      return "cpp";
    default:
      return "plaintext";
  }
}
