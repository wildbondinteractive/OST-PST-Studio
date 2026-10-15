// Settings > Software updates.

import { useState } from "react";
import { AlertTriangle, ArrowUpCircle, CheckCircle2, ExternalLink, RefreshCw } from "lucide-react";
import { api, errorMessage, updateInfoOf, type InstallKind, type UpdateMode } from "@/lib/api";
import { useApp } from "@/lib/store";
import { cn, formatDate } from "@/lib/utils";
import { Button, Card, CardHeader, Spinner } from "@/components/ui";
import { updateHeadline } from "./UpdateDialog";

export const installKindLabel: Record<InstallKind, string> = {
  nsis: "Installed with the setup program (per user)",
  msi: "Installed with the MSI package",
  portable: "Portable copy (not installed)",
};

const MODES: { value: UpdateMode; title: string; desc: string }[] = [
  {
    value: "notify",
    title: "Notify me",
    desc: "Check once a day and tell me when a new version is out. Nothing is downloaded until I choose to.",
  },
  {
    value: "automatic",
    title: "Automatic",
    desc: "Check once a day and download new versions in the background. The update installs when I close OST/PST Studio, or when I choose to restart. Never while a job is running.",
  },
  {
    value: "off",
    title: "Off",
    desc: "Never contact GitHub on my behalf. I can still press Check for updates.",
  },
];

/// A keyboard-friendly radio group of cards: arrow keys move and select, as in Windows.
function ModeChooser({ value, onChange, portable }: { value: UpdateMode; onChange: (v: UpdateMode) => void; portable: boolean }) {
  const onKey = (e: React.KeyboardEvent, i: number) => {
    const d = e.key === "ArrowDown" || e.key === "ArrowRight" ? 1 : e.key === "ArrowUp" || e.key === "ArrowLeft" ? -1 : 0;
    if (!d) return;
    e.preventDefault();
    const next = MODES[(i + d + MODES.length) % MODES.length];
    onChange(next.value);
    const el = (e.currentTarget.parentElement?.children[(i + d + MODES.length) % MODES.length] as HTMLElement | undefined);
    el?.focus();
  };
  return (
    <div role="radiogroup" aria-label="Update mode" className="grid gap-2">
      {MODES.map((m, i) => {
        const checked = m.value === value;
        return (
          <button
            key={m.value}
            type="button"
            role="radio"
            aria-checked={checked}
            tabIndex={checked ? 0 : -1}
            onClick={() => onChange(m.value)}
            onKeyDown={(e) => onKey(e, i)}
            className={cn(
              "flex items-start gap-3 rounded-md border px-3 py-2.5 text-left transition-colors",
              checked ? "border-accent bg-accent-soft" : "border-border hover:bg-surface-2",
            )}
          >
            <span className={cn("mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded-full border", checked ? "border-accent" : "border-border-strong")}>
              {checked && <span className="h-2 w-2 rounded-full bg-accent" />}
            </span>
            <span>
              <span className="block text-[13px] font-medium">
                {m.title}
                {m.value === "notify" && <span className="ml-1.5 text-[11px] font-normal text-fg-subtle">default</span>}
              </span>
              <span className="mt-0.5 block text-xs text-fg-muted">
                {m.desc}
                {portable && m.value === "automatic" && " A portable copy is only notified; it never installs anything."}
              </span>
            </span>
          </button>
        );
      })}
    </div>
  );
}

