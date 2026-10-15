import { useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { AlertTriangle, CheckCircle2, ChevronDown, ChevronRight, Download, ExternalLink, RotateCcw, ShieldCheck, ShieldAlert, XCircle, Ban, Clock } from "lucide-react";
import { api, errorMessage, type JobRecord, type JobReport } from "@/lib/api";
import { useApp } from "@/lib/store";
import { cn, formatDate, formatDuration, humanBytes, jobKindLabel, jobStatusLabel } from "@/lib/utils";
import { Badge, Button, Progress } from "@/components/ui";

export function statusTone(s: JobRecord["status"]): "success" | "warning" | "danger" | "neutral" | "accent" {
  switch (s) {
    case "completed": return "success";
    case "completedWithWarnings": return "warning";
    case "failed": return "danger";
    case "running": return "accent";
    default: return "neutral";
  }
}

export function JobProgressView({ job, compact }: { job: JobRecord; compact?: boolean }) {
  const p = job.progress;
  const pct = p.total > 0 ? (p.done / p.total) * 100 : 0;
  const live = job.status === "running";
  return (
    <div>
      <div className="flex items-center justify-between gap-3 text-xs">
        <span className="truncate text-fg-muted">{job.status === "queued" ? "Waiting for a free slot" : p.phase || "Starting"}</span>
        <span className="shrink-0 tabular-nums text-fg-muted">
          {p.determinate && p.total > 0 ? `${p.done.toLocaleString()} / ${p.total.toLocaleString()}` : p.done > 0 ? p.done.toLocaleString() : ""}
        </span>
      </div>
      <Progress className="mt-1.5" value={pct} indeterminate={live && !p.determinate} tone={job.status === "failed" ? "danger" : "accent"} />
      {!compact && (
        <div className="mt-1.5 flex flex-wrap gap-x-4 gap-y-0.5 text-[11px] text-fg-subtle">
          <span>Elapsed {formatDuration(p.elapsedMs)}</span>
          {p.etaMs != null && live && <span>About {formatDuration(p.etaMs)} left</span>}
          {p.bytesWritten > 0 && <span>{humanBytes(p.bytesWritten)} written</span>}
        </div>
      )}
    </div>
  );
}

export function JobCard({ job, defaultOpen }: { job: JobRecord; defaultOpen?: boolean }) {
  const [open, setOpen] = useState(!!defaultOpen);
  const toast = useApp((s) => s.toast);
  const upsert = useApp((s) => s.upsertJob);
  const active = job.status === "running" || job.status === "queued";
  const name = job.source.split(/[\\/]/).pop() ?? job.source;
  return (
    <div className="rounded-lg border border-border bg-surface">
      <div className="flex items-start gap-3 px-4 py-3">
        <button className="mt-0.5 text-fg-subtle hover:text-fg" onClick={() => setOpen((v) => !v)} aria-label={open ? "Collapse" : "Expand"}>
          {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        </button>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-[13px] font-semibold">{jobKindLabel[job.kind]}</span>
            <span className="truncate text-xs text-fg-muted" title={job.source}>{name}</span>
            <Badge tone={statusTone(job.status)}>{jobStatusLabel[job.status]}</Badge>
            {job.report && <ValidationBadge report={job.report} />}
          </div>
          {active ? (
            <div className="mt-2"><JobProgressView job={job} /></div>
          ) : (
            <div className="mt-0.5 text-[11px] text-fg-subtle">
              {job.finishedMs ? formatDate(job.finishedMs, "long") : ""}
              {job.report ? ` · ${formatDuration(job.report.durationMs)}` : ""}
              {job.destination && ` · ${job.destination}`}
            </div>
          )}
          {job.error && !active && <div className="mt-1 text-xs text-danger selectable">{job.error}</div>}
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {active ? (
            <Button size="sm" variant="ghost" onClick={() => void api.cancelJob(job.id)}>
              <Ban size={13} /> Cancel
            </Button>
          ) : (
            <>
              {job.destination && (job.status === "completed" || job.status === "completedWithWarnings") && (
                <Button size="sm" variant="ghost" onClick={() => void revealItemInDir(job.destination).catch((e) => toast({ tone: "warning", title: "Could not open Explorer", detail: errorMessage(e) }))}>
                  <ExternalLink size={13} /> Show
                </Button>
              )}
              {(job.status === "failed" || job.status === "cancelled") && (
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={async () => {
                    try {
                      const id = await api.retryJob(job.id);
                      const rec = await api.getJob(id);
                      if (rec) upsert(rec);
                    } catch (e) {
                      toast({ tone: "danger", title: "Could not retry", detail: errorMessage(e) });
                    }
                  }}
                >
                  <RotateCcw size={13} /> Retry
                </Button>
              )}
            </>
          )}
        </div>
      </div>
      {open && job.report && (
        <div className="border-t border-border px-4 py-3">
          <ReportView report={job.report} jobId={job.id} />
        </div>
      )}
    </div>
  );
}

export function ValidationBadge({ report }: { report: JobReport }) {
  const v = report.validation.status;
  if (v === "notRun") return null;
  return (
    <Badge tone={v === "passed" ? "success" : v === "failed" ? "danger" : "warning"}>
      {v === "passed" ? <ShieldCheck size={11} /> : <ShieldAlert size={11} />}
      {v === "passed" ? "validated" : v === "failed" ? "validation failed" : "validated with warnings"}
    </Badge>
  );
}

export function ReportView({ report, jobId }: { report: JobReport; jobId?: string }) {
  const toast = useApp((s) => s.toast);
  const saveAs = async (format: "json" | "html" | "txt") => {
    const base = (report.destination.path || report.source.path).split(/[\\/]/).pop()?.replace(/\.[^.]+$/, "") ?? "report";
    const path = await save({ defaultPath: `${base}-report.${format}`, filters: [{ name: format.toUpperCase(), extensions: [format] }] });
    if (!path) return;
    try {
      if (jobId) await api.saveReport(jobId, path, format);
      toast({ tone: "success", title: "Report saved", detail: path });
    } catch (e) {
      toast({ tone: "danger", title: "Could not save the report", detail: errorMessage(e) });
    }
  };
  const outlook = {
    unverified: ["neutral", "Outlook compatibility not verified"],
    requiresScanpst: ["warning", "Outlook will ask for SCANPST before opening this file (allocation maps marked invalid)"],
    mapsWritten: ["success", "Written with valid allocation maps; files written this way open directly in classic Outlook in testing"],
    verifiedManually: ["success", "Confirmed in Outlook manually"],
  }[report.outlook] as [Parameters<typeof Badge>[0]["tone"], string];
  const Stat = ({ label, value }: { label: string; value: React.ReactNode }) => (
    <div className="min-w-0 rounded-md bg-surface-2 px-3 py-2">
      <div className="truncate text-[10px] uppercase tracking-wide text-fg-subtle">{label}</div>
      <div className="truncate text-[13px] font-semibold tabular-nums">{value}</div>
    </div>
  );
  return (
    <div className="flex flex-col gap-3 text-xs">
      <div className="grid grid-cols-2 gap-2 sm:grid-cols-3 xl:grid-cols-6">
        <Stat label="Source" value={humanBytes(report.source.sizeBytes)} />
        {report.destination.path && <Stat label="Output" value={report.kind === "export" ? `${report.filesWritten} files` : humanBytes(report.destination.sizeBytes)} />}
        <Stat label="Folders" value={report.kind === "verify" ? report.sourceCounts.folders : `${report.outputCounts.folders} / ${report.sourceCounts.folders}`} />
        <Stat label="Messages" value={report.kind === "verify" ? report.sourceCounts.messages : `${report.outputCounts.messages} / ${report.sourceCounts.messages}`} />
        <Stat label="Attachments" value={report.outputCounts.attachments} />
        <Stat label="Duration" value={formatDuration(report.durationMs)} />
      </div>
      <div className="flex flex-wrap items-center gap-2">
        {report.sourceHealth && <Badge tone="neutral">source {report.sourceHealth}</Badge>}
        {report.indexSource === "recovered" && <Badge tone="warning">index recovered by sweeping</Badge>}
        {report.droppedNodes > 0 && <Badge tone="warning">{report.droppedNodes} node(s) left out</Badge>}
        {report.kind !== "export" && report.kind !== "verify" && <Badge tone={outlook[0]}>{outlook[1]}</Badge>}
      </div>

      {report.validation.status !== "notRun" && (
        <Section title={`Validation: ${report.validation.status === "passedWithWarnings" ? "passed with warnings" : report.validation.status}`}>
          <ul className="divide-y divide-border rounded-md border border-border">
            {report.validation.checks.map((c, i) => (
              <li key={i} className="flex items-start gap-2 px-2.5 py-1.5">
                {c.status === "passed" ? <CheckCircle2 size={13} className="mt-0.5 shrink-0 text-success" /> : c.status === "failed" ? <XCircle size={13} className="mt-0.5 shrink-0 text-danger" /> : c.status === "skipped" ? <Clock size={13} className="mt-0.5 shrink-0 text-fg-subtle" /> : <AlertTriangle size={13} className="mt-0.5 shrink-0 text-warning" />}
                <span className="w-44 shrink-0 font-medium">{c.name}</span>
                <span className="text-fg-muted selectable">{c.detail}</span>
              </li>
            ))}
          </ul>
        </Section>
      )}
      <ListSection title="Errors" items={report.errors} tone="danger" />
      <ListSection title="Warnings" items={report.warnings} tone="warning" />
      <ListSection title="Skipped items" items={report.skipped} tone="warning" />
      <ListSection title={`Detected corruption${report.corruptionCount > report.corruption.length ? ` (${report.corruptionCount} problems, ${report.corruption.length} listed)` : ""}`} items={report.corruption} tone="warning" collapsible />
      <ListSection title="Recovery actions" items={report.recoveryActions} tone="neutral" />
      <ListSection title="Notes" items={report.notes} tone="neutral" />
      <div className="flex flex-wrap items-center gap-2 pt-1">
        <span className="text-fg-subtle">Save report as</span>
        {(["html", "json", "txt"] as const).map((f) => (
          <Button key={f} size="sm" variant="ghost" onClick={() => saveAs(f)} disabled={!jobId}>
            <Download size={12} /> {f.toUpperCase()}
          </Button>
        ))}
      </div>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div>
      <div className="mb-1 font-semibold">{title}</div>
      {children}
    </div>
  );
}

function ListSection({ title, items, tone, collapsible }: { title: string; items: string[]; tone: "danger" | "warning" | "neutral"; collapsible?: boolean }) {
  const [open, setOpen] = useState(!collapsible);
  if (items.length === 0) return null;
  return (
    <div>
      <button className={cn("mb-1 flex items-center gap-1 font-semibold", tone === "danger" && "text-danger", tone === "warning" && "text-warning")} onClick={() => setOpen((v) => !v)}>
        {collapsible && (open ? <ChevronDown size={12} /> : <ChevronRight size={12} />)}
        {title} ({items.length})
      </button>
      {open && (
        <ul className={cn("max-h-56 overflow-auto rounded-md border p-2 selectable", tone === "danger" ? "border-danger/30 bg-danger-soft" : tone === "warning" ? "border-warning/30 bg-warning-soft" : "border-border bg-surface-2")}>
          {items.map((it, i) => (
            <li key={i} className="py-0.5">{it}</li>
          ))}
        </ul>
      )}
    </div>
  );
}
