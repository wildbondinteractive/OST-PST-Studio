// Typed wrappers over the Tauri commands. The types mirror crates/ostpst-core/src/model.rs
// one to one (camelCase, same names), so a change on either side shows up here.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type FileFormat = "unicode" | "unicode4k" | "ansi";
export type IndexSource = "header" | "recovered";
export type HealthStatus = "healthy" | "damaged" | "recovered" | "critical";

export interface StaleNode { nid: number; versions: number; dangling: boolean; bidData: number }
export interface NodeTypeCount { nidType: number; name: string; count: number }
export interface RecoveryInfo {
  pagesScanned: number; nbtPages: number; bbtPages: number; damagedPages: number;
  superseded: number; unresolved: number; carvedBlocks: number; sweptNodes: number; stale: StaleNode[];
}
export interface FileInfo {
  path: string; fileName: string; sizeBytes: number; declaredBytes: number; isOst: boolean;
  format: FileFormat; formatVersion: number; encoding: string; amapValid: boolean;
  indexSource: IndexSource; health: HealthStatus; nodeCount: number; blockCount: number;
  folderCount: number; messageCount: number; nodeTypes: NodeTypeCount[]; warnings: string[];
  problemCount: number; recovery: RecoveryInfo | null; essentialsMissing: string[];
}
export type FolderKind =
  | "root" | "inbox" | "outbox" | "sentItems" | "deletedItems" | "drafts" | "junkEmail" | "archive"
  | "calendar" | "contacts" | "tasks" | "notes" | "journal" | "search" | "other";
export interface FolderInfo {
  nid: number; parent: number; name: string; path: string; depth: number; kind: FolderKind;
  containerClass: string | null; contentCount: number; unreadCount: number; messageCount: number;
  subfolderCount: number; children: number[]; unreadable: boolean; orphan: boolean; hasChildren: boolean;
}
export type ItemType =
  | "mail" | "appointment" | "contact" | "distributionList" | "task" | "note" | "journal"
  | "meetingRequest" | "report" | "other";
