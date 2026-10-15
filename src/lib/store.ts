// Application state. Mailboxes, jobs and settings live in the backend; this is the
// window's view of them plus pure UI state (navigation, selection, panel sizes).

import { create } from "zustand";
import { api, type Busy, type FolderInfo, type JobRecord, type OpenedMailbox, type RecentStatus, type Settings, type History, type UpdateStatus } from "./api";

export type Page = "dashboard" | "explorer" | "convert" | "export" | "repair" | "batch" | "activity" | "settings";
export type SettingsTab = "general" | "output" | "updates" | "privacy" | "about";

export interface Toast {
  id: number;
  tone: "info" | "success" | "warning" | "danger";
  title: string;
  detail?: string;
}

interface AppStore {
  page: Page;
  setPage: (p: Page) => void;

  mailboxes: OpenedMailbox[];
  activeMailboxId: string | null;
  setActiveMailbox: (id: string | null) => void;
  openMailbox: (path: string, salvage?: boolean) => Promise<OpenedMailbox>;
  closeMailbox: (id: string) => Promise<void>;
  refreshMailboxes: () => Promise<void>;
  openingPath: string | null;

  selectedFolder: number | null;
  setSelectedFolder: (nid: number | null) => void;
  selectedMessages: number[];
  setSelectedMessages: (nids: number[]) => void;

  jobs: JobRecord[];
  upsertJob: (j: JobRecord) => void;
  refreshJobs: () => Promise<void>;

  settings: Settings | null;
  loadSettings: () => Promise<void>;
  saveSettings: (s: Settings) => Promise<void>;

  history: History | null;
  loadHistory: () => Promise<void>;

  toasts: Toast[];
  toast: (t: Omit<Toast, "id">) => void;
  dismissToast: (id: number) => void;

  // Pre-filled job targets when navigating from the explorer.
  pendingSource: string | null;
  setPendingSource: (p: string | null) => void;
  // Files handed to the batch page (from a multi-file drop).
  pendingBatchFiles: string[];
  setPendingBatchFiles: (p: string[]) => void;

  settingsTab: SettingsTab;
  openSettings: (tab?: SettingsTab) => void;

  recentStatus: Record<string, RecentStatus>;
  refreshRecentStatus: () => Promise<void>;
  removeRecent: (path: string) => Promise<void>;
  clearRecent: () => Promise<void>;

  update: UpdateStatus | null;
  setUpdate: (s: UpdateStatus) => void;
  updateDialogOpen: boolean;
  setUpdateDialogOpen: (open: boolean) => void;

  // Closing was refused because work is running; the guard dialog shows this.
  closeBlocked: Busy | null;
  setCloseBlocked: (b: Busy | null) => void;

  // Files being dragged over the window, sorted into what can and cannot be opened.
  drag: DragInfo | null;
  setDrag: (d: DragInfo | null) => void;
  // Several mailboxes were dropped at once; the window asks what to do with them.
  multiDrop: string[] | null;
  setMultiDrop: (paths: string[] | null) => void;
}

export interface DragInfo { supported: string[]; unsupported: string[] }

export const isMailboxPath = (p: string) => /\.(pst|ost)$/i.test(p);

let toastSeq = 1;

