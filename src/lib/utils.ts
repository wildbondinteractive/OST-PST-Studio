import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";
import type { FolderKind, HealthStatus, ItemType, JobKind, JobStatus } from "./api";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function humanBytes(n: number | null | undefined): string {
  if (n == null) return "";
  const units = ["bytes", "KB", "MB", "GB", "TB"];
  let v = n;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return i === 0 ? `${n} bytes` : `${v.toFixed(v < 10 ? 2 : 1)} ${units[i]}`;
}

export function formatDate(ms: number | null | undefined, opts: "short" | "long" = "short"): string {
  if (ms == null) return "";
  const d = new Date(ms);
  if (Number.isNaN(d.getTime())) return "";
  if (opts === "long") {
    return d.toLocaleString(undefined, { dateStyle: "full", timeStyle: "short" });
  }
  const now = new Date();
  const sameDay = d.toDateString() === now.toDateString();
  if (sameDay) return d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  const sameYear = d.getFullYear() === now.getFullYear();
  return d.toLocaleDateString(undefined, sameYear ? { month: "short", day: "numeric" } : { year: "numeric", month: "short", day: "numeric" });
}

export function formatDuration(ms: number): string {
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${(ms / 1000).toFixed(1)} s`;
  if (s < 3600) return `${Math.floor(s / 60)} min ${s % 60} s`;
  return `${Math.floor(s / 3600)} h ${Math.floor((s % 3600) / 60)} min`;
}

export function fileName(path: string): string {
  const i = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return i >= 0 ? path.slice(i + 1) : path;
}

export function dirName(path: string): string {
  const i = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return i >= 0 ? path.slice(0, i) : "";
}

export function hex(n: number): string {
  return "0x" + n.toString(16).toUpperCase();
}

export const healthLabel: Record<HealthStatus, string> = {
  healthy: "Healthy",
  damaged: "Damaged",
  recovered: "Recovered",
  critical: "Critical",
};

export const healthTone: Record<HealthStatus, "success" | "warning" | "danger" | "neutral"> = {
  healthy: "success",
  damaged: "warning",
  recovered: "warning",
  critical: "danger",
};

export const jobStatusLabel: Record<JobStatus, string> = {
  queued: "Queued",
  running: "Running",
  completed: "Completed",
  completedWithWarnings: "Completed with warnings",
  failed: "Failed",
  cancelled: "Cancelled",
};

export const jobKindLabel: Record<JobKind, string> = {
  convert: "Convert",
  repair: "Repair",
  export: "Export",
  verify: "Verify",
  extractAttachments: "Extract attachments",
};

export const itemTypeLabel: Record<ItemType, string> = {
  mail: "Mail",
  appointment: "Appointment",
  contact: "Contact",
  distributionList: "Distribution list",
  task: "Task",
  note: "Note",
  journal: "Journal",
  meetingRequest: "Meeting",
  report: "Report",
  other: "Other",
};

export const folderKindOrder: FolderKind[] = [
  "inbox", "drafts", "sentItems", "outbox", "deletedItems", "junkEmail", "archive",
  "calendar", "contacts", "tasks", "notes", "journal", "search", "other", "root",
];

export function pluralize(n: number, word: string, plural = word + "s") {
  return `${n.toLocaleString()} ${n === 1 ? word : plural}`;
}
