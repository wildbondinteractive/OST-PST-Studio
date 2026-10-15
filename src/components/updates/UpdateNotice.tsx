// A quiet card in the corner when there is something to decide about an update: a new
// version (notify mode), a downloaded one (automatic mode), or a background download that
// failed. Dismissing it only hides it for this session; "Remind me later" snoozes the
// version for a day; neither changes the update preference.

import { useState } from "react";
import { ArrowUpCircle, X } from "lucide-react";
import { api, errorMessage, updateInfoOf } from "@/lib/api";
import { useApp } from "@/lib/store";
import { Button } from "@/components/ui";
import { updateHeadline } from "./UpdateDialog";

export function UpdateNotice() {
  const status = useApp((s) => s.update);
  const setUpdate = useApp((s) => s.setUpdate);
  const openDialog = useApp((s) => s.setUpdateDialogOpen);
  const dialogOpen = useApp((s) => s.updateDialogOpen);
  const toast = useApp((s) => s.toast);
  const [dismissed, setDismissed] = useState<string | null>(null);

  if (!status || dialogOpen) return null;
  const info = updateInfoOf(status);
  const st = status.state;
  const wanted =
    (st.phase === "available" && status.mode !== "automatic") ||
    (st.phase === "available" && status.installKind === "portable") ||
    st.phase === "ready" ||
    (st.phase === "failed" && !!st.info);
  if (!wanted || !info || status.snoozed) return null;
  const key = `${st.phase}:${info.version}`;
  if (dismissed === key) return null;
  const head = updateHeadline(status);

  return (
    <div role="region" aria-label="Software update" className="absolute bottom-4 left-4 z-30 w-80 rounded-lg border border-border bg-surface p-3 shadow-xl">
      <div className="flex items-start gap-2.5">
        <ArrowUpCircle size={18} className="mt-0.5 shrink-0 text-accent" />
        <div className="min-w-0 flex-1">
          <p className="text-[13px] font-semibold">{head.title}</p>
          <p className="mt-0.5 text-xs text-fg-muted">{head.detail ?? `You have ${status.currentVersion}.`}</p>
        </div>
        <button className="rounded p-0.5 text-fg-subtle hover:bg-surface-3 hover:text-fg" aria-label="Dismiss" onClick={() => setDismissed(key)}>
          <X size={14} />
        </button>
      </div>
      <div className="mt-3 flex justify-end gap-2">
        {st.phase !== "failed" && (
          <Button size="sm" variant="ghost" onClick={() => api.updateRemindLater().then(setUpdate).catch((e) => toast({ tone: "danger", title: "Update", detail: errorMessage(e) }))}>
            Later
          </Button>
        )}
        <Button size="sm" variant="primary" onClick={() => openDialog(true)}>
          {st.phase === "ready" ? "Restart options" : "Details"}
        </Button>
      </div>
    </div>
  );
}
