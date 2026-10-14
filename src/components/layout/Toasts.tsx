import { AlertTriangle, CheckCircle2, Info, X, XCircle } from "lucide-react";
import { useApp } from "@/lib/store";
import { cn } from "@/lib/utils";

export function Toasts() {
  const toasts = useApp((s) => s.toasts);
  const dismiss = useApp((s) => s.dismissToast);
  if (toasts.length === 0) return null;
  return (
    <div className="pointer-events-none fixed bottom-4 right-4 z-[60] flex w-80 flex-col gap-2" role="status" aria-live="polite">
      {toasts.map((t) => (
        <div
          key={t.id}
          className={cn(
            "pointer-events-auto flex items-start gap-2.5 rounded-lg border bg-surface p-3 shadow-xl",
            t.tone === "success" && "border-success/40",
            t.tone === "warning" && "border-warning/40",
            t.tone === "danger" && "border-danger/40",
            t.tone === "info" && "border-border",
          )}
        >
          <span className={cn("mt-0.5 shrink-0", {
            "text-success": t.tone === "success",
            "text-warning": t.tone === "warning",
            "text-danger": t.tone === "danger",
            "text-accent": t.tone === "info",
          })}>
            {t.tone === "success" && <CheckCircle2 size={16} />}
            {t.tone === "warning" && <AlertTriangle size={16} />}
            {t.tone === "danger" && <XCircle size={16} />}
            {t.tone === "info" && <Info size={16} />}
          </span>
          <div className="min-w-0 flex-1">
            <p className="text-[13px] font-medium">{t.title}</p>
            {t.detail && <p className="mt-0.5 break-words text-xs text-fg-muted selectable">{t.detail}</p>}
          </div>
          <button onClick={() => dismiss(t.id)} className="shrink-0 rounded p-0.5 text-fg-subtle hover:bg-surface-3 hover:text-fg" aria-label="Dismiss">
            <X size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}
