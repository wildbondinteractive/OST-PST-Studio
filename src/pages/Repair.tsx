import { useEffect, useMemo, useState } from "react";
import { FileSearch, Play, Wrench } from "lucide-react";
import { api, errorMessage, type ProbeResult } from "@/lib/api";
import { useApp } from "@/lib/store";
import { Button, Card, CardHeader, PageHeader, Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui";
import { SourceField } from "@/components/jobs/FileFields";
import { JobCard } from "@/components/jobs/JobViews";
import { ConvertWizard } from "@/pages/Convert";

export function RepairPage() {
  const [tab, setTab] = useState("verify");
  const pendingSource = useApp((s) => s.pendingSource);
  return (
    <div className="flex h-full flex-col overflow-hidden">
      <Tabs value={tab} onValueChange={setTab} className="flex h-full flex-col">
        <div className="shrink-0 px-6 pt-3">
          <TabsList>
            <TabsTrigger value="verify">Verify a file</TabsTrigger>
            <TabsTrigger value="repair">Repair to a new PST</TabsTrigger>
          </TabsList>
        </div>
        <TabsContent value="verify" className="min-h-0 flex-1 overflow-auto data-[state=inactive]:hidden">
          <VerifyPanel initial={pendingSource} />
        </TabsContent>
        <TabsContent value="repair" className="min-h-0 flex-1 overflow-hidden data-[state=inactive]:hidden">
          <ConvertWizard mode="repair" />
        </TabsContent>
      </Tabs>
    </div>
  );
}

function VerifyPanel({ initial }: { initial: string | null }) {
  const [source, setSource] = useState(initial ?? "");
  const [probe, setProbe] = useState<ProbeResult | null>(null);
  const [startedId, setStartedId] = useState<string | null>(null);
  const jobs = useApp((s) => s.jobs);
  const toast = useApp((s) => s.toast);
  const upsertJob = useApp((s) => s.upsertJob);
  const job = useMemo(() => jobs.find((j) => j.id === startedId) ?? null, [jobs, startedId]);
  const past = jobs.filter((j) => j.kind === "verify" && j.id !== startedId).slice().reverse();

  useEffect(() => {
    if (initial) setSource(initial);
  }, [initial]);

  const start = async () => {
    try {
      const id = await api.startJob({ kind: "verify", source });
      setStartedId(id);
      const rec = await api.getJob(id);
      if (rec) upsertJob(rec);
    } catch (e) {
      toast({ tone: "danger", title: "Could not start verification", detail: errorMessage(e) });
    }
  };

  return (
    <div>
      <PageHeader title="Verify a file" description="Check every checksum, cross-check folder tables against the index, and find out what a sweep would recover. Writes nothing." />
      <div className="px-6 pb-6">
        <Card className="max-w-3xl p-5">
          <SourceField value={source} onChange={setSource} probe={probe} onProbe={setProbe} />
          <div className="mt-3 flex items-center justify-between">
            <p className="max-w-lg text-xs text-fg-muted">
              <FileSearch size={12} className="mr-1 inline" />
              Reads the whole file: every block against its checksum, every index page, and the blocks carving finds with no index at all. On a large mailbox this takes a while.
            </p>
            <Button variant="primary" disabled={!probe?.isPstOrOst} onClick={start}><Play size={14} /> Verify</Button>
          </div>
        </Card>
        {job && (
          <div className="mt-4 max-w-4xl">
            <JobCard job={job} defaultOpen />
            {(job.status === "completed" || job.status === "completedWithWarnings") && (
              <p className="mt-2 flex items-center gap-1 text-xs text-fg-muted">
                <Wrench size={12} /> To write a clean copy, switch to the "Repair to a new PST" tab.
              </p>
            )}
          </div>
        )}
        {past.length > 0 && (
          <Card className="mt-6 max-w-4xl">
            <CardHeader title="Earlier verifications this session" />
            <div className="flex flex-col gap-2 p-3">
              {past.map((j) => <JobCard key={j.id} job={j} />)}
            </div>
          </Card>
        )}
      </div>
    </div>
  );
}
