import { useCallback, useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, errorMessage } from "@/lib/api";
import { isMailboxPath, useApp, type Page } from "@/lib/store";
import { fileName, pluralize } from "@/lib/utils";
import { TooltipProvider } from "@/components/ui";
import { Sidebar } from "@/components/layout/Sidebar";
import { Toasts } from "@/components/layout/Toasts";
import { DropOverlay, MultiDropDialog } from "@/components/layout/DropOverlay";
import { CloseGuard } from "@/components/layout/CloseGuard";
import { UpdateDialog } from "@/components/updates/UpdateDialog";
import { UpdateNotice } from "@/components/updates/UpdateNotice";
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
  const setUpdate = useApp((s) => s.setUpdate);
  const setCloseBlocked = useApp((s) => s.setCloseBlocked);
  useEffect(() => {
    const unlisteners: Promise<() => void>[] = [
      api.onJobUpdate(upsertJob),
      api.onJobProgress(upsertJob),
      api.onJobDone((r) => {
        upsertJob(r);
        void loadHistory();
        const name = fileName(r.source);
        const where = r.destination ? `${name} → ${fileName(r.destination)}` : name;
        if (r.status === "completed") toast({ tone: "success", title: `${labelOf(r.kind)} finished`, detail: where });
        else if (r.status === "completedWithWarnings") toast({ tone: "warning", title: `${labelOf(r.kind)} finished with warnings`, detail: `${where}. The report on the Activity page lists them.` });
        else if (r.status === "failed") toast({ tone: "danger", title: `${labelOf(r.kind)} failed`, detail: r.error ? `${name}: ${r.error}` : name });
        else if (r.status === "cancelled") toast({ tone: "info", title: `${labelOf(r.kind)} cancelled`, detail: `${name}. Unfinished output was removed.` });
      }),
      api.onUpdateStatus(setUpdate),
      api.onCloseBlocked(setCloseBlocked),
    ];
    return () => {
      unlisteners.forEach((p) => p.then((u) => u()));
    };
  }, [upsertJob, toast, loadHistory, setUpdate, setCloseBlocked]);
}

function labelOf(kind: string) {
  return { convert: "Conversion", repair: "Repair", export: "Export", verify: "Verification", extractAttachments: "Extraction" }[kind] ?? "Job";
}

/// A modal dialog has the keyboard; window shortcuts stand aside while one is open.
const dialogOpen = () => !!document.querySelector('[role="dialog"][data-state="open"], [role="alertdialog"][data-state="open"]');

function useKeyboardShortcuts() {
  const setPage = useApp((s) => s.setPage);
  const openSettings = useApp((s) => s.openSettings);
  useEffect(() => {
    const pages: Record<string, Page> = { "1": "dashboard", "2": "explorer", "3": "convert", "4": "export", "5": "repair", "6": "batch", "7": "activity" };
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented) return;
      const key = e.key.toLowerCase();
      // F5 and Ctrl+R would reload the whole window and lose the view; they refresh the
      // current page's data instead.
      if (e.key === "F5" || (e.ctrlKey && !e.altKey && key === "r")) {
        e.preventDefault();
        if (!dialogOpen()) window.dispatchEvent(new CustomEvent("ostpst:refresh"));
        return;
      }
      if (!e.ctrlKey || e.altKey || e.metaKey || dialogOpen()) return;
      if (!e.shiftKey && pages[e.key]) {
        e.preventDefault();
        setPage(pages[e.key]);
      } else if (!e.shiftKey && e.key === ",") {
        e.preventDefault();
        openSettings();
      } else if (!e.shiftKey && key === "o") {
        e.preventDefault();
        window.dispatchEvent(new CustomEvent("ostpst:open-file"));
      } else if (!e.shiftKey && key === "f") {
        // Search needs an open mailbox; without one, leave the key alone.
        if (useApp.getState().mailboxes.length === 0) return;
        e.preventDefault();
        setPage("explorer");
        // After the explorer has rendered its search box.
        setTimeout(() => window.dispatchEvent(new CustomEvent("ostpst:focus-search")), 60);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setPage, openSettings]);
}

let sessionRestored = false;

