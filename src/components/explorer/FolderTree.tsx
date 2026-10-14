import { useMemo, useState } from "react";
import {
  Archive, Calendar, ChevronDown, ChevronRight, Contact, Folder, Inbox, ListTodo, Mail, NotebookPen, Search, Send, Trash2, Ban, FileEdit, BookOpen, AlertTriangle,
} from "lucide-react";
import type { FolderInfo, FolderKind } from "@/lib/api";
import { cn } from "@/lib/utils";
import { Checkbox } from "@/components/ui";

const ICONS: Record<FolderKind, React.ComponentType<{ size?: number; className?: string }>> = {
  root: Folder,
  inbox: Inbox,
  outbox: Send,
  sentItems: Send,
  deletedItems: Trash2,
  drafts: FileEdit,
  junkEmail: Ban,
  archive: Archive,
  calendar: Calendar,
  contacts: Contact,
  tasks: ListTodo,
  notes: NotebookPen,
  journal: BookOpen,
  search: Search,
  other: Folder,
};

export function FolderTree({
  folders,
  selected,
  onSelect,
  checked,
  onCheckedChange,
  filter,
}: {
  folders: FolderInfo[];
  selected: number | null;
  onSelect: (nid: number) => void;
  /// When given, every folder has a checkbox and this is the set of checked nids.
  checked?: Set<number>;
  onCheckedChange?: (nid: number, value: boolean) => void;
  filter?: string;
}) {
  const byNid = useMemo(() => new Map(folders.map((f) => [f.nid, f])), [folders]);
  const root = folders.find((f) => f.kind === "root");
  const orphans = folders.filter((f) => f.orphan && f.kind !== "root");
  const [collapsed, setCollapsed] = useState<Set<number>>(() => new Set());
  const q = (filter ?? "").trim().toLowerCase();

  // With a filter, show every folder whose name matches plus its ancestors.
  const visible = useMemo(() => {
    if (!q) return null;
    const keep = new Set<number>();
    for (const f of folders) {
      if (f.name.toLowerCase().includes(q)) {
        let cur: FolderInfo | undefined = f;
        let guard = 0;
        while (cur && guard++ < 64) {
          keep.add(cur.nid);
          if (cur.parent === cur.nid) break;
          cur = byNid.get(cur.parent);
        }
      }
    }
    return keep;
  }, [q, folders, byNid]);

  const toggle = (nid: number) =>
    setCollapsed((s) => {
      const n = new Set(s);
      if (n.has(nid)) n.delete(nid);
      else n.add(nid);
      return n;
    });

  const renderNode = (nid: number, depth: number, seen: Set<number>): React.ReactNode => {
    const f = byNid.get(nid);
    if (!f || seen.has(nid)) return null;
    seen.add(nid);
    if (visible && !visible.has(nid)) return null;
    const Icon = ICONS[f.kind] ?? Folder;
    const isOpen = !collapsed.has(nid) || !!q;
    const children = f.children.filter((c) => !visible || visible.has(c));
    const hasKids = children.length > 0;
    const isSel = selected === nid;
    // The unnamed root is never shown itself; its children are the top level.
    if (f.kind === "root" && depth === 0) {
      return children.map((c) => renderNode(c, 0, seen));
    }
    return (
      <div key={nid}>
        <div
          className={cn(
            "group flex h-7 cursor-default items-center gap-1 rounded-md pr-2 text-[13px]",
            isSel ? "bg-selection font-medium" : "hover:bg-surface-3",
            f.unreadable && "text-danger",
          )}
          style={{ paddingLeft: 6 + depth * 14 }}
          onClick={() => onSelect(nid)}
          onDoubleClick={() => hasKids && toggle(nid)}
          role="treeitem"
          aria-selected={isSel}
          aria-expanded={hasKids ? isOpen : undefined}
          tabIndex={isSel ? 0 : -1}
          onKeyDown={(e) => {
            if (e.key === "ArrowRight" && hasKids && !isOpen) toggle(nid);
            if (e.key === "ArrowLeft" && hasKids && isOpen) toggle(nid);
          }}
        >
          <button
            className={cn("flex h-4 w-4 shrink-0 items-center justify-center rounded text-fg-subtle hover:text-fg", !hasKids && "invisible")}
            onClick={(e) => {
              e.stopPropagation();
              toggle(nid);
            }}
            aria-label={isOpen ? "Collapse" : "Expand"}
            tabIndex={-1}
          >
            {isOpen ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
          </button>
          {checked && onCheckedChange && (
            <span onClick={(e) => e.stopPropagation()} className="mr-0.5 flex items-center">
              <Checkbox checked={checked.has(nid)} onCheckedChange={(v) => onCheckedChange(nid, v)} aria-label={`Select ${f.name}`} />
            </span>
          )}
          <Icon size={14} className={cn("shrink-0", isSel ? "text-accent" : "text-fg-muted")} />
          <span className="min-w-0 flex-1 truncate" title={f.path}>{f.name}</span>
          {f.unreadCount > 0 && <span className="shrink-0 text-[11px] font-semibold text-accent">{f.unreadCount}</span>}
          {f.messageCount > 0 && <span className="shrink-0 text-[11px] text-fg-subtle">{f.messageCount.toLocaleString()}</span>}
        </div>
        {hasKids && isOpen && <div role="group">{children.map((c) => renderNode(c, depth + 1, seen))}</div>}
      </div>
    );
  };

  if (!root && folders.length === 0) {
    return <p className="p-4 text-xs text-fg-muted">No folders could be read from this file.</p>;
  }
  return (
    <div role="tree" className="py-1">
      {root ? renderNode(root.nid, 0, new Set()) : folders.filter((f) => f.parent === f.nid).map((f) => renderNode(f.nid, 0, new Set()))}
      {orphans.length > 0 && (
        <div className="mt-2">
          <div className="flex items-center gap-1.5 px-2 py-1 text-[11px] font-medium uppercase tracking-wide text-warning">
            <AlertTriangle size={12} /> Not reachable from the root
          </div>
          {orphans.map((o) => renderNode(o.nid, 0, new Set()))}
        </div>
      )}
      <div className="mt-2 px-2 text-[10px] text-fg-subtle">
        <Mail size={10} className="mr-1 inline" />
        Counts are messages the index holds for each folder.
      </div>
    </div>
  );
}
