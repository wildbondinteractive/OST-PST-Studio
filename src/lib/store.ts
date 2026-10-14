// Application state. Mailboxes, jobs and settings live in the backend; this is the
// window's view of them plus pure UI state (navigation, selection, panel sizes).

import { create } from "zustand";
import { api, type FolderInfo, type JobRecord, type OpenedMailbox, type Settings, type History } from "./api";

export type Page = "dashboard" | "explorer" | "convert" | "export" | "repair" | "batch" | "activity" | "settings";

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
}

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
        selectedFolder: defaultFolder(opened.folders),
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
}));

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