export default function App() {
  const page = useApp((s) => s.page);
  const loadSettings = useApp((s) => s.loadSettings);
  const loadHistory = useApp((s) => s.loadHistory);
  const refreshJobs = useApp((s) => s.refreshJobs);
  const refreshMailboxes = useApp((s) => s.refreshMailboxes);
  const toast = useApp((s) => s.toast);
  const openMailbox = useApp((s) => s.openMailbox);
  const setPage = useApp((s) => s.setPage);
  const setPendingSource = useApp((s) => s.setPendingSource);
  const setDrag = useApp((s) => s.setDrag);
  const setMultiDrop = useApp((s) => s.setMultiDrop);
  const setUpdate = useApp((s) => s.setUpdate);

  useTheme();
  useBackendEvents();
  useKeyboardShortcuts();

  const openAll = useCallback(
    async (paths: string[]) => {
      let opened = 0;
      for (const p of paths) {
        try {
          await openMailbox(p);
          opened++;
        } catch (err) {
          toast({ tone: "danger", title: `Could not open ${fileName(p)}`, detail: errorMessage(err) });
        }
      }
      if (opened) setPage("explorer");
    },
    [openMailbox, setPage, toast],
  );

  useEffect(() => {
    void Promise.all([loadSettings(), loadHistory(), refreshJobs(), refreshMailboxes(), api.updateStatus().then(setUpdate)])
      .then(async () => {
        // Reopen the last session once per launch (the host returns nothing when the
        // setting is off, and only files that are still there).
        if (sessionRestored || useApp.getState().mailboxes.length > 0) return;
        sessionRestored = true;
        const paths = await api.sessionToRestore();
        let opened = 0;
        for (const p of paths) {
          try {
            await openMailbox(p);
            opened++;
          } catch {
            // Reported below as a count; a file that no longer opens is not worth a toast each.
          }
        }
        if (paths.length) {
          toast({
            tone: opened === paths.length ? "info" : "warning",
            title: `Reopened ${pluralize(opened, "mailbox", "mailboxes")} from your last session`,
            detail: opened < paths.length ? `${paths.length - opened} could not be opened.` : undefined,
          });
        }
      })
      .catch((e) => toast({ tone: "danger", title: "Could not load application state", detail: errorMessage(e) }));
  }, [loadSettings, loadHistory, refreshJobs, refreshMailboxes, toast, openMailbox, setUpdate]);

  // Files dragged over and dropped anywhere on the window.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void getCurrentWindow()
      .onDragDropEvent((e) => {
        const p = e.payload;
        if (p.type === "enter") {
          setDrag({ supported: p.paths.filter(isMailboxPath), unsupported: p.paths.filter((x) => !isMailboxPath(x)) });
          return;
        }
        if (p.type === "over") return;
        setDrag(null);
        if (p.type !== "drop") return;
        if (dialogOpen()) {
          toast({ tone: "info", title: "Close the open dialog first", detail: "Then drop the files again." });
          return;
        }
        const supported = p.paths.filter(isMailboxPath);
        const ignored = p.paths.filter((x) => !isMailboxPath(x));
        if (supported.length === 0) {
          toast({ tone: "warning", title: ignored.length === 1 ? `${fileName(ignored[0])} is not an OST or PST file` : "None of these are OST or PST files", detail: "Drop .pst or .ost files to open them." });
          return;
        }
        if (ignored.length) toast({ tone: "info", title: `${pluralize(ignored.length, "file")} ignored`, detail: "Only .pst and .ost files can be opened." });
        if (supported.length > 1) {
          setMultiDrop(supported);
          return;
        }
        // One file: on a page that takes a source, it becomes the source; elsewhere it opens.
        const current = useApp.getState().page;
        if (current === "convert" || current === "export" || current === "repair") {
          setPendingSource(supported[0]);
          toast({ tone: "info", title: "Source selected", detail: fileName(supported[0]) });
          return;
        }
        void openAll(supported);
      })
      .then((u) => (unlisten = u));
    return () => unlisten?.();
  }, [openAll, setDrag, setMultiDrop, setPendingSource, toast]);

  return (
    <TooltipProvider>
      <div className="flex h-full w-full bg-bg text-fg">
        <Sidebar />
        <main className="relative flex min-w-0 flex-1 flex-col overflow-hidden">
          {page === "dashboard" && <Dashboard />}
          {page === "explorer" && <Explorer />}
          {page === "convert" && <ConvertPage />}
          {page === "export" && <ExportPage />}
          {page === "repair" && <RepairPage />}
          {page === "batch" && <BatchPage />}
          {page === "activity" && <ActivityPage />}
          {page === "settings" && <SettingsPage />}
          <UpdateNotice />
        </main>
        <Toasts />
        <DropOverlay />
        <MultiDropDialog onOpenAll={(paths) => void openAll(paths)} />
        <UpdateDialog />
        <CloseGuard />
      </div>
    </TooltipProvider>
  );
}
