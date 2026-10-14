import { useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, errorMessage } from "@/lib/api";
import { useApp, type Page } from "@/lib/store";
import { TooltipProvider } from "@/components/ui";
import { Sidebar } from "@/components/layout/Sidebar";
import { Toasts } from "@/components/layout/Toasts";
import { DropOverlay } from "@/components/layout/DropOverlay";
import { Dashboard } from "@/pages/Dashboard";
import { Explorer } from "@/pages/Explorer";
import { ConvertPage } from "@/pages/Convert";
import { ExportPage } from "@/pages/Export";
import { RepairPage } from "@/pages/Repair";
import { BatchPage } from "@/pages/Batch";
import { ActivityPage } from "@/pages/Activity";
import { SettingsPage } from "@/pages/Settings";

function useTheme() {
  const theme = useApp((s) => s.settings?.theme ?? "system");
  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const dark = theme === "dark" || (theme === "system" && mq.matches);
      document.documentElement.classList.toggle("dark", dark);
    };
    apply();
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, [theme]);
}

function useBackendEvents() {
  const upsertJob = useApp((s) => s.upsertJob);
  const toast = useApp((s) => s.toast);
  const loadHistory = useApp((s) => s.loadHistory);
  useEffect(() => {
    const unlisteners: Promise<() => void>[] = [
      api.onJobUpdate(upsertJob),
      api.onJobProgress(upsertJob),
      api.onJobDone((r) => {
        upsertJob(r);
        void loadHistory();
        const name = r.source.split(/[\\/]/).pop() ?? r.source;
        if (r.status === "completed") toast({ tone: "success", title: `${labelOf(r.kind)} finished`, detail: name });
        else if (r.status === "completedWithWarnings") toast({ tone: "warning", title: `${labelOf(r.kind)} finished with warnings`, detail: name });
        else if (r.status === "failed") toast({ tone: "danger", title: `${labelOf(r.kind)} failed`, detail: r.error ?? name });
        else if (r.status === "cancelled") toast({ tone: "info", title: `${labelOf(r.kind)} cancelled`, detail: name });
      }),
    ];
    return () => {
      unlisteners.forEach((p) => p.then((u) => u()));
    };
  }, [upsertJob, toast, loadHistory]);
}

function labelOf(kind: string) {
  return { convert: "Conversion", repair: "Repair", export: "Export", verify: "Verification", extractAttachments: "Extraction" }[kind] ?? "Job";
}

function useKeyboardShortcuts() {
  const setPage = useApp((s) => s.setPage);
  useEffect(() => {
    const pages: Record<string, Page> = { "1": "dashboard", "2": "explorer", "3": "convert", "4": "export", "5": "repair", "6": "batch", "7": "activity", ",": "settings" };
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && !e.shiftKey && !e.altKey && pages[e.key]) {
        e.preventDefault();
        setPage(pages[e.key]);
      }
      if (e.ctrlKey && e.key.toLowerCase() === "o") {
        e.preventDefault();
        window.dispatchEvent(new CustomEvent("ostpst:open-file"));
      }
      if (e.ctrlKey && e.key.toLowerCase() === "f") {
        e.preventDefault();
        setPage("explorer");
        window.dispatchEvent(new CustomEvent("ostpst:focus-search"));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setPage]);
}

export default function App() {
  const page = useApp((s) => s.page);
  const loadSettings = useApp((s) => s.loadSettings);
  const loadHistory = useApp((s) => s.loadHistory);
  const refreshJobs = useApp((s) => s.refreshJobs);
  const refreshMailboxes = useApp((s) => s.refreshMailboxes);
  const toast = useApp((s) => s.toast);
  const openMailbox = useApp((s) => s.openMailbox);
  const setPage = useApp((s) => s.setPage);

  useTheme();
  useBackendEvents();
  useKeyboardShortcuts();

  useEffect(() => {
    void Promise.all([loadSettings(), loadHistory(), refreshJobs(), refreshMailboxes()]).catch((e) =>
      toast({ tone: "danger", title: "Could not load application state", detail: errorMessage(e) }),
    );
  }, [loadSettings, loadHistory, refreshJobs, refreshMailboxes, toast]);

  // Files dropped anywhere on the window open in the explorer.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void getCurrentWindow()
      .onDragDropEvent((e) => {
        if (e.payload.type === "enter" || e.payload.type === "over") document.body.dataset.dragging = "1";
        else delete document.body.dataset.dragging;
        if (e.payload.type === "drop") {
          const paths = e.payload.paths.filter((p) => /\.(pst|ost)$/i.test(p));
          if (paths.length === 0) {
            toast({ tone: "warning", title: "Not a PST or OST file", detail: "Drop .pst or .ost files to open them." });
            return;
          }
          (async () => {
            for (const p of paths) {
              try {
                await openMailbox(p);
                setPage("explorer");
              } catch (err) {
                toast({ tone: "danger", title: `Could not open ${p.split(/[\\/]/).pop()}`, detail: errorMessage(err) });
              }
            }
          })();
        }
      })
      .then((u) => (unlisten = u));
    return () => unlisten?.();
  }, [openMailbox, setPage, toast]);

  return (
    <TooltipProvider>
      <div className="flex h-full w-full bg-bg text-fg">
        <Sidebar />
        <main className="flex min-w-0 flex-1 flex-col overflow-hidden">
          {page === "dashboard" && <Dashboard />}
          {page === "explorer" && <Explorer />}
          {page === "convert" && <ConvertPage />}
          {page === "export" && <ExportPage />}
          {page === "repair" && <RepairPage />}
          {page === "batch" && <BatchPage />}
          {page === "activity" && <ActivityPage />}
          {page === "settings" && <SettingsPage />}
        </main>
        <Toasts />
        <DropOverlay />
      </div>
    </TooltipProvider>
  );
}