export function UpdateSettings() {
  const status = useApp((s) => s.update);
  const setUpdate = useApp((s) => s.setUpdate);
  const settings = useApp((s) => s.settings);
  const saveSettings = useApp((s) => s.saveSettings);
  const openDialog = useApp((s) => s.setUpdateDialogOpen);
  const toast = useApp((s) => s.toast);
  const [checking, setChecking] = useState(false);

  if (!status || !settings) return null;
  const info = updateInfoOf(status);
  const head = updateHeadline(status);
  const st = status.state;

  const checkNow = async () => {
    setChecking(true);
    try {
      const s = await api.updateCheck();
      setUpdate(s);
      if (s.state.phase === "upToDate") toast({ tone: "success", title: "You're running the latest version of OST/PST Studio." });
      else if (s.state.phase === "available") openDialog(true);
      else if (s.state.phase === "failed") toast({ tone: s.state.kind === "signature" ? "danger" : "warning", title: "Could not check for updates", detail: s.state.message });
    } catch (e) {
      toast({ tone: "danger", title: "Could not check for updates", detail: errorMessage(e) });
    } finally {
      setChecking(false);
    }
  };

  const setMode = (updateMode: UpdateMode) => {
    saveSettings({ ...settings, updateMode })
      .then(() => api.updateStatus().then(setUpdate))
      .catch((e) => toast({ tone: "danger", title: "Could not save settings", detail: errorMessage(e) }));
  };

  const icon =
    st.phase === "checking" || checking ? <Spinner size={16} /> :
    head.tone === "success" ? <CheckCircle2 size={16} className="text-success" /> :
    head.tone === "warning" || head.tone === "danger" ? <AlertTriangle size={16} className={head.tone === "danger" ? "text-danger" : "text-warning"} /> :
    info ? <ArrowUpCircle size={16} className="text-accent" /> : <RefreshCw size={16} className="text-fg-subtle" />;

  return (
    <div className="flex flex-col gap-4">
      <Card>
        <CardHeader title="Software updates" description="New versions come only from the project's published GitHub releases, signed and verified before anything is installed." />
        <div className="flex items-start gap-3 px-4 py-3">
          <span className="mt-0.5">{icon}</span>
          <div className="min-w-0 flex-1">
            <p className="text-[13px] font-medium" role="status">{checking ? "Checking for updates" : head.title}</p>
            {head.detail && !checking && <p className="mt-0.5 text-xs text-fg-muted">{head.detail}</p>}
            <dl className="mt-2 grid grid-cols-[auto_1fr] gap-x-4 gap-y-0.5 text-xs">
              <dt className="text-fg-subtle">Installed version</dt>
              <dd>{status.currentVersion}</dd>
              <dt className="text-fg-subtle">Installation</dt>
              <dd>{installKindLabel[status.installKind]}</dd>
              <dt className="text-fg-subtle">Last successful check</dt>
              <dd>{status.lastSuccessMs ? formatDate(status.lastSuccessMs, "long") : "Never"}</dd>
              {status.lastCheckMs && status.lastCheckMs !== status.lastSuccessMs && (
                <>
                  <dt className="text-fg-subtle">Last attempt</dt>
                  <dd>{formatDate(status.lastCheckMs, "long")} (did not complete)</dd>
                </>
              )}
            </dl>
          </div>
          <div className="flex shrink-0 flex-col items-end gap-2">
            <Button size="sm" variant={info ? "secondary" : "primary"} onClick={checkNow} disabled={checking || st.phase === "checking" || st.phase === "downloading" || st.phase === "verifying" || st.phase === "installing"}>
              <RefreshCw size={13} /> Check for updates
            </Button>
            {info && (
              <Button size="sm" variant="primary" onClick={() => openDialog(true)}>
                {st.phase === "ready" ? "Restart options" : "View update"}
              </Button>
            )}
            <Button size="sm" variant="link" onClick={() => api.openLink("releases").catch(() => {})}>
              <ExternalLink size={12} /> All releases
            </Button>
          </div>
        </div>
      </Card>

      <Card>
        <CardHeader title="When a new version is released" />
        <div className="px-4 py-3">
          <ModeChooser value={settings.updateMode} onChange={setMode} portable={status.installKind === "portable"} />
          <p className="mt-3 text-xs text-fg-muted">
            A check requests one small file from github.com; a download fetches the installer from the same release. Neither request carries anything about your mailboxes, files or use of the application, and there is no telemetry.
            {status.installKind === "msi" && " MSI installs update with the MSI package, so Windows asks for permission as it did when the application was first installed."}
          </p>
        </div>
      </Card>
    </div>
  );
}