export interface MessageSummary {
  nid: number; folder: number; subject: string; senderName: string; senderEmail: string;
  displayTo: string; dateMs: number | null; size: number; hasAttachments: boolean;
  importance: number; read: boolean; itemType: ItemType; messageClass: string; fromTable: boolean; recovered: boolean;
}
export interface MessagePage { folder: number; offset: number; total: number; items: MessageSummary[]; unreadable: number }
export type SortKey = "date" | "sender" | "subject" | "size";
export interface RecipientInfo { kind: number; name: string; email: string }
export interface AttachmentInfo {
  nid: number; name: string; mime: string; size: number; method: number;
  isEmbeddedMessage: boolean; extension: string; sha256: string | null;
}
export interface PropertyView { tag: number; ptype: number; name: string | null; typeName: string; value: string; bytes: number | null }
export interface MessageDetail {
  summary: MessageSummary; recipients: RecipientInfo[]; senderRepresentingName: string;
  senderRepresentingEmail: string; bodyText: string | null; bodyHtml: string | null;
  transportHeaders: string | null; internetMessageId: string | null; conversationTopic: string | null;
  createdMs: number | null; modifiedMs: number | null; sentMs: number | null; receivedMs: number | null;
  creatorName: string | null; lastModifierName: string | null; sensitivity: number;
  attachments: AttachmentInfo[]; properties: PropertyView[]; extra: [string, string][]; problems: string[];
}
export interface SearchQuery {
  text: string; inSubject: boolean; inSender: boolean; inRecipients: boolean; inBody: boolean;
  inAttachmentNames: boolean; folder: number | null; dateFromMs: number | null; dateToMs: number | null;
  hasAttachments: boolean | null; attachmentName: string | null; minSize: number | null; maxSize: number | null;
  itemTypes: ItemType[]; importance: number | null; unreadOnly: boolean; limit: number | null;
}
export const emptyQuery = (): SearchQuery => ({
  text: "", inSubject: true, inSender: true, inRecipients: false, inBody: false, inAttachmentNames: false,
  folder: null, dateFromMs: null, dateToMs: null, hasAttachments: null, attachmentName: null,
  minSize: null, maxSize: null, itemTypes: [], importance: null, unreadOnly: false, limit: 2000,
});
export interface SearchHit { summary: MessageSummary; matched: string }
export interface SearchResult { hits: SearchHit[]; scanned: number; total: number; truncated: boolean; cancelled: boolean; elapsedMs: number }
export type ExportFormat = "pst" | "eml" | "msg" | "mbox" | "html" | "txt" | "csv" | "vcf" | "ics" | "json";
export interface Selection { entireMailbox: boolean; folders: number[]; messages: number[] }
export type Collision = "fail" | "rename" | "overwrite";
export interface ConvertOptions {
  source: string; destination: string; salvage: boolean; collision: Collision;
  validate: boolean; writeAllocationMaps: boolean;
}
export interface ExportOptions {
  source: string; destination: string; format: ExportFormat; selection: Selection;
  salvage: boolean; collision: Collision; extractAttachments: boolean;
}
export type JobKind = "convert" | "repair" | "export" | "verify" | "extractAttachments";
export type JobStatus = "queued" | "running" | "completed" | "completedWithWarnings" | "failed" | "cancelled";
export interface JobProgress {
  phase: string; done: number; total: number; determinate: boolean; itemsOk: number; itemsFailed: number;
  warnings: number; bytesWritten: number; elapsedMs: number; etaMs: number | null;
}
export type CheckStatus = "passed" | "warning" | "failed" | "skipped";
export interface Check { name: string; status: CheckStatus; detail: string }
export type ValidationStatus = "notRun" | "passed" | "passedWithWarnings" | "failed";
export interface ValidationReport {
  status: ValidationStatus; checks: Check[]; nodeCount: number; folderCount: number; messageCount: number;
  messagesRead: number; messagesUnreadable: number; attachmentsRead: number; elapsedMs: number;
}
export type OutlookCompatibility = "unverified" | "requiresScanpst" | "mapsWritten" | "verifiedManually";
export interface FileSummary { path: string; sizeBytes: number; format: FileFormat | null; isOst: boolean | null }
export interface Counts { folders: number; messages: number; attachments: number; nodes: number; blocks: number }
export interface JobReport {
  id: string; kind: JobKind; status: JobStatus; appVersion: string; source: FileSummary; destination: FileSummary;
  startedMs: number; finishedMs: number; durationMs: number; sourceHealth: HealthStatus | null;
  indexSource: IndexSource | null; sourceCounts: Counts; outputCounts: Counts; skipped: string[];
  droppedNodes: number; droppedBlocks: number; corruption: string[]; corruptionCount: number;
  recoveryActions: string[]; validation: ValidationReport; outlook: OutlookCompatibility;
  warnings: string[]; errors: string[]; notes: string[]; exportFormat: ExportFormat | null; filesWritten: number;
}
export type JobSpec =
  | { kind: "convert"; options: ConvertOptions }
  | { kind: "repair"; options: ConvertOptions }
  | { kind: "export"; options: ExportOptions }
  | { kind: "verify"; source: string };
export interface JobRecord {
  id: string; kind: JobKind; status: JobStatus; source: string; destination: string; createdMs: number;
  startedMs: number | null; finishedMs: number | null; progress: JobProgress; error: string | null;
  report: JobReport | null;
}
export interface OpenedMailbox { id: string; info: FileInfo; folders: FolderInfo[] }
export interface ProbeResult {
  path: string; fileName: string; exists: boolean; sizeBytes: number; isPstOrOst: boolean; isOst: boolean;
  format: FileFormat | null; warnings: string[]; error: string | null;
}
export interface AttachmentPreview { info: AttachmentInfo; mime: string; base64: string; text: string | null; tooLarge: boolean }
export interface SavedAttachment { info: AttachmentInfo; path: string }
export interface ExtractResult { written: SavedAttachment[]; failed: string[]; skippedEmbedded: number }
export interface Settings {
  theme: "system" | "light" | "dark"; defaultOutputDir: string | null; defaultExportFormat: ExportFormat;
  collision: Collision; maxConcurrentJobs: number; writeAllocationMaps: boolean; validateAfterWrite: boolean;
  blockRemoteContent: boolean; showHtmlBodies: boolean; logVerbosity: string; rememberRecentFiles: boolean;
  checkForUpdates: boolean; messagePageSize: number; useScanCache: boolean; cacheDir: string | null;
}
export interface CacheInfo { dir: string; entries: number; bytes: number }
export interface RecentFile {
  path: string; fileName: string; sizeBytes: number; isOst: boolean; health: HealthStatus;
  folderCount: number; messageCount: number; openedMs: number;
}
export interface PastJob {
  id: string; kind: JobKind; status: JobStatus; source: string; destination: string; finishedMs: number;
  durationMs: number; messages: number; validation: string | null; reportJson: string | null;
}
export interface History { recentFiles: RecentFile[]; jobs: PastJob[] }
export interface PathInfo { exists: boolean; isDir: boolean; isFile: boolean; sizeBytes: number; freeSpace: number | null; isEmptyDir: boolean | null }
export interface AppInfo { version: string; engineVersion: string; dataDir: string; license: string; thirdParty: [string, string, string][] }

