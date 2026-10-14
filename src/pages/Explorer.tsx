import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { ArrowRightLeft, FolderDown, FolderOutput, Info, Mail, RefreshCw, Search, SlidersHorizontal, X, FileJson, Braces, AlertTriangle } from "lucide-react";
import { api, emptyQuery, errorMessage, type MessageDetail, type MessageSummary, type SearchQuery, type SearchResult, type SortKey } from "@/lib/api";
import { defaultFolder, useActiveMailbox, useApp } from "@/lib/store";
import { cn, healthLabel, healthTone, humanBytes, pluralize } from "@/lib/utils";
import { Badge, Button, Checkbox, ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuSeparator, ContextMenuTrigger, Dialog, DialogContent, EmptyState, Field, Input, Kbd, Select, Tooltip } from "@/components/ui";
import { FolderTree } from "@/components/explorer/FolderTree";
import { MessageList, type ListState } from "@/components/explorer/MessageList";
import { MessagePreview, PropertyTable } from "@/components/explorer/MessagePreview";
import { pickMailboxFiles } from "@/components/layout/Sidebar";
import { FileHealthDialog } from "@/components/explorer/FileHealthDialog";

function usePaneWidth(key: string, initial: number, min: number, max: number) {
  const [w, setW] = useState(() => Number(localStorage.getItem(key)) || initial);
  useEffect(() => localStorage.setItem(key, String(w)), [key, w]);
  const [active, setActive] = useState(false);
  const onDown = (e: React.MouseEvent) => {
    e.preventDefault();
    setActive(true);
    const startX = e.clientX;
    const startW = w;
    const move = (ev: MouseEvent) => setW(Math.max(min, Math.min(max, startW + ev.clientX - startX)));
    const up = () => {
      setActive(false);
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };
  return { w, onDown, active };
}

export function Explorer() {
  const mb = useActiveMailbox();
  const openMailbox = useApp((s) => s.openMailbox);
  const setPage = useApp((s) => s.setPage);
  const toast = useApp((s) => s.toast);
  const setPendingSource = useApp((s) => s.setPendingSource);
  const selectedFolder = useApp((s) => s.selectedFolder);
  const setSelectedFolder = useApp((s) => s.setSelectedFolder);
  const selected = useApp((s) => s.selectedMessages);
  const setSelected = useApp((s) => s.setSelectedMessages);
  const pageSize = useApp((s) => s.settings?.messagePageSize ?? 200);

  const left = usePaneWidth("pane.folders", 260, 180, 520);
  const mid = usePaneWidth("pane.list", 420, 300, 900);

  const [items, setItems] = useState<MessageSummary[]>([]);
  const [total, setTotal] = useState(0);
  const [unreadable, setUnreadable] = useState(0);
  const [loading, setLoading] = useState(false);
  const [list, setList] = useState<ListState>({ sort: "date", descending: true });
  const [folderFilter, setFolderFilter] = useState("");
  const [search, setSearch] = useState<SearchQuery>(emptyQuery());
  const [searchResult, setSearchResult] = useState<SearchResult | null>(null);
  const [searching, setSearching] = useState<{ done: number; total: number } | null>(null);
  const [showFilters, setShowFilters] = useState(false);
  const [healthOpen, setHealthOpen] = useState(false);
  const [propsOf, setPropsOf] = useState<MessageDetail | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const reqSeq = useRef(0);

  const previewNid = selected.length === 1 ? selected[0] : null;
  const folders = mb?.folders ?? [];
  const folder = useMemo(() => folders.find((f) => f.nid === selectedFolder) ?? null, [folders, selectedFolder]);

  // Load a folder page by page.
  const loadPage = useCallback(
    async (offset: number, sort: SortKey, descending: boolean, replace: boolean) => {
      if (!mb || selectedFolder == null) return;
      const seq = ++reqSeq.current;
      setLoading(true);
      try {
        const page = await api.listMessages(mb.id, selectedFolder, offset, pageSize, sort, descending);
        if (seq !== reqSeq.current) return;
        setTotal(page.total);
        setUnreadable(page.unreadable);
        setItems((prev) => (replace ? page.items : [...prev, ...page.items]));
      } catch (e) {
        toast({ tone: "danger", title: "Could not list the folder", detail: errorMessage(e) });
      } finally {
        if (seq === reqSeq.current) setLoading(false);
      }
    },
    [mb, selectedFolder, pageSize, toast],
  );

  // A mailbox with nothing selected lands on its Inbox (or the first folder with mail).
  useEffect(() => {
    if (mb && selectedFolder == null) {
      const f = defaultFolder(mb.folders);
      if (f != null) setSelectedFolder(f);
    }
  }, [mb, selectedFolder, setSelectedFolder]);

  useEffect(() => {
    setSearchResult(null);
    setItems([]);
    setTotal(0);
    if (selectedFolder != null) void loadPage(0, list.sort, list.descending, true);
  }, [selectedFolder, list, loadPage]);

  useEffect(() => {
    const h = () => searchRef.current?.focus();
    window.addEventListener("ostpst:focus-search", h);
    return () => window.removeEventListener("ostpst:focus-search", h);
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void api.onSearchProgress((p) => mb && p.id === mb.id && setSearching({ done: p.done, total: p.total })).then((u) => (unlisten = u));
    return () => unlisten?.();
  }, [mb]);

  const runSearch = async (scopeAll: boolean) => {
    if (!mb) return;
    const q: SearchQuery = { ...search, folder: scopeAll ? null : selectedFolder };
    const anyFilter = q.text.trim() || q.hasAttachments != null || q.dateFromMs || q.dateToMs || q.minSize || q.maxSize || q.itemTypes.length || q.importance != null || q.unreadOnly || q.attachmentName;
    if (!anyFilter) {
      setSearchResult(null);
      return;
    }
    setSearching({ done: 0, total: 0 });
    try {
      const r = await api.search(mb.id, q);
      setSearchResult(r);
      setSelected([]);
    } catch (e) {
      toast({ tone: "danger", title: "Search failed", detail: errorMessage(e) });
    } finally {
      setSearching(null);
    }
  };

  const clearSearch = () => {
    setSearch(emptyQuery());
    setSearchResult(null);
  };

  const shownItems = searchResult ? searchResult.hits.map((h) => h.summary) : items;
  const matchedOf = useMemo(() => {
    if (!searchResult) return undefined;
    const m = new Map(searchResult.hits.map((h) => [h.summary.nid, h.matched]));
    return (nid: number) => m.get(nid);
  }, [searchResult]);

  const sortedSearch = useMemo(() => {
    if (!searchResult) return shownItems;
    const arr = shownItems.slice();
    const dir = list.descending ? -1 : 1;
    arr.sort((a, b) => {
      switch (list.sort) {
        case "date": return ((a.dateMs ?? 0) - (b.dateMs ?? 0)) * dir;
        case "sender": return a.senderName.localeCompare(b.senderName) * dir;
        case "subject": return a.subject.localeCompare(b.subject) * dir;
        case "size": return (a.size - b.size) * dir;
      }
    });
    return arr;
  }, [searchResult, shownItems, list]);

  const extractSelected = async () => {
    if (!mb || selected.length === 0) return;
    const dir = await openDialog({ directory: true, title: "Extract attachments to" });
    if (!dir || Array.isArray(dir)) return;
    try {
      const r = await api.extractAttachments(mb.id, selected, dir);
      toast({
        tone: r.failed.length ? "warning" : "success",
        title: `${r.written.length} attachment(s) extracted`,
        detail: [r.skippedEmbedded ? `${r.skippedEmbedded} embedded message(s) skipped` : "", ...r.failed].filter(Boolean).join("\n") || dir,
      });
      if (r.written.length) void revealItemInDir(r.written[0].path);
    } catch (e) {
      toast({ tone: "danger", title: "Extraction failed", detail: errorMessage(e) });
    }
  };

  const goExport = () => {
    if (!mb) return;
    setPendingSource(mb.info.path);
    setPage("export");
  };
  const goConvert = () => {
    if (!mb) return;
    setPendingSource(mb.info.path);
    setPage(mb.info.isOst ? "convert" : "repair");
  };

  if (!mb) {
    return (
      <div className="flex h-full flex-col">
        <EmptyState
          icon={<Mail size={24} />}
          title="Open a mailbox to explore it"
          description="Browse folders, read messages, inspect attachments and raw MAPI properties, and search across the whole file. Drop a .pst or .ost anywhere in this window, or pick one."
          action={
            <div className="flex items-center gap-2">
              <Button
                variant="primary"
                onClick={async () => {
                  for (const f of await pickMailboxFiles()) {
                    try {
                      await openMailbox(f);
                    } catch (e) {
                      toast({ tone: "danger", title: "Could not open the file", detail: errorMessage(e) });
                    }
                  }
                }}
              >
                Open file
              </Button>
              <span className="text-xs text-fg-subtle">
                <Kbd>Ctrl</Kbd> + <Kbd>O</Kbd>
              </span>
            </div>
          }
        />
      </div>
    );
  }

  const info = mb.info;
  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* Toolbar */}
      <div className="flex h-12 shrink-0 items-center gap-2 border-b border-border bg-surface px-3">
        <div className="flex min-w-0 items-center gap-2">
          <span className="truncate text-[13px] font-semibold" title={info.path}>{info.fileName}</span>
          <Badge tone="neutral">{info.isOst ? "OST" : "PST"}{info.format === "ansi" ? " · ANSI" : info.format === "unicode4k" ? " · 4K" : ""}</Badge>
          <button onClick={() => setHealthOpen(true)} className="inline-flex" title="File health and diagnostics">
            <Badge tone={healthTone[info.health]} className="cursor-pointer">{healthLabel[info.health]}{info.problemCount ? ` · ${info.problemCount}` : ""}</Badge>
          </button>
          <span className="hidden text-xs text-fg-subtle xl:inline">{pluralize(info.messageCount, "message")} · {pluralize(info.folderCount, "folder")} · {humanBytes(info.sizeBytes)}</span>
        </div>
        <div className="flex-1" />
        <form
          className="flex items-center gap-1"
          onSubmit={(e) => {
            e.preventDefault();
            void runSearch(true);
          }}
        >
          <div className="relative">
            <Search size={14} className="pointer-events-none absolute left-2.5 top-2 text-fg-subtle" />
            <Input
              ref={searchRef}
              className="h-8 w-64 pl-8 pr-7"
              placeholder="Search mailbox…"
              value={search.text}
              onChange={(e) => setSearch({ ...search, text: e.target.value })}
              aria-label="Search"
            />
            {(search.text || searchResult) && (
              <button type="button" className="absolute right-2 top-2 text-fg-subtle hover:text-fg" onClick={clearSearch} aria-label="Clear search">
                <X size={13} />
              </button>
            )}
          </div>
          <Tooltip content="Search filters">
            <Button type="button" variant={showFilters ? "subtle" : "ghost"} size="icon" onClick={() => setShowFilters((v) => !v)} aria-label="Filters">
              <SlidersHorizontal size={15} />
            </Button>
          </Tooltip>
          <Button type="submit" variant="secondary" size="sm" disabled={!!searching}>
            Search
          </Button>
        </form>
        <div className="mx-1 h-6 w-px bg-border" />
        <Tooltip content={info.isOst ? "Convert this OST to a PST" : "Repair this PST to a new file"}>
          <Button size="sm" onClick={goConvert}>
            <ArrowRightLeft size={14} /> {info.isOst ? "Convert" : "Repair"}
          </Button>
        </Tooltip>
        <Tooltip content="Export folders or selected messages">
          <Button size="sm" onClick={goExport}>
            <FolderOutput size={14} /> Export
          </Button>
        </Tooltip>
        <Tooltip content="File health and diagnostics">
          <Button size="icon" variant="ghost" onClick={() => setHealthOpen(true)} aria-label="File details">
            <Info size={15} />
          </Button>
        </Tooltip>
      </div>

      {showFilters && (
        <SearchFilters
          query={search}
          onChange={setSearch}
          onRun={() => void runSearch(true)}
          onRunHere={() => void runSearch(false)}
          folderName={folder?.name}
        />
      )}

      {searchResult && (
        <div className="flex h-8 shrink-0 items-center gap-2 border-b border-border bg-accent-soft px-3 text-xs">
          <Search size={13} className="text-accent" />
          <span>
            {searchResult.hits.length.toLocaleString()} hit(s) in {searchResult.scanned.toLocaleString()} message(s) · {searchResult.elapsedMs} ms
            {searchResult.truncated && " · stopped at the result limit"}
            {searchResult.cancelled && " · cancelled"}
          </span>
          <div className="flex-1" />
          <Button size="sm" variant="ghost" onClick={clearSearch}>Back to folder</Button>
        </div>
      )}
      {searching && (
        <div className="flex h-8 shrink-0 items-center gap-2 border-b border-border bg-surface-2 px-3 text-xs text-fg-muted">
          <RefreshCw size={13} className="animate-spin" />
          Searching {searching.done.toLocaleString()} of {searching.total.toLocaleString()}…
          <Button size="sm" variant="ghost" onClick={() => void api.cancelSearch(mb.id)}>Cancel</Button>
        </div>
      )}

      {/* Three panes */}
      <div className="flex min-h-0 flex-1">
        <div className="flex shrink-0 flex-col border-r border-border bg-bg-subtle" style={{ width: left.w }}>
          <div className="flex h-8 shrink-0 items-center gap-1 border-b border-border px-2">
            <Input className="h-6 text-xs" placeholder="Filter folders" value={folderFilter} onChange={(e) => setFolderFilter(e.target.value)} aria-label="Filter folders" />
          </div>
          <div className="min-h-0 flex-1 overflow-auto px-1">
            <FolderTree folders={folders} selected={selectedFolder} onSelect={setSelectedFolder} filter={folderFilter} />
          </div>
        </div>
        <div className={cn("pane-resizer", left.active && "active")} onMouseDown={left.onDown} role="separator" aria-orientation="vertical" />

        <div className="flex shrink-0 flex-col border-r border-border bg-surface" style={{ width: mid.w }}>
          <ContextMenu>
            <ContextMenuTrigger asChild>
              <div className="min-h-0 flex-1">
                <MessageList
                  items={sortedSearch}
                  total={searchResult ? searchResult.hits.length : total}
                  loading={loading}
                  selected={selected}
                  onSelect={(nids) => setSelected(nids)}
                  sort={list}
                  onSortChange={setList}
                  onLoadMore={() => !searchResult && items.length < total && void loadPage(items.length, list.sort, list.descending, false)}
                  hasMore={!searchResult && items.length < total}
                  unreadable={unreadable}
                  emptyTitle={searchResult ? "No matches" : folder ? `${folder.name} is empty` : "Select a folder"}
                  emptyDescription={searchResult ? "Nothing matched the search. Try fewer filters, or search the body as well." : undefined}
                  matchedOf={matchedOf}
                />
              </div>
            </ContextMenuTrigger>
            <ContextMenuContent>
              <ContextMenuItem onSelect={goExport} disabled={selected.length === 0}>
                <FolderOutput size={14} /> Export selected…
              </ContextMenuItem>
              <ContextMenuItem onSelect={extractSelected} disabled={selected.length === 0}>
                <FolderDown size={14} /> Extract attachments…
              </ContextMenuItem>
              <ContextMenuSeparator />
              <ContextMenuItem
                disabled={selected.length !== 1}
                onSelect={async () => {
                  if (selected.length !== 1) return;
                  try {
                    setPropsOf(await api.getMessage(mb.id, selected[0]));
                  } catch (e) {
                    toast({ tone: "danger", title: "Could not read the message", detail: errorMessage(e) });
                  }
                }}
              >
                <Braces size={14} /> Raw properties
              </ContextMenuItem>
              <ContextMenuItem onSelect={() => setSelected(sortedSearch.map((m) => m.nid))}>Select all</ContextMenuItem>
            </ContextMenuContent>
          </ContextMenu>
        </div>
        <div className={cn("pane-resizer", mid.active && "active")} onMouseDown={mid.onDown} role="separator" aria-orientation="vertical" />

        <div className="min-w-0 flex-1 bg-surface">
          {selected.length > 1 ? (
            <EmptyState
              icon={<Mail size={22} />}
              title={`${selected.length.toLocaleString()} messages selected`}
              description="Export them, or extract every attachment they carry."
              action={
                <div className="flex gap-2">
                  <Button variant="primary" onClick={goExport}><FolderOutput size={14} /> Export selected</Button>
                  <Button onClick={extractSelected}><FolderDown size={14} /> Extract attachments</Button>
                </div>
              }
            />
          ) : (
            <MessagePreview mailboxId={mb.id} nid={previewNid} onOpenEmbedded={setPropsOf} />
          )}
        </div>
      </div>

      <FileHealthDialog open={healthOpen} onOpenChange={setHealthOpen} mailbox={mb} />
      <Dialog open={!!propsOf} onOpenChange={(o) => !o && setPropsOf(null)}>
        {propsOf && (
          <DialogContent title={propsOf.summary.subject} description="Every MAPI property on this item, as stored" wide className="h-[80vh]">
            <div className="h-full">
              <PropertyTable properties={propsOf.properties} nid={propsOf.summary.nid} />
            </div>
          </DialogContent>
        )}
      </Dialog>
    </div>
  );
}

