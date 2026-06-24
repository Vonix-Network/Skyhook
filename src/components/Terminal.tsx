import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { Terminal as XTerm } from "xterm";
import { FitAddon } from "xterm-addon-fit";
import { TerminalIcon, X, Plus } from "lucide-react";
import "xterm/css/xterm.css";
import { useTerminalStore } from "../lib/terminal-store";

interface TerminalProps {
  tabId: string;
  tabName: string;
}

interface ShellOpenResult {
  id: string;
  session_id: string;
}

interface ShellOutputEvent {
  shell_id: string;
  data: string;
}

interface ShellClosedEvent {
  shell_id: string;
  exit_code: number | null;
}

/** Skyhook palette → xterm theme. */
const XTERM_THEME = {
  background: "#0b0d12",
  foreground: "#e7eaf3",
  cursor: "#5ee5d1",
  cursorAccent: "#0b0d12",
  selectionBackground: "rgba(94, 229, 209, 0.30)",
  black: "#11141b",
  red: "#f87171",
  green: "#4ade80",
  yellow: "#fbbf24",
  blue: "#60a5fa",
  magenta: "#c084fc",
  cyan: "#5ee5d1",
  white: "#e7eaf3",
  brightBlack: "#4a525e",
  brightRed: "#fca5a5",
  brightGreen: "#86efac",
  brightYellow: "#fcd34d",
  brightBlue: "#93c5fd",
  brightMagenta: "#d8b4fe",
  brightCyan: "#7df0dd",
  brightWhite: "#ffffff",
};

