import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { Activity, Download, Trash2 } from "lucide-react";
import { api, errorMessage, type JobReport } from "@/lib/api";
import { useApp } from "@/lib/store";
import { formatDate, formatDuration, jobKindLabel, jobStatusLabel } from "@/lib/utils";
import { Badge, Button, Card, CardHeader, Dialog, DialogContent, EmptyState, PageHeader, Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui";
import { JobCard, ReportView, statusTone } from "@/components/jobs/JobViews";

export function ActivityPage() {
  const jobs = useApp((s) => s.jobs);
  const history = useApp((s) => s.history);
  const loadHistory = useApp((s) => s.loadHistory);
  const refreshJobs = useApp((s) => s.refreshJobs);
  const toast = useApp((s) => s.toast);
  const [tab, setTab] = useState("session");
  const [viewing, setViewing] = useState<JobReport | null>(null);

  useEffect(() => {
    void loadHistory();
    void refreshJobs();
    const h = () => {
      void loadHistory();
      void refreshJobs();
    };
    window.addEventListener("ostpst:refresh", h);
    return () => window.removeEventListener("ostpst:refresh", h);
  }, [loadHistory, refreshJobs]);

  const active = jobs.filter((j) => j.status === "running" || j.status === "queued");
  const finished = jobs.filter((j) => !active.includes(j)).slice().reverse();

  const saveFromHistory = async (json: string, name: string) => {
    const path = await save({ defaultPath: `${name}-report.json`, filters: [{ name: "JSON", extensions: ["json"] }] });
    if (!path) return;
    try {
      // The history keeps the JSON; no live job is needed to write it out.
      await api.writeTextFile(path, json);
      toast({ tone: "success", title: "Report saved", detail: path });
    } catch (e) {
      toast({ tone: "danger", title: "Could not save the report", detail: errorMessage(e) });
    }
  };

  return (
    <div className="flex h-full flex-col overflow-auto">
      <PageHeader
        title="Activity & Reports"
        description="Every job this session with its live progress, and the reports of earlier runs."
        actions={
          <Button variant="ghost" size="sm" onClick={() => api.clearFinishedJobs().then(refreshJobs)} disabled={finished.length === 0}>
            <Trash2 size={13} /> Clear finished
          </Button>
        }
      />
      <div className="px-6 pb-6">
        <Tabs value={tab} onValueChange={setTab}>
          <TabsList>
            <TabsTrigger value="session">This session ({jobs.length})</TabsTrigger>
            <TabsTrigger value="history">History ({history?.jobs.length ?? 0})</TabsTrigger>
          </TabsList>
          <TabsContent value="session" className="pt-4">
            {jobs.length === 0 ? (
              <EmptyState icon={<Activity size={22} />} title="No jobs yet" description="Conversions, repairs, exports and verifications started in this session appear here with their progress and report." />
            ) : (
              <div className="flex flex-col gap-3">
                {active.length > 0 && <div className="text-xs font-semibold text-fg-muted">In progress</div>}
                {active.map((j) => <JobCard key={j.id} job={j} defaultOpen />)}
                {finished.length > 0 && <div className="mt-2 text-xs font-semibold text-fg-muted">Finished</div>}
                {finished.map((j) => <JobCard key={j.id} job={j} />)}
              </div>
            )}
          </TabsContent>
          <TabsContent value="history" className="pt-4">
            <Card>
              <CardHeader title="Previous jobs" description="Kept locally in the application's data folder. Nothing leaves this computer." />
              {!history?.jobs.length ? (
                <div className="p-4 text-xs text-fg-muted">No history yet.</div>
              ) : (
                <table className="w-full text-xs">
                  <thead className="text-left text-fg-subtle">
                    <tr>
                      <th className="px-4 py-2 font-medium">When</th>
                      <th className="px-2 py-2 font-medium">Job</th>
                      <th className="px-2 py-2 font-medium">Source</th>
                      <th className="px-2 py-2 font-medium">Destination</th>
                      <th className="px-2 py-2 font-medium">Duration</th>
                      <th className="px-2 py-2 font-medium">Status</th>
                      <th className="px-4 py-2" />
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-border">
                    {history.jobs.map((j) => (
                      <tr key={j.id} className="hover:bg-surface-2">
                        <td className="whitespace-nowrap px-4 py-2">{formatDate(j.finishedMs, "long")}</td>
                        <td className="px-2 py-2 font-medium">{jobKindLabel[j.kind]}</td>
                        <td className="max-w-[220px] truncate px-2 py-2" title={j.source}>{j.source.split(/[\\/]/).pop()}</td>
                        <td className="max-w-[220px] truncate px-2 py-2" title={j.destination}>{j.destination.split(/[\\/]/).pop() || "-"}</td>
                        <td className="px-2 py-2">{formatDuration(j.durationMs)}</td>
                        <td className="px-2 py-2"><Badge tone={statusTone(j.status)}>{jobStatusLabel[j.status]}</Badge></td>
                        <td className="px-4 py-2 text-right">
                          {j.reportJson && (
                            <>
                              <Button size="sm" variant="ghost" onClick={() => setViewing(JSON.parse(j.reportJson!) as JobReport)}>View</Button>
                              <Button size="sm" variant="ghost" onClick={() => saveFromHistory(j.reportJson!, (j.destination || j.source).split(/[\\/]/).pop()?.replace(/\.[^.]+$/, "") ?? "job")}>
                                <Download size={12} />
                              </Button>
                            </>
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </Card>
          </TabsContent>
        </Tabs>
      </div>
      <Dialog open={!!viewing} onOpenChange={(o) => !o && setViewing(null)}>
        {viewing && (
          <DialogContent title={`${jobKindLabel[viewing.kind]} report`} description={viewing.source.path} wide>
            <ReportView report={viewing} />
          </DialogContent>
        )}
      </Dialog>
    </div>
  );
}
