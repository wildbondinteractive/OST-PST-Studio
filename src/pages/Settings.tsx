import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { BadgeCheck, Bug, Code, ExternalLink, FileText, FolderOpen, Info, Scale, Shield, Trash2 } from "lucide-react";
import { api, errorMessage, type AppInfo, type CacheInfo, type Collision, type ExportFormat, type ProjectLink, type Settings } from "@/lib/api";
import { humanBytes } from "@/lib/utils";
import { useApp, type SettingsTab } from "@/lib/store";
import { Button, Card, CardHeader, Input, Kbd, PageHeader, Select, Spinner, Switch, Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui";
import { UpdateSettings, installKindLabel } from "@/components/updates/UpdateSettings";

/// One labelled setting. Defined at module level: a component defined inside the page
/// would be a new component type on every render, which remounts its inputs and throws
/// away focus after each keystroke.
function Row({ title, desc, children, htmlFor }: { title: string; desc?: string; children: React.ReactNode; htmlFor?: string }) {
  return (
    <div className="flex items-center justify-between gap-6 px-4 py-3">
      <div className="min-w-0">
        <label htmlFor={htmlFor} className="text-[13px] font-medium">{title}</label>
        {desc && <div className="mt-0.5 text-xs text-fg-muted">{desc}</div>}
      </div>
      <div className="w-64 shrink-0">{children}</div>
    </div>
  );
}

function SwitchCell({ id, checked, onChange }: { id: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <div className="flex justify-end">
      <Switch id={id} checked={checked} onCheckedChange={onChange} />
    </div>
  );
}

const SHORTCUTS: [string, string][] = [
  ["Ctrl+O", "Open a mailbox"],
  ["Ctrl+F", "Search the open mailbox"],
  ["F5", "Refresh the current view"],
  ["Ctrl+1 to Ctrl+7", "Go to Dashboard, Explorer, Convert, Export, Repair, Batch, Activity"],
  ["Ctrl+,", "Settings"],
  ["Esc", "Close a dialog, or clear the search box"],
];

function LinkButton({ which, icon, children }: { which: ProjectLink; icon: React.ReactNode; children: React.ReactNode }) {
  const toast = useApp((s) => s.toast);
  return (
    <Button size="sm" variant="ghost" className="justify-start" onClick={() => api.openLink(which).catch((e) => toast({ tone: "danger", title: "Could not open the link", detail: errorMessage(e) }))}>
      {icon} {children} <ExternalLink size={11} className="text-fg-subtle" />
    </Button>
  );
}

export function SettingsPage() {
  const settings = useApp((s) => s.settings);
  const saveSettings = useApp((s) => s.saveSettings);
  const loadHistory = useApp((s) => s.loadHistory);
  const toast = useApp((s) => s.toast);
  const tab = useApp((s) => s.settingsTab);
  const openSettings = useApp((s) => s.openSettings);
  const setUpdate = useApp((s) => s.setUpdate);
  const [draft, setDraft] = useState<Settings | null>(settings);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [showLicense, setShowLicense] = useState(false);
  const [cache, setCache] = useState<CacheInfo | null>(null);
  const refreshCache = () => api.scanCacheInfo().then(setCache).catch(() => setCache(null));

  useEffect(() => {
    void refreshCache();
    api.appInfo().then(setInfo).catch(() => {});
    const h = () => {
      void refreshCache();
      api.updateStatus().then(setUpdate).catch(() => {});
    };
    window.addEventListener("ostpst:refresh", h);
    return () => window.removeEventListener("ostpst:refresh", h);
  }, [setUpdate]);
  useEffect(() => setDraft(settings), [settings]);

  if (!draft) {
    return (
      <div className="flex h-full items-center justify-center gap-2 text-xs text-fg-muted">
        <Spinner /> Loading settings
      </div>
    );
  }
  const set = (patch: Partial<Settings>) => {
    const next = { ...draft, ...patch };
    setDraft(next);
    saveSettings(next).catch((e) => toast({ tone: "danger", title: "Could not save settings", detail: errorMessage(e) }));
  };

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <PageHeader title="Settings" description="Preferences are saved on this computer as they change." />
      <Tabs value={tab} onValueChange={(v) => openSettings(v as SettingsTab)} className="flex min-h-0 flex-1 flex-col">
        <div className="shrink-0 px-6">
          <TabsList aria-label="Settings sections">
            <TabsTrigger value="general">General</TabsTrigger>
            <TabsTrigger value="output">Output</TabsTrigger>
            <TabsTrigger value="updates">Software updates</TabsTrigger>
            <TabsTrigger value="privacy">Privacy</TabsTrigger>
            <TabsTrigger value="about">About</TabsTrigger>
          </TabsList>
        </div>
        <div className="min-h-0 flex-1 overflow-auto">
          <div className="flex max-w-3xl flex-col gap-4 px-6 py-4">
            <TabsContent value="general" className="flex flex-col gap-4 data-[state=inactive]:hidden">
              <Card>
                <CardHeader title="Appearance" />
                <div className="divide-y divide-border">
                  <Row title="Theme" desc="Follow Windows, or always use light or dark.">
                    <Select value={draft.theme} onValueChange={(v) => set({ theme: v as Settings["theme"] })} options={[{ value: "system", label: "System" }, { value: "light", label: "Light" }, { value: "dark", label: "Dark" }]} />
                  </Row>
                  <Row title="Messages per page" htmlFor="page-size" desc="How many rows the explorer loads at a time. Larger pages mean fewer loads and more memory.">
                    <Input id="page-size" type="number" min={50} max={2000} step={50} value={draft.messagePageSize} onChange={(e) => set({ messagePageSize: Math.max(50, Math.min(2000, Number(e.target.value) || 200)) })} />
                  </Row>
                </div>
              </Card>
              <Card>
                <CardHeader title="Mailboxes" description="Only file paths and folder positions are remembered, never message contents." />
                <div className="divide-y divide-border">
                  <Row title="Reopen last session" htmlFor="restore-session" desc="When OST/PST Studio starts, reopen the mailboxes that were open when it closed (up to five, and only files that are still there).">
                    <SwitchCell id="restore-session" checked={draft.restoreSession} onChange={(v) => set({ restoreSession: v })} />
                  </Row>
                  <Row title="Return to the last folder" htmlFor="last-folder" desc="Reopening a recent mailbox shows the folder you were last looking at.">
                    <SwitchCell id="last-folder" checked={draft.rememberLastFolder} onChange={(v) => set({ rememberLastFolder: v })} />
                  </Row>
                </div>
              </Card>
              <Card>
                <CardHeader title="Keyboard shortcuts" />
                <dl className="grid grid-cols-[10rem_1fr] gap-x-4 gap-y-1.5 px-4 py-3 text-xs">
                  {SHORTCUTS.map(([k, d]) => (
                    <div key={k} className="contents">
                      <dt><Kbd>{k}</Kbd></dt>
                      <dd className="text-fg-muted">{d}</dd>
                    </div>
                  ))}
                </dl>
              </Card>
            </TabsContent>

            <TabsContent value="output" className="flex flex-col gap-4 data-[state=inactive]:hidden">
              <Card>
                <CardHeader title="Output" />
                <div className="divide-y divide-border">
                  <Row title="Default output folder" htmlFor="out-dir" desc="Suggested for conversions and exports. Leave empty to write beside the source.">
                    <div className="flex gap-1">
                      <Input id="out-dir" value={draft.defaultOutputDir ?? ""} onChange={(e) => set({ defaultOutputDir: e.target.value || null })} placeholder="Beside the source" />
                      <Button size="icon" aria-label="Choose the default output folder" onClick={async () => { const d = await open({ directory: true }); if (typeof d === "string") set({ defaultOutputDir: d }); }}>
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
                  <Row title="Validate after writing" htmlFor="validate" desc="Reopen each written PST and compare it with the source.">
                    <SwitchCell id="validate" checked={draft.validateAfterWrite} onChange={(v) => set({ validateAfterWrite: v })} />
                  </Row>
                  <Row title="Write valid allocation maps" htmlFor="maps" desc="So Outlook opens converted and repaired files without SCANPST.">
                    <SwitchCell id="maps" checked={draft.writeAllocationMaps} onChange={(v) => set({ writeAllocationMaps: v })} />
                  </Row>
                  <Row title="Maximum concurrent jobs" htmlFor="concurrency" desc="For batch operations.">
                    <Input id="concurrency" type="number" min={1} max={8} value={draft.maxConcurrentJobs} onChange={(e) => set({ maxConcurrentJobs: Math.max(1, Math.min(8, Number(e.target.value) || 1)) })} />
                  </Row>
                </div>
              </Card>
              <Card>
                <CardHeader title="Saved scans" description="The node and block index of each opened file, so a large or damaged mailbox reopens without repeating the sweep. Structure only, never message contents. An entry is reused only when the file's path, size, modification time and header checksums all match." />
                <div className="divide-y divide-border">
                  <Row title="Keep saved scans" htmlFor="scan-cache">
                    <SwitchCell id="scan-cache" checked={draft.useScanCache} onChange={(v) => set({ useScanCache: v })} />
                  </Row>
                  <Row title="Cache location" htmlFor="cache-dir" desc={cache ? `${cache.entries} saved scan(s), ${humanBytes(cache.bytes)}` : "Default: the application's cache folder."}>
                    <div className="flex gap-1">
                      <Input id="cache-dir" value={draft.cacheDir ?? ""} onChange={(e) => set({ cacheDir: e.target.value || null })} placeholder={cache?.dir ?? "Application cache folder"} />
                      <Button size="icon" aria-label="Choose the cache folder" onClick={async () => { const d = await open({ directory: true }); if (typeof d === "string") { set({ cacheDir: d }); setTimeout(refreshCache, 300); } }}>
                        <FolderOpen size={14} />
                      </Button>
                    </div>
                  </Row>
                  <Row title="Clear saved scans" desc="Files reopen with a fresh scan afterwards.">
                    <div className="flex justify-end">
                      <Button size="sm" onClick={async () => { try { const n = await api.clearScanCache(); toast({ tone: "success", title: `${n} saved scan(s) removed` }); void refreshCache(); } catch (e) { toast({ tone: "danger", title: "Could not clear the cache", detail: errorMessage(e) }); } }}>
                        <Trash2 size={13} /> Clear
                      </Button>
                    </div>
                  </Row>
                </div>
              </Card>
            </TabsContent>

            <TabsContent value="updates" className="data-[state=inactive]:hidden">
              <UpdateSettings />
            </TabsContent>

            <TabsContent value="privacy" className="flex flex-col gap-4 data-[state=inactive]:hidden">
              <Card>
                <CardHeader title="Privacy" description="Mailboxes are processed on this computer and never leave it. There is no telemetry, tracking or crash reporting." />
                <div className="divide-y divide-border">
                  <Row title="Block remote content in previews" htmlFor="block-remote" desc="Images and other remote resources in HTML bodies are never fetched unless you allow them for a message. Scripts are always disabled.">
                    <SwitchCell id="block-remote" checked={draft.blockRemoteContent} onChange={(v) => set({ blockRemoteContent: v })} />
                  </Row>
                  <Row title="Render HTML bodies" htmlFor="html-bodies" desc="Off shows the plain-text version of every message instead.">
                    <SwitchCell id="html-bodies" checked={draft.showHtmlBodies} onChange={(v) => set({ showHtmlBodies: v })} />
                  </Row>
                  <Row title="Remember recent mailboxes" htmlFor="recent" desc="Paths, sizes and counts only. Turning this off also forgets the current list.">
                    <SwitchCell id="recent" checked={draft.rememberRecentFiles} onChange={(v) => set({ rememberRecentFiles: v })} />
                  </Row>
                  <Row title="Clear history" desc="Recent mailboxes and past job reports.">
                    <div className="flex justify-end">
                      <Button size="sm" onClick={async () => { await api.clearHistory(); await loadHistory(); toast({ tone: "success", title: "History cleared" }); }}>
                        <Trash2 size={13} /> Clear
                      </Button>
                    </div>
                  </Row>
                </div>
              </Card>
              <Card>
                <CardHeader title="Network activity" />
                <div className="px-4 py-3 text-xs text-fg-muted">
                  <p>The application contacts the network in exactly these cases:</p>
                  <ul className="mt-1.5 list-disc space-y-1 pl-4">
                    <li><span className="text-fg">Update checks</span>, to github.com, as set under Software updates ({draft.updateMode === "off" ? "currently off; only when you press Check for updates" : draft.updateMode === "automatic" ? "currently automatic: daily checks and background downloads" : "currently daily checks"}).</li>
                    <li><span className="text-fg">Remote images in one message</span>, only when you press Load remote content on that message.</li>
                    <li><span className="text-fg">Links you click</span> in Settings, which open in your browser.</li>
                  </ul>
                  <p className="mt-2">Browsing, searching, converting, repairing and exporting work fully offline. <button className="text-accent hover:underline" onClick={() => api.openLink("privacy").catch(() => {})}>Privacy policy</button></p>
                </div>
              </Card>
            </TabsContent>

            <TabsContent value="about" className="flex flex-col gap-4 data-[state=inactive]:hidden">
              <Card>
                <div className="flex items-start gap-4 px-4 py-4">
                  <div className="flex h-12 w-12 shrink-0 items-center justify-center rounded-xl bg-accent text-accent-fg">
                    <Shield size={22} />
                  </div>
                  <div className="min-w-0 flex-1 text-xs">
                    <p className="text-base font-semibold">OST/PST Studio</p>
                    <p className="text-fg-muted">A Wildbond Interactive project. Free and open source, MIT licensed.</p>
                    <dl className="mt-3 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 [&>dt]:whitespace-nowrap">
                      <dt className="text-fg-subtle">Version</dt>
                      <dd>{info?.version ?? ""}</dd>
                      <dt className="text-fg-subtle">Engine</dt>
                      <dd>{info?.engineVersion ?? ""}, derived from pstfree (MIT)</dd>
                      <dt className="text-fg-subtle">Installation</dt>
                      <dd>{info ? installKindLabel[info.installKind] : ""}</dd>
                      {info && (
                        <>
                          <dt className="text-fg-subtle">Settings and history</dt>
                          <dd><button className="text-accent hover:underline selectable" onClick={() => void openPath(info.dataDir)}>{info.dataDir}</button></dd>
                        </>
                      )}
                    </dl>
                  </div>
                  <Button size="sm" onClick={() => openSettings("updates")}>Check for updates</Button>
                </div>
              </Card>
              <Card>
                <CardHeader title="Code signing" />
                <div className="flex items-start gap-3 px-4 py-3 text-xs">
                  <BadgeCheck size={16} className="mt-0.5 shrink-0 text-success" />
                  <div className="text-fg-muted">
                    <p>
                      Official releases from 0.1.2 onwards are digitally signed with Microsoft Azure Artifact Signing. The verified publisher shown by Windows is <span className="font-medium text-fg">Jacob Adlon</span>, the project's maintainer, whose identity Microsoft verified. Wildbond Interactive is the project's name, not the certificate holder.
                    </p>
                    <p className="mt-1.5">Updates delivered inside the application carry a second signature, checked against a key built into this program before anything is installed. Copies you build yourself are not signed.</p>
                    <button className="mt-1.5 text-accent hover:underline" onClick={() => api.openLink("signing").catch(() => {})}>Code signing policy</button>
                  </div>
                </div>
              </Card>
              <Card>
                <CardHeader title="Project" />
                <div className="grid grid-cols-2 gap-1 px-3 py-2">
                  <LinkButton which="repository" icon={<Code size={13} />}>Source code on GitHub</LinkButton>
                  <LinkButton which="releases" icon={<FileText size={13} />}>Releases and release notes</LinkButton>
                  <LinkButton which="issues" icon={<Bug size={13} />}>Report a problem</LinkButton>
                  <LinkButton which="notices" icon={<Scale size={13} />}>Third-party notices</LinkButton>
                </div>
                <div className="border-t border-border px-4 py-3">
                  <Button size="sm" variant="ghost" onClick={() => setShowLicense((v) => !v)} aria-expanded={showLicense}>
                    <Info size={13} /> {showLicense ? "Hide" : "Show"} license and components
                  </Button>
                  {showLicense && info && (
                    <div className="mt-3 rounded-md bg-surface-2 p-3 text-xs selectable">
                      <pre className="whitespace-pre-wrap font-sans text-[11px]">{info.license}</pre>
                      <div className="mt-2 font-semibold">Main third-party components</div>
                      <ul className="mt-1 text-[11px] text-fg-muted">
                        {info.thirdParty.map(([name, lic, url]) => (
                          <li key={name}>{name}: {lic} ({url})</li>
                        ))}
                      </ul>
                      <p className="mt-2 text-[11px] text-fg-muted">The complete list, with every library's license, is THIRD_PARTY_NOTICES.md in the source repository.</p>
                    </div>
                  )}
                </div>
              </Card>
            </TabsContent>
          </div>
        </div>
      </Tabs>
    </div>
  );
}
