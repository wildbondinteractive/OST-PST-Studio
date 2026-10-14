import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Activity, ArrowRightLeft, FolderOpen, FolderOutput, LayoutDashboard, Layers, Mail, PanelLeftClose, PanelLeftOpen, Settings, Wrench, X,
} from "lucide-react";
import { useApp, type Page } from "@/lib/store";
import { errorMessage } from "@/lib/api";
import { cn, healthTone } from "@/lib/utils";
import { Badge, Button, Tooltip } from "@/components/ui";

const NAV: { page: Page; label: string; icon: React.ComponentType<{ size?: number }>; shortcut: string }[] = [
  { page: "dashboard", label: "Dashboard", icon: LayoutDashboard, shortcut: "Ctrl+1" },
  { page: "explorer", label: "Mailbox Explorer", icon: Mail, shortcut: "Ctrl+2" },
  { page: "convert", label: "Convert OST to PST", icon: ArrowRightLeft, shortcut: "Ctrl+3" },
  { page: "export", label: "Export Mailbox", icon: FolderOutput, shortcut: "Ctrl+4" },
  { page: "repair", label: "Repair & Recovery", icon: Wrench, shortcut: "Ctrl+5" },
  { page: "batch", label: "Batch Operations", icon: Layers, shortcut: "Ctrl+6" },
  { page: "activity", label: "Activity & Reports", icon: Activity, shortcut: "Ctrl+7" },
];

export async function pickMailboxFiles(multiple = false): Promise<string[]> {
  const picked = await open({
    multiple,
    directory: false,
    title: multiple ? "Open OST or PST files" : "Open an OST or PST file",
    filters: [
      { name: "Outlook data files", extensions: ["pst", "ost"] },
      { name: "All files", extensions: ["*"] },
    ],
  });
  if (!picked) return [];
  return Array.isArray(picked) ? picked : [picked];
}

