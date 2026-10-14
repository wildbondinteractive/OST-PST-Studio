import { useEffect, useMemo, useState } from "react";
import { FolderOutput, Play } from "lucide-react";
import { api, errorMessage, type Collision, type ExportFormat, type OpenedMailbox, type ProbeResult } from "@/lib/api";
import { useActiveMailbox, useApp } from "@/lib/store";
import { cn, pluralize } from "@/lib/utils";
import { Button, Card, Checkbox, Field, PageHeader, Select, Spinner, Switch } from "@/components/ui";
import { SourceField, OutputField } from "@/components/jobs/FileFields";
import { FolderTree } from "@/components/explorer/FolderTree";
import { JobCard } from "@/components/jobs/JobViews";

const FORMATS: { value: ExportFormat; label: string; description: string; supported: boolean }[] = [
  { value: "eml", label: "EML (RFC 5322)", description: "One file per message, attachments inline. Opens anywhere.", supported: true },
  { value: "msg", label: "MSG (Outlook)", description: "Outlook's own compound-file format with MAPI properties.", supported: true },
  { value: "mbox", label: "MBOX", description: "One file per folder, mboxrd escaping. For Thunderbird and archives.", supported: true },
  { value: "html", label: "HTML", description: "One sandboxed page per message, attachments beside it.", supported: true },
  { value: "txt", label: "Plain text", description: "Headers and body as text, one file per message.", supported: true },
  { value: "csv", label: "CSV (listing / contacts)", description: "One row per item with contact fields. UTF-8 with BOM for Excel.", supported: true },
  { value: "vcf", label: "vCard 3.0", description: "Every contact in the selection, one .vcf file.", supported: true },
  { value: "ics", label: "iCalendar", description: "Every appointment in the selection, times in UTC, one .ics file.", supported: true },
  { value: "json", label: "JSON", description: "Full message detail and raw properties, for technical analysis.", supported: true },
  { value: "pst", label: "PST (selection)", description: "Not available yet. Use Convert or Repair to write the whole mailbox as a PST.", supported: false },
];

