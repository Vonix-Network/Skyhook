import { useEffect } from "react";
import { useStore } from "../lib/store";
import { X, Trash2 } from "lucide-react";

function relTime(iso: string): string {
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return iso;
  return new Date(t).toLocaleString();
}

export function KnownHostsManager() {
  const close = useStore((s) => s.closeKnownHosts);
  const entries = useStore((s) => s.knownHosts);
  const remove = useStore((s) => s.removeKnownHost);
  const reload = useStore((s) => s.loadKnownHosts);

  useEffect(() => {
    reload().catch((e) => console.error("loadKnownHosts", e));
  }, [reload]);

  return (
    <div className="modal-backdrop" onClick={close}>
      <div
        className="modal modal-wide"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal-header">
          <div className="modal-title">Trusted hosts</div>
          <button className="btn btn-ghost btn-icon" onClick={close}>
            <X size={16} />
          </button>
        </div>
        <div className="modal-body" style={{ padding: 0 }}>
          {entries.length === 0 ? (
            <div className="kh-empty">No trusted hosts yet.</div>
          ) : (
            <table className="kh-table">
              <thead>
                <tr>
                  <th>Host</th>
                  <th>Port</th>
                  <th>Algorithm</th>
                  <th>Fingerprint</th>
                  <th>Added</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {entries.map((e) => (
                  <tr key={`${e.host}:${e.port}`}>
                    <td>{e.host}</td>
                    <td>{e.port}</td>
                    <td>{e.algo}</td>
                    <td className="fingerprint" title={e.fingerprint}>
                      {e.fingerprint}
                    </td>
                    <td className="dim">{relTime(e.added_at)}</td>
                    <td>
                      <button
                        className="btn btn-ghost btn-icon"
                        title="Remove"
                        onClick={() => remove(e.host, e.port)}
                      >
                        <Trash2 size={14} />
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
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
