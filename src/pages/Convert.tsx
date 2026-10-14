import { useEffect, useMemo, useState } from "react";
import { ArrowRight, ArrowRightLeft, Check, Info, Mail, Play, ShieldCheck } from "lucide-react";
import { api, errorMessage, type Collision, type OpenedMailbox, type ProbeResult } from "@/lib/api";
import { useApp } from "@/lib/store";
import { cn, healthLabel, healthTone, humanBytes, pluralize } from "@/lib/utils";
import { Badge, Button, Card, Checkbox, Field, PageHeader, Select, Spinner, Switch } from "@/components/ui";
import { SourceField, OutputField } from "@/components/jobs/FileFields";
import { FolderTree } from "@/components/explorer/FolderTree";
import { JobCard } from "@/components/jobs/JobViews";

const STEPS = ["Source", "Analyse", "Options", "Run"] as const;

/// The conversion wizard. Repair uses the same component with `mode="repair"`: the
/// engine treats both as "write a validated PST from this file", and only the words
/// and the default expectations differ.
export function ConvertWizard({ mode }: { mode: "convert" | "repair" }) {
  const settings = useApp((s) => s.settings);
  const pendingSource = useApp((s) => s.pendingSource);
  const setPendingSource = useApp((s) => s.setPendingSource);
  const jobs = useApp((s) => s.jobs);
  const toast = useApp((s) => s.toast);
  const upsertJob = useApp((s) => s.upsertJob);
  const setPage = useApp((s) => s.setPage);

  const [step, setStep] = useState(0);
  const [source, setSource] = useState("");
  const [probe, setProbe] = useState<ProbeResult | null>(null);
  const [analysis, setAnalysis] = useState<OpenedMailbox | null>(null);
  const [analysing, setAnalysing] = useState(false);
  const [analysisError, setAnalysisError] = useState<string | null>(null);
  const [salvage, setSalvage] = useState(false);
  const [destination, setDestination] = useState("");
  const [collision, setCollision] = useState<Collision>(settings?.collision ?? "rename");
  const [validate, setValidate] = useState(settings?.validateAfterWrite ?? true);
  const [maps, setMaps] = useState(settings?.writeAllocationMaps ?? true);
  const [startedId, setStartedId] = useState<string | null>(null);
  const [verifyAfter, setVerifyAfter] = useState(false);

  useEffect(() => {
    if (pendingSource) {
      setSource(pendingSource);
      setPendingSource(null);
      setStep(0);
    }
  }, [pendingSource, setPendingSource]);
  useEffect(() => {
    if (settings) {
      setCollision(settings.collision);
      setValidate(settings.validateAfterWrite);
      setMaps(settings.writeAllocationMaps);
    }
  }, [settings]);

  const sourceOk = !!probe?.exists && !!probe.isPstOrOst;
  const job = useMemo(() => jobs.find((j) => j.id === startedId) ?? null, [jobs, startedId]);

  const analyse = async () => {
    setAnalysing(true);
    setAnalysisError(null);
    setAnalysis(null);
    try {
      const opened = await api.openMailbox(source, salvage);
      setAnalysis(opened);
      // The analysis handle is closed again: the job opens its own.
      void api.closeMailbox(opened.id);
      if (!destination) setDestination(await api.suggestOutput(source, "pst"));
    } catch (e) {
      setAnalysisError(errorMessage(e));
    } finally {
      setAnalysing(false);
    }
  };

  const start = async () => {
    try {
      const id = await api.startJob({
        kind: mode,
        options: { source, destination, salvage, collision, validate, writeAllocationMaps: maps },
      });
      setStartedId(id);
      const rec = await api.getJob(id);
      if (rec) upsertJob(rec);
      setStep(3);
    } catch (e) {
      toast({ tone: "danger", title: "Could not start the job", detail: errorMessage(e) });
    }
  };

  const info = analysis?.info;
  const isConvert = mode === "convert";
  const warnOstAsPst = isConvert && probe && !probe.isOst && probe.format === "unicode";
  const explanation = !info
    ? ""
    : info.isOst && info.format === "unicode4k"
      ? "This is an Outlook 2013 or later OST. Its blocks are compressed and up to 64 KB, so every data stream is decoded and laid out again as a PST stores it. The message store is rebuilt with the folder entry ids a PST needs and no phantom password."
      : info.format === "ansi"
        ? "This is an ANSI PST from Outlook 97 to 2002. Every id is 32-bit and its trailers differ, so the contents are re-laid as a Unicode PST, which also lifts the 2 GB ceiling."
        : info.isOst
          ? "This is a pre-2013 OST, laid out exactly as a PST. Blocks are copied byte for byte and only the header's file type and the index are rewritten."
          : "This is a Unicode PST. Every surviving block is copied byte for byte and a fresh index, allocation maps and header are written around them. Nothing inside a block is parsed, so nothing inside one can be corrupted.";

  return (
    <div className="flex h-full flex-col overflow-auto">
      <PageHeader
        title={isConvert ? "Convert OST to PST" : "Repair & Recovery"}
        description={isConvert ? "Read an OST without Outlook or Exchange and write a validated Unicode PST." : "Rebuild a damaged PST into a new, clean file. The original is never modified."}
      />
      <div className="px-6 pb-6">
        <ol className="mb-5 flex items-center gap-2 text-xs">
          {STEPS.map((s, i) => (
            <li key={s} className="flex items-center gap-2">
              <button
                className={cn("flex items-center gap-1.5 rounded-full border px-2.5 py-1", i === step ? "border-accent bg-accent-soft text-accent" : i < step ? "border-border text-fg" : "border-border text-fg-subtle")}
                onClick={() => i < step && setStep(i)}
                disabled={i > step}
              >
                <span className={cn("flex h-4 w-4 items-center justify-center rounded-full text-[10px] font-semibold", i < step ? "bg-success text-white" : i === step ? "bg-accent text-accent-fg" : "bg-surface-3")}>
                  {i < step ? <Check size={10} /> : i + 1}
                </span>
                {s}
              </button>
              {i < STEPS.length - 1 && <ArrowRight size={12} className="text-fg-subtle" />}
            </li>
          ))}
        </ol>

        {step === 0 && (
          <Card className="max-w-3xl p-5">
            <SourceField value={source} onChange={(v) => { setSource(v); setAnalysis(null); }} probe={probe} onProbe={setProbe} accept={isConvert ? ["ost", "pst"] : ["pst", "ost"]} />
            {warnOstAsPst && (
              <p className="mt-2 flex items-center gap-1.5 text-xs text-fg-muted"><Info size={12} /> This is already a PST. It can still be rebuilt here, which copies every surviving block into a clean file.</p>
            )}
            <label className="mt-4 flex items-start gap-2 text-xs">
              <Checkbox checked={salvage} onCheckedChange={setSalvage} />
              <span>
                <span className="font-medium">Salvage mode:</span> ignore the header's index and rebuild it by sweeping the file for surviving pages. Use when the file opens but looks wrong, or to pick up deleted items whose index entries survived. Slower on large files.
              </span>
            </label>
            <div className="mt-5 flex justify-end">
              <Button variant="primary" disabled={!sourceOk} onClick={() => { setStep(1); void analyse(); }}>
                Analyse file <ArrowRight size={14} />
              </Button>
            </div>
          </Card>
        )}

        {step === 1 && (
          <div className="grid max-w-5xl gap-4 lg:grid-cols-[1fr_320px]">
            <Card className="p-5">
              {analysing && (
                <div className="flex items-center gap-2 text-xs text-fg-muted"><Spinner /> Reading the header, both B-trees and every folder…</div>
              )}
              {analysisError && (
                <div className="rounded-md border border-danger/40 bg-danger-soft p-3 text-xs text-danger">{analysisError}</div>
              )}
              {info && (
                <>
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-sm font-semibold">{info.fileName}</span>
                    <Badge tone="neutral">{info.isOst ? "OST" : "PST"}</Badge>
                    <Badge tone={healthTone[info.health]}>{healthLabel[info.health]}</Badge>
                    {info.indexSource === "recovered" && <Badge tone="warning">index recovered</Badge>}
                  </div>
                  <div className="mt-3 grid grid-cols-2 gap-2 md:grid-cols-4">
                    {[
                      ["Folders", info.folderCount.toLocaleString()],
                      ["Messages", info.messageCount.toLocaleString()],
                      ["Size", humanBytes(info.sizeBytes)],
                      ["Problems", String(info.problemCount)],
                    ].map(([k, v]) => (
                      <div key={k} className="rounded-md bg-surface-2 px-3 py-2">
                        <div className="text-[10px] uppercase tracking-wide text-fg-subtle">{k}</div>
                        <div className="text-[13px] font-semibold">{v}</div>
                      </div>
                    ))}
                  </div>
                  <p className="mt-3 text-xs text-fg-muted">{explanation}</p>
                  {info.essentialsMissing.length > 0 && (
                    <p className="mt-2 rounded-md border border-danger/40 bg-danger-soft p-2 text-xs text-danger">
                      Missing {info.essentialsMissing.join(" and ")}. A PST written from this file will not open in Outlook. Export the mail instead.
                    </p>
                  )}
                  {info.warnings.length > 0 && (
                    <details className="mt-3 text-xs">
                      <summary className="cursor-pointer font-medium text-warning">{info.problemCount} problem(s) in the source</summary>
                      <ul className="mt-1 max-h-40 overflow-auto rounded-md bg-surface-2 p-2 selectable">
                        {info.warnings.map((w, i) => <li key={i}>{w}</li>)}
                      </ul>
                    </details>
                  )}
                </>
              )}
              <div className="mt-5 flex justify-between">
                <Button variant="ghost" onClick={() => setStep(0)}>Back</Button>
                <Button variant="primary" disabled={!info} onClick={() => setStep(2)}>Choose options <ArrowRight size={14} /></Button>
              </div>
            </Card>
            <Card className="flex max-h-[60vh] flex-col overflow-hidden">
              <div className="border-b border-border px-3 py-2 text-xs font-semibold">Folders found</div>
              <div className="min-h-0 flex-1 overflow-auto px-1">
                {analysis ? <FolderTree folders={analysis.folders} selected={null} onSelect={() => {}} /> : <p className="p-3 text-xs text-fg-subtle">Waiting for the analysis.</p>}
              </div>
            </Card>
          </div>
        )}

        {step === 2 && (
          <Card className="max-w-3xl p-5">
            <OutputField value={destination} onChange={setDestination} kind="file" extension="pst" label="Write the PST to" hint="The file is written to a temporary name beside this path and moved into place only once it is complete and checked." />
            <div className="mt-4 grid gap-4 md:grid-cols-2">
              <Field label="If the output already exists">
                <Select<Collision>
                  value={collision}
                  onValueChange={setCollision}
                  options={[
                    { value: "rename", label: "Write beside it with a number", description: "mailbox (2).pst" },
                    { value: "fail", label: "Stop and report", description: "Nothing is written" },
                    { value: "overwrite", label: "Replace it", description: "The existing file is deleted" },
                  ]}
                />
              </Field>
            </div>
            <div className="mt-4 flex flex-col gap-3">
              <OptionRow checked={validate} onChange={setValidate} title="Validate the output after writing" desc="Reopens the new file, checks its structure, and compares folder, message and node counts with the source. Recommended; adds a full read of the output." />
              <OptionRow checked={maps} onChange={setMaps} title="Write valid allocation maps" desc="Rebuilds the AMap, PMap and density list so Outlook opens the file directly. Without this, Outlook asks for the Inbox Repair Tool (SCANPST) first." />
              <OptionRow checked={verifyAfter} onChange={setVerifyAfter} title="Also open the result in the explorer when done" desc="Handy for a quick look at what came across." />
            </div>
            <div className="mt-5 flex justify-between">
              <Button variant="ghost" onClick={() => setStep(1)}>Back</Button>
              <Button variant="primary" disabled={!destination.trim()} onClick={start}>
                <Play size={14} /> {isConvert ? "Convert" : "Repair"}
              </Button>
            </div>
          </Card>
        )}

        {step === 3 && (
          <div className="max-w-4xl">
            {job ? <JobCard job={job} defaultOpen /> : <div className="flex items-center gap-2 text-xs text-fg-muted"><Spinner /> Starting…</div>}
            {job && (job.status === "completed" || job.status === "completedWithWarnings") && (
              <div className="mt-4 flex flex-wrap items-center gap-2">
                <Button
                  variant="primary"
                  onClick={async () => {
                    try {
                      await useApp.getState().openMailbox(job.destination);
                      setPage("explorer");
                    } catch (e) {
                      toast({ tone: "danger", title: "Could not open the result", detail: errorMessage(e) });
                    }
                  }}
                >
                  <Mail size={14} /> Open the result in the explorer
                </Button>
                <Button onClick={() => { setStep(0); setStartedId(null); setAnalysis(null); setDestination(""); }}>
                  <ArrowRightLeft size={14} /> {isConvert ? "Convert another file" : "Repair another file"}
                </Button>
                {job.report?.outlook === "mapsWritten" && (
                  <span className="flex items-center gap-1 text-xs text-fg-muted"><ShieldCheck size={13} className="text-success" /> Validated by this engine and written with valid allocation maps. Open it in Outlook to confirm; see the manual check in the docs.</span>
                )}
              </div>
            )}
            {job && verifyAfter && (job.status === "completed" || job.status === "completedWithWarnings") && <AutoOpen path={job.destination} />}
            {job && (job.status === "failed" || job.status === "cancelled") && (
              <div className="mt-4">
                <Button onClick={() => setStep(2)}>Back to options</Button>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function AutoOpen({ path }: { path: string }) {
  const openMailbox = useApp((s) => s.openMailbox);
  const setPage = useApp((s) => s.setPage);
  const [done, setDone] = useState(false);
  useEffect(() => {
    if (done) return;
    setDone(true);
    openMailbox(path).then(() => setPage("explorer")).catch(() => {});
  }, [path, done, openMailbox, setPage]);
  return null;
}

function OptionRow({ checked, onChange, title, desc }: { checked: boolean; onChange: (v: boolean) => void; title: string; desc: string }) {
  return (
    <div className="flex items-start justify-between gap-4 rounded-md border border-border p-3">
      <div>
        <div className="text-[13px] font-medium">{title}</div>
        <div className="mt-0.5 text-xs text-fg-muted">{desc}</div>
      </div>
      <Switch checked={checked} onCheckedChange={onChange} />
    </div>
  );
}

export function ConvertPage() {
  return <ConvertWizard mode="convert" />;
}

export { pluralize };
