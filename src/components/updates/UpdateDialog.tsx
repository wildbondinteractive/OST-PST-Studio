// The update details: versions, date, release notes, progress, and the actions that apply
// right now. Every action goes through the host, which enforces the rules (stable channel,
// signature, nothing running) whatever the window does.

import { useState } from "react";
import { AlertTriangle, CheckCircle2, Download, ExternalLink, RefreshCw, RotateCw, ShieldCheck } from "lucide-react";
import { api, errorMessage, updateInfoOf, type UpdateStatus } from "@/lib/api";
import { useApp } from "@/lib/store";
import { formatDate, humanBytes, pluralize } from "@/lib/utils";
import { Badge, Button, Dialog, DialogContent, DialogFooter, Progress, Spinner } from "@/components/ui";
import { ReleaseNotes } from "./ReleaseNotes";

/// What the status means in one line, for the dialog, the notice and Settings.
export function updateHeadline(s: UpdateStatus): { title: string; detail?: string; tone: "info" | "success" | "warning" | "danger" } {
  const st = s.state;
  switch (st.phase) {
    case "idle":
      return { title: "Updates have not been checked yet", tone: "info" };
    case "checking":
      return { title: "Checking for updates", tone: "info" };
    case "upToDate":
      return { title: "You're running the latest version of OST/PST Studio.", tone: "success" };
    case "available":
      return { title: `OST/PST Studio ${st.info.version} is available`, detail: s.installKind === "portable" ? "This portable copy updates by downloading the new version from GitHub." : undefined, tone: "info" };
    case "downloading":
      return { title: `Downloading ${st.info.version}`, detail: st.total ? `${humanBytes(st.received)} of ${humanBytes(st.total)}` : humanBytes(st.received), tone: "info" };
    case "verifying":
      return { title: `Verifying the ${st.info.version} download`, detail: "Checking its signature against the key built into this application.", tone: "info" };
    case "ready":
      return {
        title: `${st.info.version} is downloaded and verified`,
        detail: s.installOnClose ? "It installs when you close OST/PST Studio, or restart now." : "Restart to install it.",
        tone: "success",
      };
    case "installing":
      return { title: `Installing ${st.info.version}`, detail: "OST/PST Studio closes while the installer runs, then starts again.", tone: "info" };
    case "failed":
      return { title: st.kind === "signature" ? "Update rejected" : "Update failed", detail: st.message, tone: st.kind === "signature" ? "danger" : "warning" };
  }
}

function busyText(s: UpdateStatus) {
  const parts = [];
  if (s.busy.jobs) parts.push(pluralize(s.busy.jobs, "job"));
  if (s.busy.operations) parts.push(pluralize(s.busy.operations, "file operation"));
  return parts.join(" and ");
}

