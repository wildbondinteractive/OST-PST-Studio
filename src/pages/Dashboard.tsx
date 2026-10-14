import { useEffect } from "react";
import { ArrowRightLeft, Clock, FolderOpen, FolderOutput, Mail, ShieldCheck, Wrench, Activity as ActivityIcon, FileWarning } from "lucide-react";
import { useApp } from "@/lib/store";
import { errorMessage } from "@/lib/api";
import { Badge, Button, Card, CardHeader, EmptyState, PageHeader, Progress } from "@/components/ui";
import { pickMailboxFiles } from "@/components/layout/Sidebar";
import { formatDate, formatDuration, healthLabel, healthTone, humanBytes, jobKindLabel, jobStatusLabel, pluralize } from "@/lib/utils";

export function Dashboard() {
  const history = useApp((s) => s.history);
  const mailboxes = useApp((s) => s.mailboxes);
  const jobs = useApp((s) => s.jobs);
  const openMailbox = useApp((s) => s.openMailbox);
  const setPage = useApp((s) => s.setPage);
  const setActive = useApp((s) => s.setActiveMailbox);
  const toast = useApp((s) => s.toast);
  const loadHistory = useApp((s) => s.loadHistory);

  useEffect(() => {
    void loadHistory();
  }, [loadHistory]);

  const openPath = async (p: string) => {
    try {
      await openMailbox(p);
      setPage("explorer");
    } catch (e) {
      toast({ tone: "danger", title: "Could not open the file", detail: errorMessage(e) });
    }
  };
  const browse = async () => {
    const files = await pickMailboxFiles();
    for (const f of files) await openPath(f);
  };

  const active = jobs.filter((j) => j.status === "running" || j.status === "queued");
  const recentJobs = (history?.jobs ?? []).slice(0, 8);
  const lastValidated = (history?.jobs ?? []).find((j) => j.validation && j.validation !== "NotRun");
  const totals = mailboxes.reduce(
    (a, m) => ({ messages: a.messages + m.info.messageCount, folders: a.folders + m.info.folderCount, bytes: a.bytes + m.info.sizeBytes }),
    { messages: 0, folders: 0, bytes: 0 },
  );

  return (
    <div className="flex h-full flex-col overflow-auto">
      <PageHeader
        title="Dashboard"
        description="Open a mailbox to browse it, or start a conversion, export or repair."
        actions={
          <Button variant="primary" onClick={browse}>
            <FolderOpen size={15} /> Open file
          </Button>
        }
      />
      <div className="grid grid-cols-1 gap-4 px-6 pb-6 lg:grid-cols-3">
        {/* Quick actions */}
        <Card className="lg:col-span-3">
          <div className="grid grid-cols-2 divide-x divide-border md:grid-cols-4">
            {[
              { icon: ArrowRightLeft, title: "Convert OST to PST", desc: "Read an orphaned OST and write a validated Unicode PST.", page: "convert" as const },
              { icon: FolderOutput, title: "Export mailbox", desc: "EML, MSG, MBOX, HTML, text, CSV, vCard, iCalendar.", page: "export" as const },
              { icon: Wrench, title: "Repair a PST", desc: "Rebuild a damaged file from whatever survived.", page: "repair" as const },
              { icon: Mail, title: "Browse and search", desc: "Folders, messages, attachments and raw properties.", page: "explorer" as const },
            ].map((a) => (
              <button key={a.page} onClick={() => setPage(a.page)} className="flex flex-col items-start gap-2 p-4 text-left transition-colors hover:bg-surface-2">
                <span className="flex h-8 w-8 items-center justify-center rounded-md bg-accent-soft text-accent">
                  <a.icon size={16} />
                </span>
                <span className="text-[13px] font-semibold">{a.title}</span>
                <span className="text-xs text-fg-muted">{a.desc}</span>
              </button>
            ))}
          </div>
        </Card>

        {/* Open mailboxes */}
        <Card className="lg:col-span-2">
          <CardHeader
            title="Open mailboxes"
            description={mailboxes.length ? `${pluralize(totals.messages, "message")} across ${pluralize(totals.folders, "folder")}, ${humanBytes(totals.bytes)} on disk` : undefined}
          />
          {mailboxes.length === 0 ? (
            <EmptyState
              compact
              icon={<Mail size={20} />}
              title="Nothing open"
              description="Open a .pst or .ost file, or drop one onto this window. Files are read in place and never modified."
              action={<Button onClick={browse}>Open file</Button>}
            />
          ) : (
            <div className="divide-y divide-border">
              {mailboxes.map((m) => (
                <button
                  key={m.id}
                  className="flex w-full items-center gap-3 px-4 py-3 text-left hover:bg-surface-2"
                  onClick={() => {
                    setActive(m.id);
                    setPage("explorer");
                  }}
                >
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="truncate text-[13px] font-medium">{m.info.fileName}</span>
                      <Badge tone="neutral">{m.info.isOst ? "OST" : "PST"}</Badge>
                      <Badge tone={healthTone[m.info.health]}>{healthLabel[m.info.health]}</Badge>
                      {m.info.indexSource === "recovered" && <Badge tone="warning">index recovered</Badge>}
                    </div>
                    <div className="mt-0.5 truncate text-xs text-fg-muted">{m.info.path}</div>
                  </div>
                  <div className="shrink-0 text-right text-xs text-fg-muted">
                    <div>{pluralize(m.info.messageCount, "message")}</div>
                    <div>{pluralize(m.info.folderCount, "folder")} · {humanBytes(m.info.sizeBytes)}</div>
                  </div>
                  {m.info.problemCount > 0 && (
                    <span className="flex items-center gap-1 text-xs text-warning" title={m.info.warnings.join("\n")}>
                      <FileWarning size={14} /> {m.info.problemCount}
                    </span>
                  )}
                </button>
              ))}
            </div>
          )}
        </Card>

        {/* Jobs now */}
        <Card>
          <CardHeader title="Jobs" description={active.length ? `${active.length} in progress` : "Nothing running"} action={<Button variant="ghost" size="sm" onClick={() => setPage("activity")}>View all</Button>} />
          {active.length === 0 ? (
            <div className="flex items-center gap-2 px-4 py-4 text-xs text-fg-muted">
              <ActivityIcon size={14} /> Conversions, exports and repairs show their progress here.
            </div>
          ) : (
            <div className="divide-y divide-border">
              {active.map((j) => (
                <div key={j.id} className="px-4 py-3">
                  <div className="flex items-center justify-between gap-2 text-xs">
                    <span className="truncate font-medium">{jobKindLabel[j.kind]} · {j.source.split(/[\\/]/).pop()}</span>
                    <span className="shrink-0 text-fg-muted">{j.status === "queued" ? "queued" : j.progress.phase}</span>
                  </div>
                  <Progress
                    className="mt-2"
                    value={j.progress.total ? (j.progress.done / j.progress.total) * 100 : 0}
                    indeterminate={j.status === "running" && !j.progress.determinate}
                  />
                </div>
              ))}
            </div>
          )}
        </Card>

        {/* Recent files */}
        <Card className="lg:col-span-2">
          <CardHeader title="Recent files" />
          {!history?.recentFiles.length ? (
            <div className="flex items-center gap-2 px-4 py-4 text-xs text-fg-muted">
              <Clock size={14} /> Files you open appear here for quick access.
            </div>
          ) : (
            <div className="divide-y divide-border">
              {history.recentFiles.map((f) => (
                <button key={f.path} className="flex w-full items-center gap-3 px-4 py-2.5 text-left hover:bg-surface-2" onClick={() => void openPath(f.path)}>
                  <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded bg-surface-3 text-[10px] font-semibold uppercase text-fg-muted">{f.isOst ? "ost" : "pst"}</span>
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-[13px]">{f.fileName}</div>
                    <div className="truncate text-xs text-fg-subtle">{f.path}</div>
                  </div>
                  <div className="shrink-0 text-right text-xs text-fg-muted">
                    <div>{pluralize(f.messageCount, "message")}</div>
                    <div>{humanBytes(f.sizeBytes)} · {formatDate(f.openedMs)}</div>
                  </div>
                  <Badge tone={healthTone[f.health]}>{healthLabel[f.health]}</Badge>
                </button>
              ))}
            </div>
          )}
        </Card>

        {/* Last validation */}
        <Card>
          <CardHeader title="Last validation" />
          {lastValidated ? (
            <div className="px-4 py-3 text-xs">
              <div className="flex items-center gap-2">
                <ShieldCheck size={16} className={lastValidated.validation === "Passed" ? "text-success" : lastValidated.validation === "Failed" ? "text-danger" : "text-warning"} />
                <span className="font-medium">{lastValidated.validation === "PassedWithWarnings" ? "Passed with warnings" : lastValidated.validation}</span>
              </div>
              <div className="mt-1 truncate text-fg-muted">{lastValidated.destination.split(/[\\/]/).pop() || lastValidated.source.split(/[\\/]/).pop()}</div>
              <div className="mt-0.5 text-fg-subtle">{formatDate(lastValidated.finishedMs, "long")}</div>
            </div>
          ) : (
            <div className="px-4 py-4 text-xs text-fg-muted">Every conversion and repair reopens its output and checks it against the source. The result shows here.</div>
          )}
        </Card>

        {/* Previous jobs */}
        <Card className="lg:col-span-3">
          <CardHeader title="Previous jobs" action={<Button variant="ghost" size="sm" onClick={() => setPage("activity")}>Reports</Button>} />
          {recentJobs.length === 0 ? (
            <div className="px-4 py-4 text-xs text-fg-muted">No jobs have run yet.</div>
          ) : (
            <table className="w-full text-xs">
              <thead className="text-left text-fg-subtle">
                <tr>
                  <th className="px-4 py-2 font-medium">Job</th>
                  <th className="px-2 py-2 font-medium">Source</th>
                  <th className="px-2 py-2 font-medium">Destination</th>
                  <th className="px-2 py-2 font-medium">Messages</th>
                  <th className="px-2 py-2 font-medium">Duration</th>
                  <th className="px-2 py-2 font-medium">Validation</th>
                  <th className="px-4 py-2 font-medium">Status</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border">
                {recentJobs.map((j) => (
                  <tr key={j.id} className="hover:bg-surface-2">
                    <td className="px-4 py-2 font-medium">{jobKindLabel[j.kind]}</td>
                    <td className="max-w-[240px] truncate px-2 py-2" title={j.source}>{j.source.split(/[\\/]/).pop()}</td>
                    <td className="max-w-[240px] truncate px-2 py-2" title={j.destination}>{j.destination.split(/[\\/]/).pop() || "-"}</td>
                    <td className="px-2 py-2">{j.messages.toLocaleString()}</td>
                    <td className="px-2 py-2">{formatDuration(j.durationMs)}</td>
                    <td className="px-2 py-2">{j.validation && j.validation !== "NotRun" ? j.validation : "-"}</td>
                    <td className="px-4 py-2">
                      <Badge tone={j.status === "completed" ? "success" : j.status === "failed" ? "danger" : j.status === "cancelled" ? "neutral" : "warning"}>{jobStatusLabel[j.status]}</Badge>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </Card>
      </div>
    </div>
  );
}
