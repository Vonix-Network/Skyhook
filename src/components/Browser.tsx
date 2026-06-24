import { useState, useEffect, useRef, useMemo, useCallback } from "react";
import { Tab, useStore } from "../lib/store";
import {
  ChevronLeft,
  ChevronRight,
  ArrowUp,
  RefreshCw,
  FolderPlus,
  Upload,
  Download as DownloadIcon,
  Trash2,
  Folder,
  File as FileIcon,
  Link as LinkIcon,
  ChevronUp,
  ChevronDown,
  Edit3,
  FileEdit,
  Copy,
  Info,
} from "lucide-react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { api, DirEntry } from "../lib/api";
import { ContextMenu, ContextMenuItem } from "./ContextMenu";

function fmtSize(n: number) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(2)} GB`;
}

function fmtDate(ts: number | null) {
  if (!ts) return "—";
  const d = new Date(ts * 1000);
  const now = new Date();
  const sameYear = d.getFullYear() === now.getFullYear();
  return d.toLocaleString(undefined, {
    month: "short",
    day: "2-digit",
    year: sameYear ? undefined : "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function fmtMode(mode: number | null) {
  if (mode == null) return "—";
  const p = mode & 0o777;
  const out = [];
  for (let shift = 6; shift >= 0; shift -= 3) {
    const x = (p >> shift) & 7;
    out.push((x & 4 ? "r" : "-") + (x & 2 ? "w" : "-") + (x & 1 ? "x" : "-"));
  }
  return out.join("");
}

function joinPath(dir: string, name: string) {
  return dir.endsWith("/") ? dir + name : dir + "/" + name;
}

function toast(kind: "error" | "success" | "info", message: string) {
  window.dispatchEvent(
    new CustomEvent("skyhook:toast", { detail: { kind, message } })
  );
}

type SortKey = "name" | "size" | "modified" | "mode";
type SortDir = "asc" | "desc";

interface CtxState {
  x: number;
  y: number;
  entry: DirEntry | null; // null = empty area
}

export function Browser({ tab }: { tab: Tab }) {
  const navigate = useStore((s) => s.navigate);
  const refresh = useStore((s) => s.refresh);
  const back = useStore((s) => s.goBack);
  const forward = useStore((s) => s.goForward);
  const up = useStore((s) => s.goUp);
  const enqueueTransfer = useStore((s) => s.enqueueTransfer);
  const updateTransfer = useStore((s) => s.updateTransfer);

  // Settings (best-effort: read off the store if extended, else defaults).
  const settings = useStore((s) => (s as any).settings) as
    | { show_hidden_files?: boolean; confirm_on_delete?: boolean }
    | undefined;
  const showHidden = settings?.show_hidden_files ?? false;
  const confirmOnDelete = settings?.confirm_on_delete ?? true;

  const [pathInput, setPathInput] = useState(tab.cwd);
  const pathRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  useEffect(() => setPathInput(tab.cwd), [tab.cwd]);

  const [sortKey, setSortKey] = useState<SortKey>("name");
  const [sortDir, setSortDir] = useState<SortDir>("asc");
  const [renaming, setRenaming] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [ctx, setCtx] = useState<CtxState | null>(null);
  const [lastClickPath, setLastClickPath] = useState<string | null>(null);
  const [localSelected, setLocalSelected] = useState<Set<string>>(new Set());

  // Mirror store selection -> local (so shift-range works smoothly)
  useEffect(() => {
    setLocalSelected(new Set(tab.selected));
  }, [tab.selected, tab.id]);

  const visibleEntries = useMemo(() => {
    let list = tab.entries.filter((e) => showHidden || !e.name.startsWith("."));
    const dirCmp = (a: DirEntry, b: DirEntry) =>
      (a.is_dir ? 0 : 1) - (b.is_dir ? 0 : 1);
    const cmp = (a: DirEntry, b: DirEntry) => {
      const d = dirCmp(a, b);
      if (d !== 0) return d;
      let r = 0;
      switch (sortKey) {
        case "name":
          r = a.name.localeCompare(b.name, undefined, { numeric: true });
          break;
        case "size":
          r = a.size - b.size;
          break;
        case "modified":
          r = (a.modified ?? 0) - (b.modified ?? 0);
          break;
        case "mode":
          r = (a.mode ?? 0) - (b.mode ?? 0);
          break;
      }
      return sortDir === "asc" ? r : -r;
    };
    return [...list].sort(cmp);
  }, [tab.entries, sortKey, sortDir, showHidden]);

  const setSelection = useCallback(
    (paths: string[]) => {
      const next = new Set(paths);
      setLocalSelected(next);
      // Sync to store selected via toggleSelect-friendly path: rewrite directly
      useStore.setState((s) => ({
        tabs: s.tabs.map((t) =>
          t.id === tab.id ? { ...t, selected: next } : t
        ),
      }));
    },
    [tab.id]
  );

  const onPathSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    navigate(tab.id, pathInput);
  };

  const openEntry = useCallback(
    (entry: DirEntry) => {
      if (entry.is_dir) {
        navigate(tab.id, entry.path);
      } else {
        window.dispatchEvent(
          new CustomEvent("skyhook:open-editor", {
            detail: { tabId: tab.id, path: entry.path },
          })
        );
      }
    },
    [navigate, tab.id]
  );

  const doUpload = useCallback(
    async (targetDir?: string) => {
      const selected = await openDialog({ multiple: true, directory: false });
      if (!selected) return;
      const files = Array.isArray(selected) ? selected : [selected];
      const dir = targetDir ?? tab.cwd;
      for (const local of files) {
        const filename = local.split(/[\\/]/).pop()!;
        const remote = joinPath(dir, filename);
        enqueueTransfer({
          sessionId: tab.id,
          direction: "upload",
          local,
          remote,
          total: null,
        });
        const recent = useStore
          .getState()
          .transfers.find(
            (t) => t.local === local && t.remote === remote && t.status === "queued"
          );
        if (recent) {
          updateTransfer(recent.id, { status: "active" });
          invoke<number>("upload_file", { sessionId: tab.id, local, remote })
            .then((bytes) => {
              updateTransfer(recent.id, { status: "done", bytes, total: bytes });
              refresh(tab.id);
              toast("success", `Uploaded ${filename}`);
            })
            .catch((err: any) => {
              const msg = err?.message ?? String(err);
              updateTransfer(recent.id, { status: "error", error: msg });
              toast("error", `Upload failed: ${msg}`);
            });
        }
      }
    },
    [tab.id, tab.cwd, enqueueTransfer, updateTransfer, refresh]
  );

  const doDownload = useCallback(async () => {
    if (localSelected.size === 0) {
      toast("error", "Select a file to download.");
      return;
    }
    for (const remote of localSelected) {
      const entry = tab.entries.find((e) => e.path === remote);
      if (!entry || entry.is_dir) continue;
      const local = await saveDialog({ defaultPath: entry.name });
      if (!local) continue;
      enqueueTransfer({
        sessionId: tab.id,
        direction: "download",
        local,
        remote,
        total: entry.size,
      });
      const recent = useStore
        .getState()
        .transfers.find(
          (t) => t.local === local && t.remote === remote && t.status === "queued"
        );
      if (recent) {
        updateTransfer(recent.id, { status: "active" });
        api
          .downloadFile(tab.id, remote, local)
          .then((bytes) => {
            updateTransfer(recent.id, { status: "done", bytes, total: bytes });
            toast("success", `Downloaded ${entry.name}`);
          })
          .catch((err: any) => {
            const msg = err?.message ?? String(err);
            updateTransfer(recent.id, { status: "error", error: msg });
            toast("error", `Download failed: ${msg}`);
          });
      }
    }
  }, [localSelected, tab.entries, tab.id, enqueueTransfer, updateTransfer]);

  const doMkdir = useCallback(async () => {
    const name = prompt("New folder name:");
    if (!name) return;
    const full = joinPath(tab.cwd, name);
    try {
      await api.makeDir(tab.id, full);
      refresh(tab.id);
      toast("success", `Created ${name}`);
    } catch (e: any) {
      toast("error", e?.message ?? String(e));
    }
  }, [tab.id, tab.cwd, refresh]);

  const doDelete = useCallback(
    async (paths?: string[]) => {
      const targets = paths ?? Array.from(localSelected);
      if (targets.length === 0) return;
      if (confirmOnDelete) {
        if (
          !confirm(
            `Delete ${targets.length} item(s)? This cannot be undone.`
          )
        )
          return;
      }
      let ok = 0;
      for (const p of targets) {
        try {
          await invoke<void>("remove_path", { sessionId: tab.id, path: p });
          ok++;
        } catch (e: any) {
          toast("error", `${p}: ${e?.message ?? e}`);
        }
      }
      if (ok > 0) toast("success", `Deleted ${ok} item(s)`);
      refresh(tab.id);
    },
    [localSelected, confirmOnDelete, tab.id, refresh]
  );

  const startRename = useCallback((entry: DirEntry) => {
    setRenaming(entry.path);
    setRenameValue(entry.name);
  }, []);

  const commitRename = useCallback(
    async (entry: DirEntry) => {
      const newName = renameValue.trim();
      setRenaming(null);
      if (!newName || newName === entry.name) return;
      const to = joinPath(tab.cwd, newName);
      try {
        await invoke<void>("rename", {
          sessionId: tab.id,
          from: entry.path,
          to,
        });
        toast("success", `Renamed to ${newName}`);
        refresh(tab.id);
      } catch (e: any) {
        toast("error", `Rename failed: ${e?.message ?? e}`);
      }
    },
    [renameValue, tab.cwd, tab.id, refresh]
  );

  const copyPath = useCallback(async (p: string) => {
    try {
      await navigator.clipboard.writeText(p);
      toast("success", "Path copied");
    } catch {
      toast("error", "Clipboard unavailable");
    }
  }, []);

  const showProperties = useCallback((entry: DirEntry) => {
    const lines = [
      `Name: ${entry.name}`,
      `Path: ${entry.path}`,
      `Type: ${entry.is_dir ? "Directory" : entry.is_symlink ? "Symlink" : "File"}`,
      `Size: ${entry.is_dir ? "—" : fmtSize(entry.size)}`,
      `Modified: ${fmtDate(entry.modified)}`,
      `Perms: ${fmtMode(entry.mode)}`,
    ].join("\n");
    alert(lines);
  }, []);

  // Row click handlers — replace / additive / shift-range.
  const onRowClick = (ev: React.MouseEvent, entry: DirEntry) => {
    if (renaming) return;
    const path = entry.path;
    if (ev.shiftKey && lastClickPath) {
      const paths = visibleEntries.map((e) => e.path);
      const a = paths.indexOf(lastClickPath);
      const b = paths.indexOf(path);
      if (a >= 0 && b >= 0) {
        const [lo, hi] = a < b ? [a, b] : [b, a];
        setSelection(paths.slice(lo, hi + 1));
        return;
      }
    }
    if (ev.ctrlKey || ev.metaKey) {
      const next = new Set(localSelected);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      setSelection(Array.from(next));
    } else {
      setSelection([path]);
    }
    setLastClickPath(path);
  };

  // Keyboard handling.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // Skip if user is typing in inputs.
      const tgt = e.target as HTMLElement | null;
      const inEditable =
        tgt &&
        (tgt.tagName === "INPUT" ||
          tgt.tagName === "TEXTAREA" ||
          (tgt as HTMLElement).isContentEditable);
      if (e.key === "F5") {
        e.preventDefault();
        refresh(tab.id);
        return;
      }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "l") {
        e.preventDefault();
        pathRef.current?.focus();
        pathRef.current?.select();
        return;
      }
      if (inEditable) return;
      if (e.key === "Backspace") {
        e.preventDefault();
        up(tab.id);
        return;
      }
      if (e.key === "F2") {
        if (localSelected.size === 1) {
          const p = Array.from(localSelected)[0];
          const entry = visibleEntries.find((x) => x.path === p);
          if (entry) {
            e.preventDefault();
            startRename(entry);
          }
        }
        return;
      }
      if (e.key === "Delete") {
        if (localSelected.size > 0) {
          e.preventDefault();
          doDelete();
        }
        return;
      }
      if (e.key === "Enter") {
        if (localSelected.size === 1) {
          const p = Array.from(localSelected)[0];
          const entry = visibleEntries.find((x) => x.path === p);
          if (entry) {
            e.preventDefault();
            openEntry(entry);
          }
        }
        return;
      }
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        if (visibleEntries.length === 0) return;
        e.preventDefault();
        const paths = visibleEntries.map((x) => x.path);
        const cur = lastClickPath ?? Array.from(localSelected)[0];
        let idx = cur ? paths.indexOf(cur) : -1;
        idx = e.key === "ArrowDown" ? Math.min(paths.length - 1, idx + 1) : Math.max(0, idx - 1);
        const next = paths[idx];
        setSelection([next]);
        setLastClickPath(next);
        return;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [
    tab.id,
    refresh,
    up,
    localSelected,
    visibleEntries,
    lastClickPath,
    startRename,
    doDelete,
    openEntry,
  ]);

  // Tauri 2 drag-drop from OS.
  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    let cancelled = false;
    (async () => {
      try {
        unlisten = await listen<{ paths: string[] }>(
          "tauri://drag-drop",
          (event) => {
            const paths = event.payload?.paths ?? [];
            if (paths.length === 0) return;
            for (const local of paths) {
              const filename = local.split(/[\\/]/).pop()!;
              const remote = joinPath(tab.cwd, filename);
              enqueueTransfer({
                sessionId: tab.id,
                direction: "upload",
                local,
                remote,
                total: null,
              });
              const recent = useStore
                .getState()
                .transfers.find(
                  (t) =>
                    t.local === local &&
                    t.remote === remote &&
                    t.status === "queued"
                );
              if (recent) {
                updateTransfer(recent.id, { status: "active" });
                invoke<number>("upload_file", {
                  sessionId: tab.id,
                  local,
                  remote,
                })
                  .then((bytes) => {
                    updateTransfer(recent.id, {
                      status: "done",
                      bytes,
                      total: bytes,
                    });
                    refresh(tab.id);
                    toast("success", `Uploaded ${filename}`);
                  })
                  .catch((err: any) => {
                    const msg = err?.message ?? String(err);
                    updateTransfer(recent.id, { status: "error", error: msg });
                    toast("error", `Upload failed: ${msg}`);
                  });
              }
            }
          }
        );
        if (cancelled && unlisten) unlisten();
      } catch {
        /* drag-drop unsupported in this env */
      }
    })();
    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, [tab.id, tab.cwd, enqueueTransfer, updateTransfer, refresh]);

  // Header sort click.
  const onSortClick = (k: SortKey) => {
    if (sortKey === k) {
      setSortDir((d) => (d === "asc" ? "desc" : "asc"));
    } else {
      setSortKey(k);
      setSortDir("asc");
    }
  };

  const sortChevron = (k: SortKey) =>
    sortKey === k ? (
      sortDir === "asc" ? (
        <ChevronUp size={12} className="sort-chevron" />
      ) : (
        <ChevronDown size={12} className="sort-chevron" />
      )
    ) : null;

  // Context menu builders.
  const buildFileMenu = (entry: DirEntry): ContextMenuItem[] => [
    {
      label: entry.is_dir ? "Open" : "Open",
      icon: entry.is_dir ? <Folder size={14} /> : <FileIcon size={14} />,
      onClick: () => openEntry(entry),
    },
    ...(entry.is_dir
      ? []
      : [
          {
            label: "Edit",
            icon: <FileEdit size={14} />,
            onClick: () =>
              window.dispatchEvent(
                new CustomEvent("skyhook:open-editor", {
                  detail: { tabId: tab.id, path: entry.path },
                })
              ),
          },
          {
            label: "Download",
            icon: <DownloadIcon size={14} />,
            onClick: async () => {
              setSelection([entry.path]);
              await doDownload();
            },
          },
        ]),
    { separator: true },
    {
      label: "Rename",
      icon: <Edit3 size={14} />,
      onClick: () => startRename(entry),
    },
    {
      label: "Copy path",
      icon: <Copy size={14} />,
      onClick: () => copyPath(entry.path),
    },
    { separator: true },
    {
      label: "Delete",
      icon: <Trash2 size={14} />,
      danger: true,
      onClick: () => doDelete([entry.path]),
    },
    { separator: true },
    {
      label: "Properties",
      icon: <Info size={14} />,
      onClick: () => showProperties(entry),
    },
  ];

  const buildEmptyMenu = (): ContextMenuItem[] => [
    {
      label: "Refresh",
      icon: <RefreshCw size={14} />,
      onClick: () => refresh(tab.id),
    },
    {
      label: "New folder",
      icon: <FolderPlus size={14} />,
      onClick: doMkdir,
    },
    {
      label: "Upload here",
      icon: <Upload size={14} />,
      onClick: () => doUpload(tab.cwd),
    },
  ];

  return (
    <div className="browser" ref={listRef}>
      {!tab.connected && (
        <div className="reconnect-banner">
          <span>
            ⚠ Session closed by server. This can happen if too many SFTP
            sessions were opened simultaneously.
          </span>
          <button
            className="btn btn-primary"
            onClick={() => useStore.getState().reconnect(tab.id)}
          >
            Reconnect
          </button>
        </div>
      )}
      <div className="toolbar">
        <div className="nav-btns">
          <button
            className="btn btn-ghost btn-icon"
            disabled={tab.historyIndex === 0}
            onClick={() => back(tab.id)}
            title="Back"
          >
            <ChevronLeft size={16} />
          </button>
          <button
            className="btn btn-ghost btn-icon"
            disabled={tab.historyIndex >= tab.history.length - 1}
            onClick={() => forward(tab.id)}
            title="Forward"
          >
            <ChevronRight size={16} />
          </button>
          <button
            className="btn btn-ghost btn-icon"
            onClick={() => up(tab.id)}
            title="Parent directory (Backspace)"
          >
            <ArrowUp size={16} />
          </button>
          <button
            className="btn btn-ghost btn-icon"
            onClick={() => refresh(tab.id)}
            title="Refresh (F5)"
          >
            <RefreshCw size={14} />
          </button>
        </div>
        <form
          className="path-bar"
          onSubmit={onPathSubmit}
          style={{ flex: 1 }}
        >
          <input
            ref={pathRef}
            value={pathInput}
            onChange={(e) => setPathInput(e.target.value)}
            spellCheck={false}
            title="Path (Ctrl+L to focus)"
          />
        </form>
        <button className="btn" onClick={doMkdir} title="New folder">
          <FolderPlus size={14} /> New
        </button>
        <button className="btn" onClick={() => doUpload()} title="Upload from local">
          <Upload size={14} /> Upload
        </button>
        <button className="btn" onClick={doDownload} title="Download selected">
          <DownloadIcon size={14} /> Download
        </button>
        <button
          className="btn btn-danger"
          onClick={() => doDelete()}
          disabled={localSelected.size === 0}
          title="Delete selected (Del)"
        >
          <Trash2 size={14} />
        </button>
      </div>

      <div className="file-list-header sortable">
        <div className="sort-cell" onClick={() => onSortClick("name")}>
          Name {sortChevron("name")}
        </div>
        <div className="sort-cell" onClick={() => onSortClick("size")}>
          Size {sortChevron("size")}
        </div>
        <div className="sort-cell" onClick={() => onSortClick("modified")}>
          Modified {sortChevron("modified")}
        </div>
        <div className="sort-cell" onClick={() => onSortClick("mode")}>
          Perms {sortChevron("mode")}
        </div>
      </div>

      <div
        className="file-list"
        onContextMenu={(e) => {
          // Empty-area right click (only if target is the list container).
          if (e.target === e.currentTarget) {
            e.preventDefault();
            setCtx({ x: e.clientX, y: e.clientY, entry: null });
          }
        }}
        onClick={(e) => {
          if (e.target === e.currentTarget) {
            setSelection([]);
            setLastClickPath(null);
          }
        }}
      >
        {tab.loading && <div className="loading-state">Loading…</div>}
        {tab.error && <div className="error-state">Error: {tab.error}</div>}
        {!tab.loading && !tab.error && visibleEntries.length === 0 && (
          <div className="empty-state">Empty directory</div>
        )}
        {!tab.loading &&
          !tab.error &&
          visibleEntries.map((e) => {
            const selected = localSelected.has(e.path);
            const isRenaming = renaming === e.path;
            return (
              <div
                key={e.path}
                className={`file-row ${selected ? "selected" : ""}`}
                onClick={(ev) => onRowClick(ev, e)}
                onDoubleClick={() => !isRenaming && openEntry(e)}
                onContextMenu={(ev) => {
                  ev.preventDefault();
                  ev.stopPropagation();
                  if (!selected) setSelection([e.path]);
                  setCtx({ x: ev.clientX, y: ev.clientY, entry: e });
                }}
              >
                <div className="name-cell">
                  {e.is_symlink ? (
                    <LinkIcon size={15} className="icon-link" />
                  ) : e.is_dir ? (
                    <Folder size={15} className="icon-folder" />
                  ) : (
                    <FileIcon size={15} className="icon-file" />
                  )}
                  {isRenaming ? (
                    <input
                      className="rename-input"
                      autoFocus
                      value={renameValue}
                      onChange={(ev) => setRenameValue(ev.target.value)}
                      onClick={(ev) => ev.stopPropagation()}
                      onBlur={() => commitRename(e)}
                      onKeyDown={(ev) => {
                        if (ev.key === "Enter") {
                          ev.preventDefault();
                          commitRename(e);
                        } else if (ev.key === "Escape") {
                          ev.preventDefault();
                          setRenaming(null);
                        }
                      }}
                    />
                  ) : (
                    <span>{e.name}</span>
                  )}
                </div>
                <div className="dim mono">
                  {e.is_dir ? "—" : fmtSize(e.size)}
                </div>
                <div className="dim">{fmtDate(e.modified)}</div>
                <div className="dim mono">{fmtMode(e.mode)}</div>
              </div>
            );
          })}
      </div>

      {ctx && (
        <ContextMenu
          x={ctx.x}
          y={ctx.y}
          items={ctx.entry ? buildFileMenu(ctx.entry) : buildEmptyMenu()}
          onClose={() => setCtx(null)}
        />
      )}
    </div>
  );
}
