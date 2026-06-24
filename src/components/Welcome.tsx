import { useStore } from "../lib/store";
import { Plus, Server } from "lucide-react";

export function Welcome() {
  const openForm = useStore((s) => s.openConnectionForm);
  const connections = useStore((s) => s.connections);
  const open = useStore((s) => s.openConnection);

  return (
    <div className="welcome">
      <div className="brand-mark" style={{ width: 64, height: 64, fontSize: 28, borderRadius: 18 }}>
        S
      </div>
      <h1>Welcome to Skyhook</h1>
      <p>
        A modern SFTP client. Manage your servers, edit files in place, and move data
        around without leaving the keyboard.
      </p>
      <div style={{ display: "flex", gap: 10 }}>
        <button className="btn btn-primary" onClick={() => openForm(null)}>
          <Plus size={15} /> New connection
        </button>
        {connections.length > 0 && (
          <button
            className="btn"
            onClick={() => open(connections[0].id).catch((e) => alert(e?.message ?? e))}
          >
            <Server size={14} /> Connect to {connections[0].name}
          </button>
        )}
      </div>
    </div>
  );
}
