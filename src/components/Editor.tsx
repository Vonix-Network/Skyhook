import { useCallback, useEffect, useRef, useState } from "react";
import MonacoEditor, { OnMount } from "@monaco-editor/react";
import { invoke } from "@tauri-apps/api/core";
import { Save, X, AlertCircle } from "lucide-react";
import { useEditorStore, languageForPath } from "../lib/editor-store";

interface EditorProps {
  tabId: string;
  path: string;
}

const MAX_BYTES = 10 * 1024 * 1024; // 10 MB

function toast(kind: "success" | "error" | "info", message: string) {
  window.dispatchEvent(
    new CustomEvent("skyhook:toast", { detail: { kind, message } }),
  );
}

export function Editor({ tabId, path }: EditorProps) {
  const closeEditor = useEditorStore((s) => s.closeEditor);
  const setDirty = useEditorStore((s) => s.setDirty);

  const [content, setContent] = useState<string>("");
  const [original, setOriginal] = useState<string>("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [cursor, setCursor] = useState<{ line: number; col: number }>({
    line: 1,
    col: 1,
  });

  const editorRef = useRef<Parameters<OnMount>[0] | null>(null);
  const contentRef = useRef<string>("");
  contentRef.current = content;

  const language = languageForPath(path);
  const dirty = content !== original;

  // Load file
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    invoke<string>("read_file", { sessionId: tabId, path })
      .then((text) => {
        if (cancelled) return;
        if (text.length > MAX_BYTES) {
          setError(`File too large (${text.length} bytes; max 10 MB)`);
          setContent("");
          setOriginal("");
        } else {
          setContent(text);
          setOriginal(text);
        }
        setLoading(false);
      })
      .catch((e) => {
        if (cancelled) return;
        setError(String(e));
        setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [tabId, path]);

  // Push dirty state to store
  useEffect(() => {
    setDirty(tabId, dirty);
  }, [tabId, dirty, setDirty]);

  const handleClose = useCallback(() => {
    if (dirty) {
      const ok = window.confirm("Discard unsaved changes?");
      if (!ok) return;
    }
    closeEditor(tabId);
  }, [dirty, closeEditor, tabId]);

  const handleSave = useCallback(async () => {
    if (saving) return;
    const text = contentRef.current;
    if (text.length > MAX_BYTES) {
      toast("error", "File too large to save");
      return;
    }
    setSaving(true);
    try {
      await invoke<void>("write_file", {
        sessionId: tabId,
        path,
        content: text,
      });
      setOriginal(text);
      toast("success", "Saved");
    } catch (e) {
      toast("error", `Save failed: ${e}`);
    } finally {
      setSaving(false);
    }
  }, [tabId, path, saving]);

  // Keyboard handlers: Ctrl/Cmd+S, Esc
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        void handleSave();
      } else if (e.key === "Escape") {
        // Only handle Esc if not inside an input/textarea outside monaco
        e.preventDefault();
        handleClose();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [handleSave, handleClose]);

  const onMount: OnMount = (editor, monaco) => {
    editorRef.current = editor;
    editor.onDidChangeCursorPosition((e) => {
      setCursor({ line: e.position.lineNumber, col: e.position.column });
    });
    // Bind Ctrl/Cmd+S inside monaco too
    editor.addCommand(
      monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS,
      () => void handleSave(),
    );
  };

  return (
    <div className="editor-view">
      <div className="editor-header">
        <div className="editor-path">
          {dirty && <span className="editor-dirty-dot" title="Unsaved changes" />}
          <span className="editor-path-text">{path}</span>
        </div>
        <div className="editor-actions">
          <button
            className="editor-btn"
            onClick={() => void handleSave()}
            disabled={saving || loading || !!error || !dirty}
            title="Save (Ctrl/Cmd+S)"
          >
            <Save size={14} />
            <span>Save</span>
          </button>
          <button
            className="editor-btn"
            onClick={handleClose}
            title="Close (Esc)"
          >
            <X size={14} />
            <span>Close</span>
          </button>
        </div>
      </div>

      <div className="editor-body">
        {loading ? (
          <div className="editor-loading">Loading…</div>
        ) : error ? (
          <div className="editor-error">
            <AlertCircle size={16} />
            <span>{error}</span>
          </div>
        ) : (
          <MonacoEditor
            height="100%"
            theme="vs-dark"
            language={language}
            value={content}
            onChange={(v) => setContent(v ?? "")}
            onMount={onMount}
            options={{
              automaticLayout: true,
              fontSize: 13,
              minimap: { enabled: false },
              scrollBeyondLastLine: false,
              tabSize: 2,
            }}
          />
        )}
      </div>

      <div className="editor-status">
        <span>
          Ln {cursor.line}, Col {cursor.col}
        </span>
        <span>UTF-8</span>
        <span>{language}</span>
        {dirty && <span className="editor-status-dirty">● modified</span>}
      </div>
    </div>
  );
}
