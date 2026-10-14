import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Layers, Play, Plus, Trash2, X } from "lucide-react";
import { api, errorMessage, type Collision, type ExportFormat, type JobKind, type ProbeResult } from "@/lib/api";
import { useApp } from "@/lib/store";
import { cn, humanBytes } from "@/lib/utils";
import { Badge, Button, Card, Checkbox, EmptyState, Field, Input, PageHeader, Select } from "@/components/ui";
import { OutputField } from "@/components/jobs/FileFields";
import { JobCard } from "@/components/jobs/JobViews";

interface QueueItem {
  path: string;
  probe: ProbeResult | null;
  jobId: string | null;
}

export function BatchPage() {
  const settings = useApp((s) => s.settings);
  const jobs = useApp((s) => s.jobs);
  const toast = useApp((s) => s.toast);
  const upsertJob = useApp((s) => s.upsertJob);
  const saveSettings = useApp((s) => s.saveSettings);

  const [items, setItems] = useState<QueueItem[]>([]);
  const [operation, setOperation] = useState<Exclude<JobKind, "extractAttachments">>("convert");
  const [format, setFormat] = useState<ExportFormat>("eml");
  const [outputDir, setOutputDir] = useState("");
  const [collision, setCollision] = useState<Collision>(settings?.collision ?? "rename");
  const [validate, setValidate] = useState(true);
  const [maps, setMaps] = useState(true);
  const [salvage, setSalvage] = useState(false);
  const [concurrency, setConcurrency] = useState(settings?.maxConcurrentJobs ?? 2);

  useEffect(() => {
    if (settings) {
      setCollision(settings.collision);
      setConcurrency(settings.maxConcurrentJobs);
      setValidate(settings.validateAfterWrite);
      setMaps(settings.writeAllocationMaps);
      if (settings.defaultOutputDir && !outputDir) setOutputDir(settings.defaultOutputDir);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settings]);

  const addFiles = async () => {
    const picked = await open({ multiple: true, filters: [{ name: "Outlook data files", extensions: ["pst", "ost"] }] });
    if (!picked) return;
    const paths = (Array.isArray(picked) ? picked : [picked]).filter((p) => !items.some((i) => i.path === p));
    const probes = await Promise.all(paths.map((p) => api.probeFile(p).catch(() => null)));
    setItems((s) => [...s, ...paths.map((path, i) => ({ path, probe: probes[i], jobId: null }))]);
  };

  const runAll = async () => {
    if (concurrency !== settings?.maxConcurrentJobs && settings) {
      await saveSettings({ ...settings, maxConcurrentJobs: concurrency }).catch(() => {});
    }
    const next = items.slice();
    for (let i = 0; i < next.length; i++) {
      const it = next[i];
      if (it.jobId || !it.probe?.isPstOrOst) continue;
      const stem = it.probe.fileName.replace(/\.[^.]+$/, "");
      try {
        let id: string;
        if (operation === "verify") {
          id = await api.startJob({ kind: "verify", source: it.path });
        } else if (operation === "export") {
          id = await api.startJob({
            kind: "export",
            options: { source: it.path, destination: `${outputDir}\\${stem}`, format, selection: { entireMailbox: true, folders: [], messages: [] }, salvage, collision, extractAttachments: true },
          });
        } else {
          id = await api.startJob({
            kind: operation,
            options: { source: it.path, destination: `${outputDir}\\${stem}.pst`, salvage, collision, validate, writeAllocationMaps: maps },
          });
        }
        next[i] = { ...it, jobId: id };
        const rec = await api.getJob(id);
        if (rec) upsertJob(rec);
      } catch (e) {
        toast({ tone: "danger", title: `Could not queue ${it.probe.fileName}`, detail: errorMessage(e) });
      }
    }
    setItems(next);
  };

  const queued = items.filter((i) => i.jobId);
  const batchJobs = useMemo(() => queued.map((i) => jobs.find((j) => j.id === i.jobId)).filter(Boolean), [queued, jobs]);
  const done = batchJobs.filter((j) => j && !["queued", "running"].includes(j.status)).length;
  const failed = batchJobs.filter((j) => j && j.status === "failed").length;
  const overall = batchJobs.length ? (batchJobs.reduce((a, j) => a + (j ? (["queued", "running"].includes(j.status) ? (j.progress.total ? j.progress.done / j.progress.total : 0) : 1) : 0), 0) / batchJobs.length) * 100 : 0;
  const needsDir = operation !== "verify";
  const ready = items.some((i) => !i.jobId && i.probe?.isPstOrOst) && (!needsDir || outputDir.trim());

  return (
    <div className="flex h-full flex-col overflow-auto">
      <PageHeader
        title="Batch Operations"
        description="Queue several OST or PST files and run the same operation on each. A failure in one file never stops the others."
        actions={<Button onClick={addFiles}><Plus size={14} /> Add files</Button>}
      />
      <div className="grid gap-4 px-6 pb-6 xl:grid-cols-[1fr_380px]">
        <Card className="flex flex-col">
          <div className="border-b border-border px-4 py-2 text-[13px] font-semibold">Queue ({items.length})</div>
          {items.length === 0 ? (
            <EmptyState compact icon={<Layers size={20} />} title="No files queued" description="Add OST or PST files. Each becomes its own job with its own report." action={<Button onClick={addFiles}><Plus size={14} /> Add files</Button>} />
          ) : (
            <div className="divide-y divide-border">
              {items.map((it) => {
                const job = it.jobId ? jobs.find((j) => j.id === it.jobId) : null;
                return (
                  <div key={it.path} className="flex items-center gap-3 px-4 py-2">
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-[13px]">{it.probe?.fileName ?? it.path}</div>
                      <div className="truncate text-xs text-fg-subtle">
                        {it.probe?.isPstOrOst ? `${it.probe.isOst ? "OST" : "PST"} · ${humanBytes(it.probe.sizeBytes)}` : <span className="text-danger">{it.probe?.error ?? "not a PST or OST"}</span>}
                      </div>
                    </div>
                    {job ? (
                      <div className="w-56">
                        <div className="flex items-center justify-between text-[11px]">
                          <Badge tone={job.status === "completed" ? "success" : job.status === "failed" ? "danger" : job.status === "completedWithWarnings" ? "warning" : job.status === "running" ? "accent" : "neutral"}>{job.status}</Badge>
                          <span className="text-fg-subtle">{job.status === "running" ? job.progress.phase : ""}</span>
                        </div>
                        <div className="mt-1 h-1 w-full overflow-hidden rounded-full bg-surface-3">
                          <div className={cn("h-full rounded-full", job.status === "failed" ? "bg-danger" : "bg-accent")} style={{ width: `${["queued", "running"].includes(job.status) ? (job.progress.total ? (job.progress.done / job.progress.total) * 100 : 0) : 100}%` }} />
                        </div>
                      </div>
                    ) : (
                      <Button size="iconSm" variant="ghost" onClick={() => setItems((s) => s.filter((x) => x.path !== it.path))} aria-label="Remove">
                        <X size={13} />
                      </Button>
                    )}
                  </div>
                );
              })}
            </div>
          )}
          {batchJobs.length > 0 && (
            <div className="border-t border-border px-4 py-3 text-xs">
              <div className="flex items-center justify-between">
                <span>Overall: {done} of {batchJobs.length} finished{failed ? `, ${failed} failed` : ""}</span>
                <Button size="sm" variant="ghost" onClick={() => setItems((s) => s.filter((i) => !i.jobId || ["queued", "running"].includes(jobs.find((j) => j.id === i.jobId)?.status ?? "")))}>
                  <Trash2 size={12} /> Clear finished
                </Button>
              </div>
              <div className="mt-1.5 h-1.5 w-full overflow-hidden rounded-full bg-surface-3">
                <div className="h-full rounded-full bg-accent transition-[width]" style={{ width: `${overall}%` }} />
              </div>
            </div>
          )}
        </Card>

        <Card className="p-5">
          <div className="flex flex-col gap-4">
            <Field label="Operation">
              <Select
                value={operation}
                onValueChange={(v) => setOperation(v as typeof operation)}
                options={[
                  { value: "convert", label: "Convert to PST", description: "OST or ANSI PST to a validated Unicode PST" },
                  { value: "repair", label: "Repair to a new PST", description: "Rebuild each file into a clean copy" },
                  { value: "export", label: "Export entire mailbox", description: "One subfolder per file" },
                  { value: "verify", label: "Verify only", description: "Writes nothing" },
                ]}
              />
            </Field>
            {operation === "export" && (
              <Field label="Export format">
                <Select<ExportFormat>
                  value={format}
                  onValueChange={setFormat}
                  options={[
                    { value: "eml", label: "EML" }, { value: "msg", label: "MSG" }, { value: "mbox", label: "MBOX" }, { value: "html", label: "HTML" },
                    { value: "txt", label: "Plain text" }, { value: "csv", label: "CSV" }, { value: "json", label: "JSON" },
                  ]}
                />
              </Field>
            )}
            {needsDir && <OutputField value={outputDir} onChange={setOutputDir} kind="dir" label="Output folder" hint={operation === "export" ? "A subfolder named after each file is created here." : "Each PST is written here with the source file's name."} />}
            <Field label="If an output already exists">
              <Select<Collision> value={collision} onValueChange={setCollision} options={[{ value: "rename", label: "Write beside it with a number" }, { value: "fail", label: "Stop that file and report" }, { value: "overwrite", label: "Replace it" }]} />
            </Field>
            <Field label="Concurrent jobs" hint="How many files are processed at once. Disk speed usually matters more than CPU here.">
              <Input type="number" min={1} max={8} value={concurrency} onChange={(e) => setConcurrency(Math.max(1, Math.min(8, Number(e.target.value) || 1)))} />
            </Field>
            {(operation === "convert" || operation === "repair") && (
              <div className="flex flex-col gap-2 text-xs">
                <label className="flex items-center gap-2"><Checkbox checked={validate} onCheckedChange={setValidate} /> Validate each output after writing</label>
                <label className="flex items-center gap-2"><Checkbox checked={maps} onCheckedChange={setMaps} /> Write valid allocation maps</label>
              </div>
            )}
            <label className="flex items-center gap-2 text-xs"><Checkbox checked={salvage} onCheckedChange={setSalvage} /> Salvage mode for every file</label>
            <Button variant="primary" disabled={!ready} onClick={runAll}><Play size={14} /> Run batch</Button>
          </div>
        </Card>
      </div>
      {batchJobs.length > 0 && (
        <div className="flex flex-col gap-2 px-6 pb-6">
          <div className="text-[13px] font-semibold">Job details</div>
          {batchJobs.map((j) => j && <JobCard key={j.id} job={j} />)}
        </div>
      )}
    </div>
  );
}
