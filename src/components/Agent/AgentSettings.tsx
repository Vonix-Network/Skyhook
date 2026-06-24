import { useEffect, useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { X, Check, Trash2 } from "lucide-react";
import { useStore } from "../../lib/store";

type Provider = "anthropic" | "openai";
type ApprovalMode = "manual" | "auto_read" | "yolo";
type ReasoningEffort = "low" | "medium" | "high";

interface AgentSettingsData {
  default_provider: Provider;
  anthropic_model: string;
  openai_model: string;
  max_turns_per_invocation: number;
  default_approval_mode: ApprovalMode;
  reasoning_effort: ReasoningEffort;
  show_thinking: boolean;
}

const DEFAULT_SETTINGS: AgentSettingsData = {
  default_provider: "anthropic",
  anthropic_model: "claude-sonnet-4-5-20250929",
  openai_model: "gpt-4o",
  max_turns_per_invocation: 20,
  default_approval_mode: "manual",
  reasoning_effort: "medium",
  show_thinking: false,
};

interface Props {
  onClose(): void;
}

export function AgentSettings({ onClose }: Props) {
  const toast = useStore((s) => s.toast);

  const [settings, setSettings] = useState<AgentSettingsData | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);

  const [anthropicModels, setAnthropicModels] = useState<string[]>([]);
  const [openaiModels, setOpenaiModels] = useState<string[]>([]);

  const [hasAnthropic, setHasAnthropic] = useState(false);
  const [hasOpenai, setHasOpenai] = useState(false);

  const [anthropicKey, setAnthropicKey] = useState("");
  const [openaiKey, setOpenaiKey] = useState("");

  const [yoloConfirm, setYoloConfirm] = useState("");
  const previousMode = useRef<ApprovalMode>("manual");

  const refreshKeyStatus = useCallback(async () => {
    try {
      const [a, o] = await Promise.all([
        invoke<boolean>("agent_has_api_key", { provider: "anthropic" }),
        invoke<boolean>("agent_has_api_key", { provider: "openai" }),
      ]);
      setHasAnthropic(a);
      setHasOpenai(o);
    } catch {
      /* tolerate */
    }
  }, []);

  useEffect(() => {
    let active = true;
    (async () => {
      try {
        const loaded = await invoke<AgentSettingsData>("agent_get_settings");
        if (!active) return;
        const merged = { ...DEFAULT_SETTINGS, ...(loaded || {}) };
        setSettings(merged);
        previousMode.current = merged.default_approval_mode;
      } catch (e: any) {
        if (!active) return;
        setSettings(DEFAULT_SETTINGS);
      } finally {
        if (active) setLoading(false);
      }
      await refreshKeyStatus();
      try {
        const [a, o] = await Promise.all([
          invoke<string[]>("agent_list_models", { provider: "anthropic" }),
          invoke<string[]>("agent_list_models", { provider: "openai" }),
        ]);
        if (!active) return;
        setAnthropicModels(a || []);
        setOpenaiModels(o || []);
      } catch {
        /* tolerate */
      }
    })();
    return () => {
      active = false;
    };
  }, [refreshKeyStatus]);

  // Esc closes
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [onClose]);

  const patch = (p: Partial<AgentSettingsData>) =>
    setSettings((s) => (s ? { ...s, ...p } : s));

  const saveKey = async (provider: Provider) => {
    const key = provider === "anthropic" ? anthropicKey : openaiKey;
    if (!key.trim()) {
      toast("API key is empty", "warning");
      return;
    }
    try {
      await invoke("agent_set_api_key", { provider, key: key.trim() });
      if (provider === "anthropic") {
        setAnthropicKey("");
        setHasAnthropic(true);
      } else {
        setOpenaiKey("");
        setHasOpenai(true);
      }
      toast(`${provider} key saved`, "success");
    } catch (e: any) {
      toast(`Failed to save key: ${e?.message ?? e}`, "error");
    }
  };

  const removeKey = async (provider: Provider) => {
    if (!window.confirm(`Remove ${provider} API key?`)) return;
    try {
      await invoke("agent_remove_api_key", { provider });
      if (provider === "anthropic") setHasAnthropic(false);
      else setHasOpenai(false);
      toast(`${provider} key removed`, "success");
    } catch (e: any) {
      toast(`Failed to remove key: ${e?.message ?? e}`, "error");
    }
  };

  const onPickApprovalMode = (mode: ApprovalMode) => {
    if (mode === "yolo" && settings?.default_approval_mode !== "yolo") {
      // require confirmation typed
      const typed = window.prompt(
        "Yolo mode auto-approves ALL tool calls including writes and commands.\nType YOLO to confirm:",
      );
      if (typed !== "YOLO") {
        toast("Yolo mode not enabled (confirmation failed)", "warning");
        return;
      }
      setYoloConfirm("YOLO");
    }
    patch({ default_approval_mode: mode });
  };

  const save = async () => {
    if (!settings) return;
    setSaving(true);
    try {
      await invoke("agent_save_settings", { settings });
      toast("Agent settings saved", "success");
      onClose();
    } catch (e: any) {
      toast(`Failed to save: ${e?.message ?? e}`, "error");
    } finally {
      setSaving(false);
    }
  };

  const showReasoning =
    settings?.default_provider === "openai" &&
    /^o[134]/i.test(settings?.openai_model || "");

  // Suppress unused warning while still tracking state for future use
  void yoloConfirm;

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal modal-wide"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label="Agent Settings"
      >
        <div className="modal-header">
          <div className="modal-title">Agent Settings</div>
          <button className="btn btn-ghost btn-icon" onClick={onClose} aria-label="Close">
            <X size={16} />
          </button>
        </div>
        <div className="modal-body">
          {loading || !settings ? (
            <div className="dim">Loading agent settings…</div>
          ) : (
            <>
              {/* API Keys */}
              <div className="settings-section">
                <div className="settings-section-title">API Keys</div>

                <div className="agent-key-row">
                  <div className="agent-key-label">
                    <div className="label">Anthropic</div>
                    <div className={`agent-key-status ${hasAnthropic ? "ok" : "missing"}`}>
                      {hasAnthropic ? (
                        <>
                          <Check size={11} /> Configured
                        </>
                      ) : (
                        "Not set"
                      )}
                    </div>
                  </div>
                  <input
                    type="password"
                    autoComplete="new-password"
                    placeholder={hasAnthropic ? "Replace key…" : "sk-ant-…"}
                    value={anthropicKey}
                    onChange={(e) => setAnthropicKey(e.target.value)}
                    className="agent-key-input"
                  />
                  <button
                    className="btn btn-primary btn-sm"
                    onClick={() => saveKey("anthropic")}
                    disabled={!anthropicKey.trim()}
                  >
                    Save
                  </button>
                  {hasAnthropic && (
                    <button
                      className="btn btn-ghost btn-icon btn-danger"
                      onClick={() => removeKey("anthropic")}
                      title="Remove key"
                    >
                      <Trash2 size={13} />
                    </button>
                  )}
                </div>

                <div className="agent-key-row">
                  <div className="agent-key-label">
                    <div className="label">OpenAI</div>
                    <div className={`agent-key-status ${hasOpenai ? "ok" : "missing"}`}>
                      {hasOpenai ? (
                        <>
                          <Check size={11} /> Configured
                        </>
                      ) : (
                        "Not set"
                      )}
                    </div>
                  </div>
                  <input
                    type="password"
                    autoComplete="new-password"
                    placeholder={hasOpenai ? "Replace key…" : "sk-…"}
                    value={openaiKey}
                    onChange={(e) => setOpenaiKey(e.target.value)}
                    className="agent-key-input"
                  />
                  <button
                    className="btn btn-primary btn-sm"
                    onClick={() => saveKey("openai")}
                    disabled={!openaiKey.trim()}
                  >
                    Save
                  </button>
                  {hasOpenai && (
                    <button
                      className="btn btn-ghost btn-icon btn-danger"
                      onClick={() => removeKey("openai")}
                      title="Remove key"
                    >
                      <Trash2 size={13} />
                    </button>
                  )}
                </div>
              </div>

              {/* Provider + model */}
              <div className="settings-section">
                <div className="settings-section-title">Provider & Models</div>

                <div className="settings-row">
                  <div>
                    <div className="label">Default provider</div>
                    <div className="desc">Used for new conversations.</div>
                  </div>
                  <div className="agent-radio-group">
                    <label className="agent-radio">
                      <input
                        type="radio"
                        name="provider"
                        checked={settings.default_provider === "anthropic"}
                        onChange={() => patch({ default_provider: "anthropic" })}
                      />
                      Anthropic
                    </label>
                    <label className="agent-radio">
                      <input
                        type="radio"
                        name="provider"
                        checked={settings.default_provider === "openai"}
                        onChange={() => patch({ default_provider: "openai" })}
                      />
                      OpenAI
                    </label>
                  </div>
                </div>

                <div className="settings-row">
                  <div>
                    <div className="label">Anthropic model</div>
                  </div>
                  <select
                    value={settings.anthropic_model}
                    onChange={(e) => patch({ anthropic_model: e.target.value })}
                  >
                    {anthropicModels.length === 0 && (
                      <option value={settings.anthropic_model}>
                        {settings.anthropic_model}
                      </option>
                    )}
                    {anthropicModels.map((m) => (
                      <option key={m} value={m}>
                        {m}
                      </option>
                    ))}
                  </select>
                </div>

                <div className="settings-row">
                  <div>
                    <div className="label">OpenAI model</div>
                  </div>
                  <select
                    value={settings.openai_model}
                    onChange={(e) => patch({ openai_model: e.target.value })}
                  >
                    {openaiModels.length === 0 && (
                      <option value={settings.openai_model}>
                        {settings.openai_model}
                      </option>
                    )}
                    {openaiModels.map((m) => (
                      <option key={m} value={m}>
                        {m}
                      </option>
                    ))}
                  </select>
                </div>

                {showReasoning && (
                  <div className="settings-row">
                    <div>
                      <div className="label">Reasoning effort</div>
                      <div className="desc">For o1/o3/o4-family models.</div>
                    </div>
                    <div className="agent-radio-group">
                      {(["low", "medium", "high"] as ReasoningEffort[]).map((v) => (
                        <label key={v} className="agent-radio">
                          <input
                            type="radio"
                            name="reasoning"
                            checked={settings.reasoning_effort === v}
                            onChange={() => patch({ reasoning_effort: v })}
                          />
                          {v}
                        </label>
                      ))}
                    </div>
                  </div>
                )}
              </div>

              {/* Behavior */}
              <div className="settings-section">
                <div className="settings-section-title">Behavior</div>

                <div className="settings-row">
                  <div>
                    <div className="label">Max turns per invocation</div>
                    <div className="desc">Safety cap on agent loop iterations (1–100).</div>
                  </div>
                  <input
                    type="number"
                    min={1}
                    max={100}
                    value={settings.max_turns_per_invocation}
                    onChange={(e) =>
                      patch({
                        max_turns_per_invocation: Math.max(
                          1,
                          Math.min(100, parseInt(e.target.value || "1") || 1),
                        ),
                      })
                    }
                    style={{ width: 80 }}
                  />
                </div>

                <div className="settings-row">
                  <div>
                    <div className="label">Default approval mode</div>
                    <div className="desc">
                      Manual = approve every tool. Auto-read = auto-approve read-only.
                      Yolo = auto-approve all.
                    </div>
                  </div>
                  <div className="agent-radio-group">
                    <label className="agent-radio">
                      <input
                        type="radio"
                        name="approval"
                        checked={settings.default_approval_mode === "manual"}
                        onChange={() => onPickApprovalMode("manual")}
                      />
                      Manual
                    </label>
                    <label className="agent-radio">
                      <input
                        type="radio"
                        name="approval"
                        checked={settings.default_approval_mode === "auto_read"}
                        onChange={() => onPickApprovalMode("auto_read")}
                      />
                      Auto-read
                    </label>
                    <label className="agent-radio agent-radio-danger">
                      <input
                        type="radio"
                        name="approval"
                        checked={settings.default_approval_mode === "yolo"}
                        onChange={() => onPickApprovalMode("yolo")}
                      />
                      Yolo
                    </label>
                  </div>
                </div>

                <div className="settings-row">
                  <div>
                    <div className="label">Show thinking</div>
                    <div className="desc">
                      Display the model's extended-thinking blocks when available.
                    </div>
                  </div>
                  <input
                    type="checkbox"
                    className="switch"
                    checked={settings.show_thinking}
                    onChange={(e) => patch({ show_thinking: e.target.checked })}
                  />
                </div>
              </div>
            </>
          )}
        </div>
        <div className="modal-footer">
          <button className="btn btn-ghost" onClick={onClose} disabled={saving}>
            Cancel
          </button>
          <button
            className="btn btn-primary"
            onClick={save}
            disabled={saving || loading || !settings}
          >
            {saving ? "Saving…" : "Save"}
          </button>
        </div>
      </div>
    </div>
  );
}