export const api = {
  probeFile: (path: string) => invoke<ProbeResult>("probe_file", { path }),
  openMailbox: (path: string, salvage = false) => invoke<OpenedMailbox>("open_mailbox", { path, salvage }),
  closeMailbox: (id: string) => invoke<void>("close_mailbox", { id }),
  listOpenMailboxes: () => invoke<OpenedMailbox[]>("list_open_mailboxes"),
  getFolders: (id: string) => invoke<FolderInfo[]>("get_folders", { id }),
  listMessages: (id: string, folder: number, offset: number, limit: number, sort: SortKey, descending: boolean) =>
    invoke<MessagePage>("list_messages", { id, folder, offset, limit, sort, descending }),
  getMessage: (id: string, nid: number) => invoke<MessageDetail>("get_message", { id, nid }),
  getEmbeddedMessage: (id: string, nid: number, attachment: number) =>
    invoke<MessageDetail>("get_embedded_message", { id, nid, attachment }),
  nodeProperties: (id: string, nid: number) => invoke<PropertyView[]>("node_properties", { id, nid }),
  attachmentPreview: (id: string, nid: number, attachment: number) =>
    invoke<AttachmentPreview>("attachment_preview", { id, nid, attachment }),
  saveAttachment: (id: string, nid: number, attachment: number, destination: string) =>
    invoke<SavedAttachment>("save_attachment", { id, nid, attachment, destination }),
  extractAttachments: (id: string, messages: number[], directory: string) =>
    invoke<ExtractResult>("extract_attachments", { id, messages, directory }),
  search: (id: string, query: SearchQuery) => invoke<SearchResult>("search", { id, query }),
  cancelSearch: (id: string) => invoke<void>("cancel_search", { id }),
  startJob: (spec: JobSpec) => invoke<string>("start_job", { spec }),
  cancelJob: (id: string) => invoke<void>("cancel_job", { id }),
  retryJob: (id: string) => invoke<string>("retry_job", { id }),
  listJobs: () => invoke<JobRecord[]>("list_jobs"),
  getJob: (id: string) => invoke<JobRecord | null>("get_job", { id }),
  clearFinishedJobs: () => invoke<void>("clear_finished_jobs"),
  saveReport: (id: string, path: string, format: "json" | "html" | "txt") =>
    invoke<void>("save_report", { id, path, format }),
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (settings: Settings) => invoke<Settings>("set_settings", { settings }),
  getHistory: () => invoke<History>("get_history"),
  clearHistory: () => invoke<void>("clear_history"),
  pathInfo: (path: string) => invoke<PathInfo>("path_info", { path }),
  suggestOutput: (source: string, extension: string) => invoke<string>("suggest_output", { source, extension }),
  appInfo: () => invoke<AppInfo>("app_info"),
  exportStructureJson: (id: string, path: string) => invoke<void>("export_structure_json", { id, path }),
  writeTextFile: (path: string, content: string) => invoke<void>("write_text_file", { path, content }),
  scanCacheInfo: () => invoke<CacheInfo>("scan_cache_info"),
  clearScanCache: () => invoke<number>("clear_scan_cache"),

  onJobUpdate: (cb: (r: JobRecord) => void): Promise<UnlistenFn> => listen<JobRecord>("job://update", (e) => cb(e.payload)),
  onJobProgress: (cb: (r: JobRecord) => void): Promise<UnlistenFn> => listen<JobRecord>("job://progress", (e) => cb(e.payload)),
  onJobDone: (cb: (r: JobRecord) => void): Promise<UnlistenFn> => listen<JobRecord>("job://done", (e) => cb(e.payload)),
  onSearchProgress: (cb: (p: { id: string; done: number; total: number }) => void): Promise<UnlistenFn> =>
    listen<{ id: string; done: number; total: number }>("search://progress", (e) => cb(e.payload)),
};

export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return String(e);
}