export function ExportPage() {
  const active = useActiveMailbox();
  const settings = useApp((s) => s.settings);
  const pendingSource = useApp((s) => s.pendingSource);
  const setPendingSource = useApp((s) => s.setPendingSource);
  const selectedMessages = useApp((s) => s.selectedMessages);
  const jobs = useApp((s) => s.jobs);
  const toast = useApp((s) => s.toast);
  const upsertJob = useApp((s) => s.upsertJob);

  const [source, setSource] = useState("");
  const [probe, setProbe] = useState<ProbeResult | null>(null);
  const [mailbox, setMailbox] = useState<OpenedMailbox | null>(null);
  const [loading, setLoading] = useState(false);
  const [scope, setScope] = useState<"all" | "folders" | "messages">("all");
  const [checked, setChecked] = useState<Set<number>>(new Set());
  const [format, setFormat] = useState<ExportFormat>(settings?.defaultExportFormat ?? "eml");
  const [destination, setDestination] = useState("");
  const [collision, setCollision] = useState<Collision>(settings?.collision ?? "rename");
  const [extract, setExtract] = useState(true);
  const [salvage, setSalvage] = useState(false);
  const [startedId, setStartedId] = useState<string | null>(null);
  const job = useMemo(() => jobs.find((j) => j.id === startedId) ?? null, [jobs, startedId]);

  // Prefer the mailbox the user came from; a pending source from the explorer wins.
  useEffect(() => {
    if (pendingSource) {
      setSource(pendingSource);
      setPendingSource(null);
      if (selectedMessages.length > 0 && active?.info.path === pendingSource) setScope("messages");
    } else if (!source && active) {
      setSource(active.info.path);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pendingSource, active]);
  useEffect(() => {
    if (settings) {
      setFormat(settings.defaultExportFormat);
      setCollision(settings.collision);
    }
  }, [settings]);

  // The folder tree for the selection comes from the open mailbox when it is the same
  // file, otherwise from a short-lived analysis handle.
  useEffect(() => {
    setMailbox(null);
    setChecked(new Set());
    if (!probe?.isPstOrOst) return;
    if (active && active.info.path === source) {
      setMailbox(active);
      return;
    }
    let live = true;
    setLoading(true);
    api
      .openMailbox(source, salvage)
      .then((m) => {
        if (!live) return void api.closeMailbox(m.id);
        setMailbox(m);
      })
      .catch((e) => live && toast({ tone: "danger", title: "Could not read the file", detail: errorMessage(e) }))
      .finally(() => live && setLoading(false));
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [probe?.path, probe?.isPstOrOst, salvage]);

  useEffect(() => {
    if (source && !destination) api.suggestOutput(source, "").then(setDestination).catch(() => {});
  }, [source, destination]);

  const selectedCount = useMemo(() => {
    if (!mailbox) return 0;
    if (scope === "all") return mailbox.info.messageCount;
    if (scope === "messages") return selectedMessages.length;
    const by = new Map(mailbox.folders.map((f) => [f.nid, f]));
    const seen = new Set<number>();
    let n = 0;
    const walk = (nid: number) => {
      if (seen.has(nid)) return;
      seen.add(nid);
      const f = by.get(nid);
      if (!f) return;
      n += f.messageCount;
      f.children.forEach(walk);
    };
    checked.forEach(walk);
    return n;
  }, [mailbox, scope, checked, selectedMessages]);

  const start = async () => {
    try {
      const id = await api.startJob({
        kind: "export",
        options: {
          source,
          destination,
          format,
          selection: {
            entireMailbox: scope === "all",
            folders: scope === "folders" ? [...checked] : [],
            messages: scope === "messages" ? selectedMessages : [],
          },
          salvage,
          collision,
          extractAttachments: extract,
        },
      });
      setStartedId(id);
      const rec = await api.getJob(id);
      if (rec) upsertJob(rec);
    } catch (e) {
      toast({ tone: "danger", title: "Could not start the export", detail: errorMessage(e) });
    }
  };

  const fmt = FORMATS.find((f) => f.value === format)!;
  const canRun = !!probe?.isPstOrOst && !!destination.trim() && fmt.supported && selectedCount > 0;

  return (
    <div className="flex h-full flex-col overflow-auto">
      <PageHeader title="Export Mailbox" description="Write the whole mailbox, chosen folders, or selected messages in a mail, contact or calendar format." />
      <div className="grid gap-4 px-6 pb-6 xl:grid-cols-[1fr_360px]">
        <div className="flex flex-col gap-4">
          <Card className="p-5">
            <SourceField value={source} onChange={(v) => { setSource(v); setDestination(""); }} probe={probe} onProbe={setProbe} />
            <label className="mt-2 flex items-center gap-2 text-xs">
              <Checkbox checked={salvage} onCheckedChange={setSalvage} /> Salvage mode (ignore the header's index and sweep the file)
            </label>
          </Card>

          <Card className="p-5">
            <div className="mb-3 text-[13px] font-semibold">What to export</div>
            <div className="flex flex-wrap gap-2">
              {([
                ["all", "Entire mailbox", mailbox ? pluralize(mailbox.info.messageCount, "message") : ""],
                ["folders", "Selected folders", checked.size ? `${checked.size} chosen` : ""],
                ["messages", "Selected messages", selectedMessages.length ? `${selectedMessages.length} from the explorer` : "select some in the explorer first"],
              ] as const).map(([v, label, sub]) => (
                <button
                  key={v}
                  onClick={() => setScope(v)}
                  disabled={v === "messages" && (selectedMessages.length === 0 || active?.info.path !== source)}
                  className={cn("flex min-w-[160px] flex-col items-start rounded-md border px-3 py-2 text-left text-xs disabled:opacity-50", scope === v ? "border-accent bg-accent-soft" : "border-border hover:bg-surface-2")}
                >
                  <span className="font-medium">{label}</span>
                  <span className="text-fg-subtle">{sub}</span>
                </button>
              ))}
            </div>
            {scope === "folders" && (
              <div className="mt-3 max-h-72 overflow-auto rounded-md border border-border bg-surface-2 px-1">
                {loading && <div className="flex items-center gap-2 p-3 text-xs text-fg-muted"><Spinner size={12} /> Reading folders…</div>}
                {mailbox && (
                  <FolderTree
                    folders={mailbox.folders}
                    selected={null}
                    onSelect={() => {}}
                    checked={checked}
                    onCheckedChange={(nid, v) =>
                      setChecked((s) => {
                        const n = new Set(s);
                        if (v) n.add(nid);
                        else n.delete(nid);
                        return n;
                      })
                    }
                  />
                )}
              </div>
            )}
            <p className="mt-2 text-xs text-fg-muted">{selectedCount > 0 ? `${pluralize(selectedCount, "message")} will be written.` : "Nothing selected yet."}</p>
          </Card>

          <Card className="p-5">
            <div className="grid gap-4 md:grid-cols-2">
              <Field label="Format">
                <Select<ExportFormat> value={format} onValueChange={setFormat} options={FORMATS.map((f) => ({ value: f.value, label: f.label, description: f.description }))} />
              </Field>
              <Field label="If a file already exists">
                <Select<Collision>
                  value={collision}
                  onValueChange={setCollision}
                  options={[
                    { value: "rename", label: "Write beside it with a number" },
                    { value: "fail", label: "Stop and report" },
                    { value: "overwrite", label: "Replace it" },
                  ]}
                />
              </Field>
            </div>
            <p className="mt-2 text-xs text-fg-muted">{fmt.description}</p>
            {!fmt.supported && <p className="mt-1 text-xs text-warning">This format is planned and not available yet.</p>}
            <OutputField value={destination} onChange={setDestination} kind="dir" label="Write into folder" hint="One directory per mailbox folder is created inside it, mirroring the tree." />
            {(format === "html" || format === "txt" || format === "json") && (
              <div className="mt-3 flex items-center justify-between rounded-md border border-border p-3">
                <div>
                  <div className="text-[13px] font-medium">Extract attachments as files</div>
                  <div className="text-xs text-fg-muted">Written beside each message in a folder named after it. Names are sanitised; nothing is executed.</div>
                </div>
                <Switch checked={extract} onCheckedChange={setExtract} />
              </div>
            )}
            <div className="mt-5 flex justify-end">
              <Button variant="primary" disabled={!canRun} onClick={start}><Play size={14} /> Export</Button>
            </div>
          </Card>
        </div>

        <div className="flex flex-col gap-3">
          <div className="text-[13px] font-semibold">Exports this session</div>
          {job && <JobCard job={job} defaultOpen />}
          {jobs.filter((j) => j.kind === "export" && j.id !== startedId).slice().reverse().map((j) => <JobCard key={j.id} job={j} />)}
          {jobs.filter((j) => j.kind === "export").length === 0 && (
            <div className="rounded-lg border border-dashed border-border p-4 text-xs text-fg-subtle">
              <FolderOutput size={14} className="mb-1" /> Progress and the report for each export appear here.
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
