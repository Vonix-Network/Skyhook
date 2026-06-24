import { useStore } from "../lib/store";
import { X } from "lucide-react";

export function Settings() {
  const settings = useStore((s) => s.settings);
  const update = useStore((s) => s.updateSettings);
  const close = useStore((s) => s.closeSettings);
  const openAgentSettings = useStore((s) => s.openAgentSettings);

  if (!settings) {
    return (
      <div className="modal-backdrop" onClick={close}>
        <div className="modal" onClick={(e) => e.stopPropagation()}>
          <div className="modal-header">
            <div className="modal-title">Settings</div>
            <button className="btn btn-ghost btn-icon" onClick={close}>
              <X size={16} />
            </button>
          </div>
          <div className="modal-body">
            <div className="dim">Loading settings…</div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="modal-backdrop" onClick={close}>
      <div
        className="modal modal-wide"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal-header">
          <div className="modal-title">Settings</div>
          <button className="btn btn-ghost btn-icon" onClick={close}>
            <X size={16} />
          </button>
        </div>
        <div className="modal-body">
          {/* Appearance */}
          <div className="settings-section">
            <div className="settings-section-title">Appearance</div>
            <div className="settings-row">
              <div>
                <div className="label">Theme</div>
                <div className="desc">Follow system, or force a palette.</div>
              </div>
              <select
                value={settings.theme}
                onChange={(e) => update({ theme: e.target.value })}
              >
                <option value="system">System</option>
                <option value="dark">Dark</option>
                <option value="light">Light</option>
              </select>
            </div>
          </div>

          {/* Behavior */}
          <div className="settings-section">
            <div className="settings-section-title">Behavior</div>
            <div className="settings-row">
              <div>
                <div className="label">Confirm before deleting</div>
                <div className="desc">
                  Prompt before removing remote files or folders.
                </div>
              </div>
              <input
                type="checkbox"
                className="switch"
                checked={settings.confirm_on_delete}
                onChange={(e) => update({ confirm_on_delete: e.target.checked })}
              />
            </div>
            <div className="settings-row">
              <div>
                <div className="label">Show hidden files</div>
                <div className="desc">Include dotfiles in directory listings.</div>
              </div>
              <input
                type="checkbox"
                className="switch"
                checked={settings.show_hidden_files}
                onChange={(e) => update({ show_hidden_files: e.target.checked })}
              />
            </div>
          </div>

          {/* Transfers */}
          <div className="settings-section">
            <div className="settings-section-title">Transfers</div>
            <div className="settings-row">
              <div>
                <div className="label">Concurrent transfers</div>
                <div className="desc">
                  Maximum parallel file transfers per session (1–8).
                </div>
              </div>
              <input
                type="number"
                min={1}
                max={8}
                value={settings.transfer_concurrency}
                onChange={(e) =>
                  update({
                    transfer_concurrency: Math.max(
                      1,
                      Math.min(8, parseInt(e.target.value || "1") || 1),
                    ),
                  })
                }
                style={{ width: 80 }}
              />
            </div>
          </div>

          {/* Editor */}
          <div className="settings-section">
            <div className="settings-section-title">Editor</div>
            <div className="settings-row">
              <div>
                <div className="label">Word wrap</div>
                <div className="desc">Wrap long lines in the file editor.</div>
              </div>
              <input
                type="checkbox"
                className="switch"
                checked={settings.editor_word_wrap}
                onChange={(e) => update({ editor_word_wrap: e.target.checked })}
              />
            </div>
          </div>

          {/* Agent */}
          <div className="settings-section">
            <div className="settings-section-title">Agent</div>
            <div className="settings-row">
              <div>
                <div className="label">Agent settings</div>
                <div className="desc">
                  API keys, default provider &amp; model, approval mode, reasoning effort.
                </div>
              </div>
              <button
                className="btn btn-secondary"
                onClick={() => {
                  close();
                  openAgentSettings();
                }}
              >
                Open Agent Settings
              </button>
            </div>
          </div>
        </div>
        <div className="modal-footer">
          <button className="btn btn-primary" onClick={close}>
            Done
          </button>
        </div>
      </div>
    </div>
  );
}