export const useApp = create<AppStore>((set, get) => ({
  page: "dashboard",
  setPage: (page) => set({ page }),

  mailboxes: [],
  activeMailboxId: null,
  openingPath: null,
  setActiveMailbox: (id) => set({ activeMailboxId: id, selectedFolder: null, selectedMessages: [] }),
  openMailbox: async (path, salvage = false) => {
    set({ openingPath: path });
    try {
      const existing = get().mailboxes.find((m) => m.info.path === path && !salvage);
      if (existing) {
        set({ activeMailboxId: existing.id, selectedFolder: null, selectedMessages: [] });
        return existing;
      }
      const opened = await api.openMailbox(path, salvage);
      set((s) => ({
        mailboxes: [...s.mailboxes.filter((m) => m.id !== opened.id), opened],
        activeMailboxId: opened.id,
        selectedFolder: startFolder(opened, s.history, s.settings),
        selectedMessages: [],
      }));
      void get().loadHistory();
      return opened;
    } finally {
      set({ openingPath: null });
    }
  },
  closeMailbox: async (id) => {
    await api.closeMailbox(id);
    set((s) => {
      const mailboxes = s.mailboxes.filter((m) => m.id !== id);
      const activeMailboxId = s.activeMailboxId === id ? (mailboxes[0]?.id ?? null) : s.activeMailboxId;
      return { mailboxes, activeMailboxId, selectedFolder: null, selectedMessages: [] };
    });
  },
  refreshMailboxes: async () => {
    const mailboxes = await api.listOpenMailboxes();
    set((s) => ({
      mailboxes,
      activeMailboxId: s.activeMailboxId && mailboxes.some((m) => m.id === s.activeMailboxId) ? s.activeMailboxId : (mailboxes[0]?.id ?? null),
    }));
  },

  selectedFolder: null,
  setSelectedFolder: (nid) => set({ selectedFolder: nid, selectedMessages: [] }),
  selectedMessages: [],
  setSelectedMessages: (nids) => set({ selectedMessages: nids }),

  jobs: [],
  upsertJob: (j) =>
    set((s) => {
      const i = s.jobs.findIndex((x) => x.id === j.id);
      if (i < 0) return { jobs: [...s.jobs, j] };
      const jobs = s.jobs.slice();
      // Progress events carry no report; keep the last known one.
      jobs[i] = { ...j, report: j.report ?? jobs[i].report };
      return { jobs };
    }),
  refreshJobs: async () => set({ jobs: await api.listJobs() }),

  settings: null,
  loadSettings: async () => set({ settings: await api.getSettings() }),
  saveSettings: async (s) => set({ settings: await api.setSettings(s) }),

  history: null,
  loadHistory: async () => set({ history: await api.getHistory() }),

  toasts: [],
  toast: (t) => {
    const id = toastSeq++;
    set((s) => ({ toasts: [...s.toasts, { ...t, id }] }));
    setTimeout(() => get().dismissToast(id), t.tone === "danger" ? 9000 : 5000);
  },
  dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),

  pendingSource: null,
  setPendingSource: (pendingSource) => set({ pendingSource }),
  pendingBatchFiles: [],
  setPendingBatchFiles: (pendingBatchFiles) => set({ pendingBatchFiles }),

  settingsTab: "general",
  openSettings: (tab) => set((s) => ({ page: "settings", settingsTab: tab ?? s.settingsTab })),

  recentStatus: {},
  refreshRecentStatus: async () => {
    const list = await api.recentFilesStatus();
    set({ recentStatus: Object.fromEntries(list.map((r) => [r.path, r])) });
  },
  removeRecent: async (path) => set({ history: await api.removeRecentFile(path) }),
  clearRecent: async () => set({ history: await api.clearRecentFiles(), recentStatus: {} }),

  update: null,
  setUpdate: (update) => set({ update }),
  updateDialogOpen: false,
  setUpdateDialogOpen: (updateDialogOpen) => set({ updateDialogOpen }),

  closeBlocked: null,
  setCloseBlocked: (closeBlocked) => set({ closeBlocked }),

  drag: null,
  setDrag: (drag) => set({ drag }),
  multiDrop: null,
  setMultiDrop: (multiDrop) => set({ multiDrop }),
}));

/// Where a freshly opened mailbox starts: the folder last viewed in it when that is
/// remembered and still exists, otherwise the usual default.
function startFolder(opened: OpenedMailbox, history: History | null, settings: Settings | null): number | null {
  if (settings?.rememberLastFolder !== false) {
    const last = history?.recentFiles.find((f) => f.path === opened.info.path)?.lastFolder;
    if (last != null && opened.folders.some((f) => f.nid === last && !f.unreadable)) return last;
  }
  return defaultFolder(opened.folders);
}

/// The folder to show first: the Inbox when there is one, else the first folder that
/// holds anything, else the root's first child.
export function defaultFolder(folders: FolderInfo[]): number | null {
  const inbox = folders.find((f) => f.kind === "inbox" && !f.orphan);
  if (inbox) return inbox.nid;
  const withMail = folders.find((f) => f.messageCount > 0 && !f.orphan && f.kind !== "root");
  if (withMail) return withMail.nid;
  const root = folders.find((f) => f.kind === "root");
  return root?.children[0] ?? folders[0]?.nid ?? null;
}

export function useActiveMailbox() {
  return useApp((s) => s.mailboxes.find((m) => m.id === s.activeMailboxId) ?? null);
}
