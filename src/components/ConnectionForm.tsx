import { useState } from "react";
import { useStore } from "../lib/store";
import { Connection, AuthMethod } from "../lib/api";
import { X } from "lucide-react";

type AuthKind = "password" | "key" | "agent";

function detectKind(auth: AuthMethod): AuthKind {
  if (auth === "agent" || (typeof auth === "object" && "agent" in auth)) return "agent";
  if (typeof auth === "object" && "key" in auth) return "key";
  return "password";
}

export function ConnectionForm() {
  const editing = useStore((s) => s.editingConnection);
  const close = useStore((s) => s.closeConnectionForm);
  const save = useStore((s) => s.saveConnection);

  const [name, setName] = useState(editing?.name ?? "");
  const [host, setHost] = useState(editing?.host ?? "");
  const [port, setPort] = useState(editing?.port ?? 22);
  const [username, setUsername] = useState(editing?.username ?? "");
  const [defaultPath, setDefaultPath] = useState(editing?.default_path ?? "");
  const [kind, setKind] = useState<AuthKind>(
    editing ? detectKind(editing.auth) : "password"
  );
  const [password, setPassword] = useState("");
  const [privateKey, setPrivateKey] = useState("");
  const [passphrase, setPassphrase] = useState("");
  const [saving, setSaving] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const canSave = name && host && username && port > 0;

  const submit = async () => {
    if (!canSave) return;
    setSaving(true);
    setErr(null);
    let auth: AuthMethod;
    if (kind === "agent") auth = "agent";
    else if (kind === "key")
      auth = { key: { private_key: privateKey, passphrase: passphrase || null } };
    else auth = { password: { password } };

    const conn: Connection = {
      id: editing?.id ?? "",
      name,
      host,
      port,
      username,
      auth,
      default_path: defaultPath || null,
      color: editing?.color ?? null,
      created_at: editing?.created_at ?? 0,
    };
    try {
      await save(conn);
    } catch (e: any) {
      setErr(e?.message ?? String(e));
      setSaving(false);
    }
  };

  return (
    <div className="modal-backdrop" onClick={close}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <div className="modal-title">
            {editing ? "Edit connection" : "New connection"}
          </div>
          <button className="btn btn-ghost btn-icon" onClick={close}>
            <X size={16} />
          </button>
        </div>
        <div className="modal-body">
          <div className="field">
            <label>Display name</label>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="My server"
              autoFocus
            />
          </div>
          <div className="field-row">
            <div className="field">
              <label>Host</label>
              <input
                value={host}
                onChange={(e) => setHost(e.target.value)}
                placeholder="example.com"
              />
            </div>
            <div className="field">
              <label>Port</label>
              <input
                type="number"
                value={port}
                onChange={(e) => setPort(parseInt(e.target.value || "22"))}
              />
            </div>
          </div>
          <div className="field">
            <label>Username</label>
            <input
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              placeholder="root"
            />
          </div>

          <div className="field">
            <label>Authentication</label>
            <div className="tab-pills">
              {(["password", "key", "agent"] as AuthKind[]).map((k) => (
                <div
                  key={k}
                  className={`tab-pill ${kind === k ? "active" : ""}`}
                  onClick={() => setKind(k)}
                >
                  {k === "password" ? "Password" : k === "key" ? "Private key" : "SSH agent"}
                </div>
              ))}
            </div>
          </div>

          {kind === "password" && (
            <div className="field">
              <label>Password {editing && "(leave blank to keep)"}</label>
              <input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="••••••••"
              />
            </div>
          )}
          {kind === "key" && (
            <>
              <div className="field">
                <label>Private key {editing && "(leave blank to keep)"}</label>
                <textarea
                  value={privateKey}
                  onChange={(e) => setPrivateKey(e.target.value)}
                  placeholder="-----BEGIN OPENSSH PRIVATE KEY-----&#10;…"
                />
              </div>
              <div className="field">
                <label>Passphrase (optional)</label>
                <input
                  type="password"
                  value={passphrase}
                  onChange={(e) => setPassphrase(e.target.value)}
                />
              </div>
            </>
          )}
          {kind === "agent" && (
            <div style={{ color: "var(--text-2)", fontSize: 12.5 }}>
              Uses your local <code>ssh-agent</code> (set <code>SSH_AUTH_SOCK</code>).
            </div>
          )}

          <div className="field">
            <label>Default path (optional)</label>
            <input
              value={defaultPath}
              onChange={(e) => setDefaultPath(e.target.value)}
              placeholder="/home/user"
            />
          </div>
          {err && <div style={{ color: "var(--danger)", fontSize: 12 }}>{err}</div>}
        </div>
        <div className="modal-footer">
          <button className="btn btn-ghost" onClick={close} disabled={saving}>
            Cancel
          </button>
          <button
            className="btn btn-primary"
            onClick={submit}
            disabled={!canSave || saving}
          >
            {saving ? "Saving…" : editing ? "Update" : "Create"}
          </button>
        </div>
      </div>
    </div>
  );
}
