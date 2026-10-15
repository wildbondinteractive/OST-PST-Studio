// Shown when the window is closed while work is running. The host refused the close; this
// says what is running and lets the user keep working or stop it and close.

import { useState } from "react";
import { AlertTriangle } from "lucide-react";
import { api, errorMessage } from "@/lib/api";
import { useApp } from "@/lib/store";
import { fileName, jobKindLabel, pluralize } from "@/lib/utils";
import { Button, Dialog, DialogContent, DialogFooter, Spinner } from "@/components/ui";

export function CloseGuard() {
  const busy = useApp((s) => s.closeBlocked);
  const setBusy = useApp((s) => s.setCloseBlocked);
  const jobs = useApp((s) => s.jobs.filter((j) => j.status === "running" || j.status === "queued"));
  const toast = useApp((s) => s.toast);
  const [closing, setClosing] = useState(false);

  const close = async () => {
    setClosing(true);
    try {
      await api.closeApp(true);
    } catch (e) {
      toast({ tone: "warning", title: "Not closed yet", detail: errorMessage(e) });
      setClosing(false);
    }
  };

  return (
    <Dialog open={!!busy} onOpenChange={(o) => !o && !closing && setBusy(null)}>
      <DialogContent title="Work is still running" description="Closing now would interrupt it.">
        <div className="flex items-start gap-3 text-xs">
          <AlertTriangle size={18} className="mt-0.5 shrink-0 text-warning" />
          <div className="min-w-0 flex-1">
            {jobs.length > 0 && (
              <>
                <p className="font-medium">{pluralize(jobs.length, "job")} in progress</p>
                <ul className="mt-1 space-y-0.5 text-fg-muted">
                  {jobs.slice(0, 6).map((j) => (
                    <li key={j.id} className="truncate">
                      {jobKindLabel[j.kind]} · {fileName(j.source)} · {j.status === "queued" ? "queued" : j.progress.phase || "running"}
                    </li>
                  ))}
                  {jobs.length > 6 && <li>and {jobs.length - 6} more</li>}
                </ul>
              </>
            )}
            {!!busy?.operations && <p className={jobs.length ? "mt-2" : ""}>{pluralize(busy.operations, "file operation")} finishing; closing waits for them.</p>}
            <p className="mt-3 text-fg-muted">
              If you close now, jobs stop at a safe point and remove their unfinished output. Your original mailboxes are never changed.
            </p>
          </div>
        </div>
        <DialogFooter>
          <Button onClick={() => setBusy(null)} disabled={closing} autoFocus>Keep working</Button>
          <Button variant="danger" onClick={close} disabled={closing}>
            {closing ? <Spinner size={14} className="text-white" /> : null} {jobs.length ? "Stop jobs and close" : "Close when finished"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