export function UpdateDialog() {
  const open = useApp((s) => s.updateDialogOpen);
  const setOpen = useApp((s) => s.setUpdateDialogOpen);
  const status = useApp((s) => s.update);
  const setUpdate = useApp((s) => s.setUpdate);
  const toast = useApp((s) => s.toast);
  const setPage = useApp((s) => s.setPage);
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [showDetail, setShowDetail] = useState(false);

  if (!status) return null;
  const info = updateInfoOf(status);
  const st = status.state;
  const head = updateHeadline(status);
  const blocked = status.busy.jobs + status.busy.operations > 0;

  const run = async (name: string, f: () => Promise<unknown>) => {
    setBusyAction(name);
    try {
      await f();
    } catch (e) {
      toast({ tone: "danger", title: "Update", detail: errorMessage(e) });
    } finally {
      setBusyAction(null);
    }
  };
  const check = () => run("check", async () => setUpdate(await api.updateCheck()));
  const download = () => run("download", async () => setUpdate(await api.updateDownload()));
  const install = () => run("install", () => api.updateInstall());
  const later = () => run("later", async () => {
    setUpdate(await api.updateRemindLater());
    setOpen(false);
  });
  const github = () => run("github", () => api.openLink(info ? `release:${info.version}` : "latest"));

  const canDownload = status.installKind !== "portable" && (st.phase === "available" || (st.phase === "failed" && !!st.info));

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent title="Software update" description={`Installed version ${status.currentVersion}`}>
        <div className="flex items-start gap-3">
          <span className="mt-0.5">
            {head.tone === "success" ? <CheckCircle2 size={18} className="text-success" /> : head.tone === "danger" || head.tone === "warning" ? <AlertTriangle size={18} className={head.tone === "danger" ? "text-danger" : "text-warning"} /> : st.phase === "checking" || st.phase === "verifying" ? <Spinner size={18} /> : <Download size={18} className="text-accent" />}
          </span>
          <div className="min-w-0 flex-1">
            <p className="text-[13px] font-semibold" role="status">{head.title}</p>
            {head.detail && <p className="mt-0.5 text-xs text-fg-muted">{head.detail}</p>}
            {st.phase === "failed" && st.detail && (
              <button className="mt-1 text-[11px] text-accent hover:underline" onClick={() => setShowDetail((v) => !v)}>
                {showDetail ? "Hide technical details" : "Show technical details"}
              </button>
            )}
            {st.phase === "failed" && st.detail && showDetail && <pre className="mt-1 whitespace-pre-wrap rounded bg-surface-2 p-2 font-mono text-[11px] text-fg-muted selectable">{st.detail}</pre>}
          </div>
        </div>

        {(st.phase === "downloading" || st.phase === "verifying") && (
          <Progress className="mt-3" value={st.phase === "downloading" && st.total ? (st.received / st.total) * 100 : 0} indeterminate={st.phase === "verifying" || !(st.phase === "downloading" && st.total)} />
        )}

        {info && (
          <div className="mt-4 rounded-md border border-border">
            <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-b border-border bg-surface-2 px-3 py-2 text-xs">
              <span>Current <strong>{status.currentVersion}</strong></span>
              <span>Available <strong>{info.version}</strong></span>
              {info.dateMs && <span className="text-fg-muted">Released {formatDate(info.dateMs, "long")}</span>}
              <Badge tone="success" className="ml-auto"><ShieldCheck size={11} /> Signed release</Badge>
            </div>
            <div className="max-h-64 overflow-auto px-3 py-2">
              {info.notes ? <ReleaseNotes notes={info.notes} /> : <p className="text-xs text-fg-muted">No release notes were published with this version.</p>}
            </div>
          </div>
        )}

        {st.phase === "ready" && blocked && (
          <div className="mt-3 flex items-start gap-2 rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-xs">
            <AlertTriangle size={14} className="mt-0.5 shrink-0 text-warning" />
            <div>
              <p className="font-medium">{busyText(status)} still running</p>
              <p className="mt-0.5 text-fg-muted">Installing closes OST/PST Studio, so it waits until they finish. Nothing is interrupted.</p>
              <button className="mt-1 text-accent hover:underline" onClick={() => { setOpen(false); setPage("activity"); }}>View activity</button>
            </div>
          </div>
        )}

        <DialogFooter>
          {info && (
            <Button variant="ghost" onClick={github} disabled={!!busyAction}>
              <ExternalLink size={14} /> View on GitHub
            </Button>
          )}
          <div className="flex-1" />
          {(st.phase === "available" || st.phase === "ready") && (
            <Button onClick={later} disabled={!!busyAction}>Remind me later</Button>
          )}
          {canDownload && (
            <Button variant="primary" onClick={download} disabled={!!busyAction}>
              {busyAction === "download" ? <Spinner size={14} className="text-accent-fg" /> : <Download size={14} />} Download and install
            </Button>
          )}
          {status.installKind === "portable" && st.phase === "available" && (
            <Button variant="primary" onClick={github} disabled={!!busyAction}>
              <ExternalLink size={14} /> Download from GitHub
            </Button>
          )}
          {st.phase === "ready" && (
            <Button variant="primary" onClick={install} disabled={!!busyAction || blocked} title={blocked ? "Waiting for running work to finish" : undefined}>
              <RotateCw size={14} /> Restart to update
            </Button>
          )}
          {(st.phase === "idle" || st.phase === "upToDate" || (st.phase === "failed" && !st.info)) && (
            <Button variant="primary" onClick={check} disabled={!!busyAction}>
              {busyAction === "check" ? <Spinner size={14} className="text-accent-fg" /> : <RefreshCw size={14} />} Check again
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