export function Terminal({ tabId, tabName }: TerminalProps) {
  const closeTerminal = useTerminalStore((s) => s.closeTerminal);
  const setShellId = useTerminalStore((s) => s.setShellId);
  const setExitCode = useTerminalStore((s) => s.setExitCode);

  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<XTerm | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const shellIdRef = useRef<string | null>(null);
  const closedRef = useRef(false);

  const [status, setStatus] = useState<"connecting" | "running" | "closed">(
    "connecting",
  );
  const [exitInfo, setExitInfo] = useState<number | null>(null);

  const close = useCallback(() => {
    closeTerminal(tabId);
  }, [closeTerminal, tabId]);

  // Bootstrap xterm + open shell + wire events.
  useEffect(() => {
    if (!containerRef.current) return;
    let disposed = false;
    let unlistenOutput: UnlistenFn | null = null;
    let unlistenClosed: UnlistenFn | null = null;

    const term = new XTerm({
      cursorBlink: true,
      cursorStyle: "block",
      fontFamily:
        'ui-monospace, "JetBrains Mono", "SF Mono", Menlo, Consolas, monospace',
      fontSize: 13,
      lineHeight: 1.2,
      scrollback: 5000,
      allowProposedApi: true,
      theme: XTERM_THEME,
      smoothScrollDuration: 80,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(containerRef.current);
    termRef.current = term;
    fitRef.current = fit;

    // Initial size — fit before requesting the shell so backend gets accurate dims.
    try {
      fit.fit();
    } catch {
      /* container not yet measured; xterm defaults are fine */
    }

    term.focus();

    // Forward user input -> shell_write.
    const dataDisposable = term.onData((input) => {
      const sid = shellIdRef.current;
      if (!sid || closedRef.current) return;
      const bytes = Array.from(new TextEncoder().encode(input));
      invoke("shell_write", { shellId: sid, data: bytes }).catch((e) => {
        console.error("shell_write failed", e);
      });
    });

    // Open shell.
    (async () => {
      try {
        // Pre-subscribe so we don't lose the first burst of output.
        unlistenOutput = await listen<ShellOutputEvent>(
          "shell-output",
          (ev) => {
            const p = ev.payload;
            if (!p || p.shell_id !== shellIdRef.current) return;
            term.write(p.data);
          },
        );
        unlistenClosed = await listen<ShellClosedEvent>(
          "shell-closed",
          (ev) => {
            const p = ev.payload;
            if (!p || p.shell_id !== shellIdRef.current) return;
            closedRef.current = true;
            setStatus("closed");
            setExitInfo(p.exit_code);
            setExitCode(tabId, p.exit_code);
            const codeStr =
              p.exit_code == null ? "?" : String(p.exit_code);
            term.write(`\r\n\x1b[33m[Process exited with code ${codeStr}]\x1b[0m\r\n`);
            term.options.disableStdin = true;
          },
        );

        const result = await invoke<ShellOpenResult>("shell_open", {
          sessionId: tabId,
          cols: term.cols,
          rows: term.rows,
        });
        if (disposed) {
          // Race: component unmounted while opening — clean up the orphan.
          invoke("shell_close", { shellId: result.id }).catch(() => {});
          return;
        }
        shellIdRef.current = result.id;
        setShellId(tabId, result.id);
        setStatus("running");
      } catch (e: any) {
        const msg = e?.message ?? String(e);
        term.write(`\r\n\x1b[31mFailed to open shell: ${msg}\x1b[0m\r\n`);
        setStatus("closed");
        closedRef.current = true;
      }
    })();

    // Resize forwarding.
    const sendResize = () => {
      if (!fitRef.current || !termRef.current) return;
      try {
        fitRef.current.fit();
      } catch {
        return;
      }
      const sid = shellIdRef.current;
      if (!sid || closedRef.current) return;
      const cols = termRef.current.cols;
      const rows = termRef.current.rows;
      invoke("shell_resize", { shellId: sid, cols, rows }).catch((e) => {
        console.error("shell_resize failed", e);
      });
    };

    let rafId: number | null = null;
    const schedule = () => {
      if (rafId != null) return;
      rafId = window.requestAnimationFrame(() => {
        rafId = null;
        sendResize();
      });
    };

    const ro = new ResizeObserver(() => schedule());
    ro.observe(containerRef.current);

    return () => {
      disposed = true;
      ro.disconnect();
      if (rafId != null) cancelAnimationFrame(rafId);
      dataDisposable.dispose();
      if (unlistenOutput) unlistenOutput();
      if (unlistenClosed) unlistenClosed();
      const sid = shellIdRef.current;
      if (sid && !closedRef.current) {
        invoke("shell_close", { shellId: sid }).catch(() => {});
      }
      try {
        term.dispose();
      } catch {
        /* noop */
      }
      termRef.current = null;
      fitRef.current = null;
    };
  }, [tabId, setShellId, setExitCode]);

  // Copy/paste keyboard shortcuts at the document level so they fire while
  // xterm has focus (xterm swallows most keys otherwise).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      // Only handle when the terminal area or its descendants are focused.
      if (!containerRef.current?.contains(target)) {
        // Esc closes the terminal view, but only when focus is on the body
        // (i.e. not inside an input, modal, dialog, etc.).
        if (e.key === "Escape") {
          if (document.activeElement === document.body) {
            e.preventDefault();
            close();
          }
        }
        return;
      }
      const term = termRef.current;
      if (!term) return;
      const isMac =
        typeof navigator !== "undefined" &&
        /mac|iphone|ipad/i.test(navigator.platform);
      const mod = isMac ? e.metaKey : e.ctrlKey && e.shiftKey;
      const key = e.key.toLowerCase();

      if (mod && key === "c") {
        const sel = term.getSelection();
        if (sel && sel.length > 0) {
          e.preventDefault();
          navigator.clipboard.writeText(sel).catch(() => {});
          return;
        }
      }
      if (mod && key === "v") {
        e.preventDefault();
        navigator.clipboard
          .readText()
          .then((text) => {
            if (!text) return;
            const sid = shellIdRef.current;
            if (!sid || closedRef.current) return;
            const bytes = Array.from(new TextEncoder().encode(text));
            invoke("shell_write", { shellId: sid, data: bytes }).catch(() => {});
          })
          .catch(() => {});
        return;
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [close]);

  const newShell = useCallback(() => {
    // Multi-shell deferred — for now just refocus the active terminal.
    termRef.current?.focus();
  }, []);

  return (
    <div className="terminal-view">
      <div className="terminal-header">
        <div className="terminal-title">
          <TerminalIcon size={14} />
          <span className="terminal-title-text">shell @ {tabName}</span>
          <span className={`terminal-status terminal-status-${status}`}>
            {status === "connecting"
              ? "connecting…"
              : status === "running"
                ? "connected"
                : exitInfo == null
                  ? "closed"
                  : `exit ${exitInfo}`}
          </span>
        </div>
        <div className="terminal-actions">
          <button
            className="btn btn-ghost btn-icon"
            onClick={newShell}
            title="Focus terminal"
          >
            <Plus size={14} />
          </button>
          <button
            className="btn btn-ghost btn-icon"
            onClick={close}
            title="Close shell (Esc)"
          >
            <X size={14} />
          </button>
        </div>
      </div>
      <div
        className="terminal-body"
        ref={containerRef}
        onClick={() => termRef.current?.focus()}
      />
    </div>
  );
}