interface Preset {
  name: string;
  query: SearchQuery;
}

function loadPresets(): Preset[] {
  try {
    return JSON.parse(localStorage.getItem("search.presets") ?? "[]") as Preset[];
  } catch {
    return [];
  }
}

function SearchFilters({ query, onChange, onRun, onRunHere, folderName }: { query: SearchQuery; onChange: (q: SearchQuery) => void; onRun: () => void; onRunHere: () => void; folderName?: string }) {
  const set = (patch: Partial<SearchQuery>) => onChange({ ...query, ...patch });
  const [presets, setPresets] = useState<Preset[]>(loadPresets);
  const [presetName, setPresetName] = useState("");
  const savePresets = (p: Preset[]) => {
    setPresets(p);
    localStorage.setItem("search.presets", JSON.stringify(p));
  };
  const savePreset = () => {
    const name = presetName.trim();
    if (!name) return;
    savePresets([...presets.filter((p) => p.name !== name), { name, query: { ...query, folder: null } }]);
    setPresetName("");
  };
  const toDate = (ms: number | null) => (ms ? new Date(ms).toISOString().slice(0, 10) : "");
  const fromDate = (s: string, endOfDay: boolean) => (s ? Date.parse(s + (endOfDay ? "T23:59:59Z" : "T00:00:00Z")) : null);
  const fields: [keyof SearchQuery, string][] = [["inSubject", "Subject"], ["inSender", "Sender"], ["inRecipients", "Recipients"], ["inBody", "Body"], ["inAttachmentNames", "Attachment names"]];
  return (
    <div className="shrink-0 border-b border-border bg-surface-2 px-3 py-2">
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-6">
        <div className="col-span-2 flex flex-col gap-1">
          <span className="text-xs font-medium text-fg-muted">Search in</span>
          <div className="flex flex-wrap gap-x-3 gap-y-1">
            {fields.map(([k, label]) => (
              <label key={k} className="flex items-center gap-1.5 text-xs">
                <Checkbox checked={query[k] as boolean} onCheckedChange={(v) => set({ [k]: v } as Partial<SearchQuery>)} /> {label}
              </label>
            ))}
          </div>
        </div>
        <Field label="From date">
          <Input type="date" value={toDate(query.dateFromMs)} onChange={(e) => set({ dateFromMs: fromDate(e.target.value, false) })} />
        </Field>
        <Field label="To date">
          <Input type="date" value={toDate(query.dateToMs)} onChange={(e) => set({ dateToMs: fromDate(e.target.value, true) })} />
        </Field>
        <Field label="Attachments">
          <Select
            value={query.hasAttachments == null ? "any" : query.hasAttachments ? "yes" : "no"}
            onValueChange={(v) => set({ hasAttachments: v === "any" ? null : v === "yes" })}
            options={[{ value: "any", label: "Any" }, { value: "yes", label: "With attachments" }, { value: "no", label: "Without attachments" }]}
          />
        </Field>
        <Field label="Item type">
          <Select
            value={query.itemTypes[0] ?? "any"}
            onValueChange={(v) => set({ itemTypes: v === "any" ? [] : [v as SearchQuery["itemTypes"][number]] })}
            options={[
              { value: "any", label: "Any" }, { value: "mail", label: "Mail" }, { value: "appointment", label: "Appointments" }, { value: "contact", label: "Contacts" },
              { value: "task", label: "Tasks" }, { value: "note", label: "Notes" }, { value: "meetingRequest", label: "Meeting requests" },
            ]}
          />
        </Field>
        <Field label="Attachment name contains">
          <Input value={query.attachmentName ?? ""} onChange={(e) => set({ attachmentName: e.target.value || null })} placeholder="invoice.pdf" />
        </Field>
        <Field label="Importance">
          <Select
            value={query.importance == null ? "any" : String(query.importance)}
            onValueChange={(v) => set({ importance: v === "any" ? null : Number(v) })}
            options={[{ value: "any", label: "Any" }, { value: "2", label: "High" }, { value: "1", label: "Normal" }, { value: "0", label: "Low" }]}
          />
        </Field>
        <Field label="Min size (KB)">
          <Input type="number" min={0} value={query.minSize ? query.minSize / 1024 : ""} onChange={(e) => set({ minSize: e.target.value ? Number(e.target.value) * 1024 : null })} />
        </Field>
        <Field label="Max size (KB)">
          <Input type="number" min={0} value={query.maxSize ? query.maxSize / 1024 : ""} onChange={(e) => set({ maxSize: e.target.value ? Number(e.target.value) * 1024 : null })} />
        </Field>
        <div className="flex items-end gap-2">
          <label className="flex h-8 items-center gap-1.5 text-xs">
            <Checkbox checked={query.unreadOnly} onCheckedChange={(v) => set({ unreadOnly: v })} /> Unread only
          </label>
        </div>
        <div className="col-span-2 flex items-end justify-end gap-2">
          <Button size="sm" onClick={onRunHere} disabled={!folderName}>Search in {folderName ?? "folder"}</Button>
          <Button size="sm" variant="primary" onClick={onRun}>Search whole mailbox</Button>
        </div>
      </div>
      <div className="mt-2 flex flex-wrap items-center gap-2 border-t border-border pt-2 text-xs">
        <span className="text-fg-subtle">Presets</span>
        {presets.length === 0 && <span className="text-fg-subtle">none saved yet</span>}
        {presets.map((p) => (
          <span key={p.name} className="flex items-center overflow-hidden rounded-md border border-border">
            <button className="px-2 py-0.5 hover:bg-surface-3" onClick={() => onChange({ ...p.query, folder: query.folder })} title="Load this preset">
              {p.name}
            </button>
            <button className="border-l border-border px-1.5 py-0.5 text-fg-subtle hover:bg-surface-3 hover:text-danger" onClick={() => savePresets(presets.filter((x) => x.name !== p.name))} aria-label={`Delete preset ${p.name}`}>
              <X size={11} />
            </button>
          </span>
        ))}
        <div className="ml-auto flex items-center gap-1">
          <Input className="h-6 w-40 text-xs" placeholder="Save current as…" value={presetName} onChange={(e) => setPresetName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && savePreset()} />
          <Button size="sm" variant="ghost" onClick={savePreset} disabled={!presetName.trim()}>Save</Button>
        </div>
      </div>
    </div>
  );
}

export { FileJson, AlertTriangle };
