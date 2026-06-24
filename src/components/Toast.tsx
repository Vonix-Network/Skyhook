import { useStore, ToastKind } from "../lib/store";
import {
  CheckCircle2,
  AlertCircle,
  AlertTriangle,
  Info,
  X,
} from "lucide-react";

function iconFor(kind: ToastKind) {
  switch (kind) {
    case "success":
      return <CheckCircle2 size={16} className="toast-icon" />;
    case "error":
      return <AlertCircle size={16} className="toast-icon" />;
    case "warning":
      return <AlertTriangle size={16} className="toast-icon" />;
    default:
      return <Info size={16} className="toast-icon" />;
  }
}

export function ToastContainer() {
  const toasts = useStore((s) => s.toasts);
  const dismiss = useStore((s) => s.dismissToast);
  if (toasts.length === 0) return null;
  return (
    <div className="toast-stack" role="status" aria-live="polite">
      {toasts.map((t) => (
        <div key={t.id} className={`toast ${t.kind}`}>
          {iconFor(t.kind)}
          <div className="toast-msg">{t.message}</div>
          <button
            className="toast-close"
            onClick={() => dismiss(t.id)}
            aria-label="Dismiss"
          >
            <X size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}
