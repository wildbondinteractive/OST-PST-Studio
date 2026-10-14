import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { FolderOpen, Info, Shield, Trash2 } from "lucide-react";
import { api, errorMessage, type AppInfo, type Collision, type ExportFormat, type Settings } from "@/lib/api";
import { useApp } from "@/lib/store";
import { Button, Card, CardHeader, Input, PageHeader, Select, Switch } from "@/components/ui";

export function SettingsPage() {
  const settings = useApp((s) => s.settings);
  const saveSettings = useApp((s) => s.saveSettings);
  const loadHistory = useApp((s) => s.loadHistory);
  const toast = useApp((s) => s.toast);
  const [draft, setDraft] = useState<Settings | null>(settings);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [showLicense, setShowLicense] = useState(false);

  useEffect(() => setDraft(settings), [settings]);
  useEffect(() => {
    api.appInfo().then(setInfo).catch(() => {});
  }, []);

  if (!draft) return null;
  const set = (patch: Partial<Settings>) => {
    const next = { ...draft, ...patch };
    setDraft(next);
    saveSettings(next).catch((e) => toast({ tone: "danger", title: "Could not save settings", detail: errorMessage(e) }));
  };

  const Row = ({ title, desc, children }: { title: string; desc?: string; children: React.ReactNode }) => (
    <div className="flex items-center justify-between gap-6 px-4 py-3">
      <div className="min-w-0">
        <div className="text-[13px] font-medium">{title}</div>
        {desc && <div className="mt-0.5 text-xs text-fg-muted">{desc}</div>}
      </div>
      <div className="w-64 shrink-0">{children}</div>
    </div>
  );

  return (
    <div className="flex h-full flex-col overflow-auto">
      <PageHeader title="Settings" description="Preferences are saved locally as they change." />
      <div className="flex max-w-3xl flex-col gap-4 px-6 pb-6">
        <Card>
          <CardHeader title="Appearance" />
          <div className="divide-y divide-border">
            <Row title="Theme" desc="Follow Windows, or force light or dark.">
              <Select value={draft.theme} onValueChange={(v) => set({ theme: v as Settings["theme"] })} options={[{ value: "system", label: "System" }, { value: "light", label: "Light" }, { value: "dark", label: "Dark" }]} />
            </Row>
            <Row title="Messages per page" desc="How many rows the explorer loads at a time. Larger pages mean fewer loads and more memory.">
              <Input type="number" min={50} max={2000} step={50} value={draft.messagePageSize} onChange={(e) => set({ messagePageSize: Math.max(50, Math.min(2000, Number(e.target.value) || 200)) })} />
            </Row>
          </div>
        </Card>

        <Card>
          <CardHeader title="Output" />
          <div className="divide-y divide-border">
            <Row title="Default output folder" desc="Suggested for conversions and exports. Leave empty to write beside the source.">
              <div className="flex gap-1">
                <Input value={draft.defaultOutputDir ?? ""} onChange={(e) => set({ defaultOutputDir: e.target.value || null })} placeholder="Beside the source" />
                <Button size="icon" aria-label="Browse" onClick={async () => { const d = await open({ directory: true }); if (typeof d === "string") set({ defaultOutputDir: d }); }}>
                  <FolderOpen size={14} />
                </Button>
              </div>
            </Row>
            <Row title="Default export format">
              <Select<ExportFormat>
                value={draft.defaultExportFormat}
                onValueChange={(v) => set({ defaultExportFormat: v })}
                options={[{ value: "eml", label: "EML" }, { value: "msg", label: "MSG" }, { value: "mbox", label: "MBOX" }, { value: "html", label: "HTML" }, { value: "txt", label: "Plain text" }, { value: "csv", label: "CSV" }, { value: "json", label: "JSON" }]}
              />
            </Row>
            <Row title="When an output file already exists">
              <Select<Collision> value={draft.collision} onValueChange={(v) => set({ collision: v })} options={[{ value: "rename", label: "Write beside it with a number" }, { value: "fail", label: "Stop and report" }, { value: "overwrite", label: "Replace it" }]} />
            </Row>
            <Row title="Validate after writing" desc="Reopen each written PST and compare it with the source.">
              <div className="flex justify-end"><Switch checked={draft.validateAfterWrite} onCheckedChange={(v) => set({ validateAfterWrite: v })} /></div>
            </Row>
            <Row title="Write valid allocation maps" desc="So Outlook opens converted and repaired files without SCANPST.">
              <div className="flex justify-end"><Switch checked={draft.writeAllocationMaps} onCheckedChange={(v) => set({ writeAllocationMaps: v })} /></div>
            </Row>
            <Row title="Maximum concurrent jobs" desc="For batch operations.">
              <Input type="number" min={1} max={8} value={draft.maxConcurrentJobs} onChange={(e) => set({ maxConcurrentJobs: Math.max(1, Math.min(8, Number(e.target.value) || 1)) })} />
            </Row>
          </div>
        </Card>

        <Card>
          <CardHeader title="Privacy" description="OST/PST Studio works fully offline. There is no telemetry and no update check unless you turn one on." />
          <div className="divide-y divide-border">
            <Row title="Block remote content in previews" desc="Images and other remote resources in HTML bodies are never fetched unless you allow them for a message. Scripts are always disabled.">
              <div className="flex justify-end"><Switch checked={draft.blockRemoteContent} onCheckedChange={(v) => set({ blockRemoteContent: v })} /></div>
            </Row>
            <Row title="Render HTML bodies" desc="Off shows the plain-text version of every message instead.">
              <div className="flex justify-end"><Switch checked={draft.showHtmlBodies} onCheckedChange={(v) => set({ showHtmlBodies: v })} /></div>
            </Row>
            <Row title="Remember recent files" desc="Paths and counts only; never message contents.">
              <div className="flex justify-end"><Switch checked={draft.rememberRecentFiles} onCheckedChange={(v) => set({ rememberRecentFiles: v })} /></div>
            </Row>
            <Row title="Check for updates" desc="Not implemented yet; this stays off and nothing is contacted.">
              <div className="flex justify-end"><Switch checked={false} disabled onCheckedChange={() => {}} /></div>
            </Row>
            <Row title="Clear history" desc="Recent files and past job reports.">
              <div className="flex justify-end">
                <Button size="sm" onClick={async () => { await api.clearHistory(); await loadHistory(); toast({ tone: "success", title: "History cleared" }); }}>
                  <Trash2 size={13} /> Clear
                </Button>
              </div>
            </Row>
          </div>
        </Card>

        <Card>
          <CardHeader title="About" />
          <div className="px-4 py-3 text-xs">
            <div className="flex items-center gap-2 text-[13px] font-semibold"><Shield size={14} className="text-accent" /> OST/PST Studio {info?.version ?? ""}</div>
            <p className="mt-1 text-fg-muted">Engine {info?.engineVersion ?? ""}, based on pstfree (MIT). Reads, converts, repairs and exports Outlook OST and PST files without Outlook. MIT licensed; the source is free to use and build on.</p>
            {info && (
              <p className="mt-1 text-fg-subtle">
                Settings and history: <button className="text-accent hover:underline selectable" onClick={() => void openPath(info.dataDir)}>{info.dataDir}</button>
              </p>
            )}
            <div className="mt-3 flex gap-2">
              <Button size="sm" variant="ghost" onClick={() => setShowLicense((v) => !v)}><Info size={13} /> License and third-party notices</Button>
            </div>
            {showLicense && info && (
              <div className="mt-3 rounded-md bg-surface-2 p-3 selectable">
                <pre className="whitespace-pre-wrap font-sans text-[11px]">{info.license}</pre>
                <div className="mt-2 font-semibold">Third-party components</div>
                <ul className="mt-1 text-[11px] text-fg-muted">
                  {info.thirdParty.map(([name, lic, url]) => (
                    <li key={name}>{name}: {lic} ({url})</li>
                  ))}
                </ul>
              </div>
            )}
          </div>
        </Card>
      </div>
    </div>
  );
}