export function Sidebar() {
  const page = useApp((s) => s.page);
  const setPage = useApp((s) => s.setPage);
  const mailboxes = useApp((s) => s.mailboxes);
  const activeId = useApp((s) => s.activeMailboxId);
  const setActive = useApp((s) => s.setActiveMailbox);
  const closeMailbox = useApp((s) => s.closeMailbox);
  const openMailbox = useApp((s) => s.openMailbox);
  const openingPath = useApp((s) => s.openingPath);
  const jobs = useApp((s) => s.jobs);
  const toast = useApp((s) => s.toast);
  const [collapsed, setCollapsed] = useState(() => localStorage.getItem("sidebar") === "collapsed");

  useEffect(() => localStorage.setItem("sidebar", collapsed ? "collapsed" : "open"), [collapsed]);

  const running = jobs.filter((j) => j.status === "running" || j.status === "queued").length;

  const doOpen = async () => {
    const files = await pickMailboxFiles(false);
    for (const f of files) {
      try {
        await openMailbox(f);
        setPage("explorer");
      } catch (e) {
        toast({ tone: "danger", title: "Could not open the file", detail: errorMessage(e) });
      }
    }
  };
  useEffect(() => {
    const h = () => void doOpen();
    window.addEventListener("ostpst:open-file", h);
    return () => window.removeEventListener("ostpst:open-file", h);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <aside className={cn("flex h-full shrink-0 flex-col border-r border-border bg-bg-subtle transition-[width] duration-150", collapsed ? "w-14" : "w-60")}>
      <div className={cn("flex h-12 items-center gap-2 px-3", collapsed && "justify-center px-0")}>
        <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-accent text-accent-fg">
          <Mail size={15} />
        </div>
        {!collapsed && (
          <div className="min-w-0 leading-tight">
            <div className="truncate text-[13px] font-semibold">OST/PST Studio</div>
            <div className="truncate text-[10px] text-fg-subtle">Outlook data recovery</div>
          </div>
        )}
      </div>

      <div className={cn("px-2 pb-2", collapsed && "px-1.5")}>
        <Tooltip content="Open an OST or PST file (Ctrl+O)" side="right">
          <Button variant="primary" className={cn("w-full", collapsed && "px-0")} onClick={doOpen} disabled={!!openingPath}>
            <FolderOpen size={15} />
            {!collapsed && <span>Open file</span>}
          </Button>
        </Tooltip>
      </div>

      <nav className="flex flex-col gap-0.5 px-2" aria-label="Main">
        {NAV.map(({ page: p, label, icon: Icon, shortcut }) => {
          const active = page === p;
          const button = (
            <button
              key={p}
              onClick={() => setPage(p)}
              className={cn(
                "relative flex h-8 w-full items-center gap-2.5 rounded-md px-2 text-[13px] transition-colors",
                active ? "bg-surface font-medium text-fg shadow-sm" : "text-fg-muted hover:bg-surface hover:text-fg",
                collapsed && "justify-center px-0",
              )}
              aria-current={active ? "page" : undefined}
            >
              {active && <span className="absolute left-0 top-1.5 h-5 w-[3px] rounded-r bg-accent" />}
              <Icon size={16} />
              {!collapsed && <span className="flex-1 truncate text-left">{label}</span>}
              {!collapsed && p === "activity" && running > 0 && <Badge tone="accent">{running}</Badge>}
            </button>
          );
          return collapsed ? (
            <Tooltip key={p} content={`${label} (${shortcut})`} side="right">
              {button}
            </Tooltip>
          ) : (
            button
          );
        })}
      </nav>

      {!collapsed && (
        <div className="mt-4 min-h-0 flex-1 overflow-auto px-2">
          <div className="px-2 pb-1 text-[11px] font-medium uppercase tracking-wide text-fg-subtle">Open mailboxes</div>
          {mailboxes.length === 0 && !openingPath && <p className="px-2 py-1 text-xs text-fg-subtle">None yet. Open or drop a file.</p>}
          {openingPath && (
            <div className="flex items-center gap-2 px-2 py-1 text-xs text-fg-muted">
              <span className="skeleton h-3 w-3 rounded-full" /> Opening {openingPath.split(/[\\/]/).pop()}
            </div>
          )}
          {mailboxes.map((m) => (
            <div
              key={m.id}
              className={cn(
                "group flex items-center gap-2 rounded-md px-2 py-1.5 text-xs",
                m.id === activeId ? "bg-surface shadow-sm" : "hover:bg-surface",
              )}
            >
              <button
                className="flex min-w-0 flex-1 items-center gap-2 text-left"
                onClick={() => {
                  setActive(m.id);
                  setPage("explorer");
                }}
                title={m.info.path}
              >
                <span className={cn("h-2 w-2 shrink-0 rounded-full", {
                  "bg-success": healthTone[m.info.health] === "success",
                  "bg-warning": healthTone[m.info.health] === "warning",
                  "bg-danger": healthTone[m.info.health] === "danger",
                })} />
                <span className="truncate">{m.info.fileName}</span>
                <span className="ml-auto shrink-0 text-[10px] uppercase text-fg-subtle">{m.info.isOst ? "ost" : "pst"}</span>
              </button>
              <button
                className="hidden h-5 w-5 shrink-0 items-center justify-center rounded text-fg-subtle hover:bg-surface-3 hover:text-fg group-hover:flex"
                onClick={() => void closeMailbox(m.id)}
                aria-label={`Close ${m.info.fileName}`}
              >
                <X size={12} />
              </button>
            </div>
          ))}
        </div>
      )}
      {collapsed && <div className="flex-1" />}

      <div className={cn("flex items-center gap-1 border-t border-border p-2", collapsed && "flex-col")}>
        <Tooltip content="Settings (Ctrl+,)" side="right">
          <Button variant={page === "settings" ? "subtle" : "ghost"} size="icon" onClick={() => setPage("settings")} aria-label="Settings">
            <Settings size={16} />
          </Button>
        </Tooltip>
        <div className="flex-1" />
        <Tooltip content={collapsed ? "Expand sidebar" : "Collapse sidebar"} side="right">
          <Button variant="ghost" size="icon" onClick={() => setCollapsed((c) => !c)} aria-label="Toggle sidebar">
            {collapsed ? <PanelLeftOpen size={16} /> : <PanelLeftClose size={16} />}
          </Button>
        </Tooltip>
      </div>
    </aside>
  );
}
