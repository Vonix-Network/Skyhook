import { ReactNode } from "react";
import { Inbox } from "lucide-react";

export interface EmptyStateProps {
  title: string;
  hint?: string;
  icon?: ReactNode;
  action?: ReactNode;
}

export function EmptyState({ title, hint, icon, action }: EmptyStateProps) {
  return (
    <div className="empty-state-full">
      <div className="empty-state-icon">{icon ?? <Inbox size={28} />}</div>
      <div className="empty-state-title">{title}</div>
      {hint && <div className="empty-state-hint">{hint}</div>}
      {action && <div className="empty-state-action">{action}</div>}
    </div>
  );
}
