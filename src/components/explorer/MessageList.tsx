import { useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDown, ArrowUp, Calendar, Contact, ListTodo, NotebookPen, Paperclip, Mail, AlertCircle, Flag, FileWarning } from "lucide-react";
import type { ItemType, MessageSummary, SortKey } from "@/lib/api";
import { cn, formatDate, humanBytes } from "@/lib/utils";
import { EmptyState, Spinner } from "@/components/ui";

const ROW = 56;

const TYPE_ICON: Partial<Record<ItemType, React.ComponentType<{ size?: number; className?: string }>>> = {
  appointment: Calendar,
  meetingRequest: Calendar,
  contact: Contact,
  distributionList: Contact,
  task: ListTodo,
  note: NotebookPen,
};

export interface ListState {
  sort: SortKey;
  descending: boolean;
}

export function MessageList({
  items,
  total,
  loading,
  selected,
  onSelect,
  onOpen,
  onContextMenu,
  sort,
  onSortChange,
  onLoadMore,
  hasMore,
  unreadable,
  emptyTitle,
  emptyDescription,
  matchedOf,
}: {
  items: MessageSummary[];
  total: number;
  loading: boolean;
  selected: number[];
  onSelect: (nids: number[], anchor?: number) => void;
  onOpen?: (nid: number) => void;
  onContextMenu?: (e: React.MouseEvent, nid: number) => void;
  sort: ListState;
  onSortChange: (s: ListState) => void;
  onLoadMore?: () => void;
  hasMore?: boolean;
  unreadable?: number;
  emptyTitle?: string;
  emptyDescription?: string;
  matchedOf?: (nid: number) => string | undefined;
}) {
  const parentRef = useRef<HTMLDivElement>(null);
  const [anchor, setAnchor] = useState<number | null>(null);
  const selectedSet = useMemo(() => new Set(selected), [selected]);
  const virt = useVirtualizer({
    count: items.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW,
    overscan: 12,
  });

  useEffect(() => {
    const last = virt.getVirtualItems().at(-1);
    if (last && hasMore && !loading && onLoadMore && last.index >= items.length - 20) onLoadMore();
  }, [virt.getVirtualItems(), hasMore, loading, onLoadMore, items.length]); // eslint-disable-line react-hooks/exhaustive-deps

  const click = (e: React.MouseEvent, idx: number) => {
    const nid = items[idx].nid;
    if (e.shiftKey && anchor != null) {
      const a = items.findIndex((m) => m.nid === anchor);
      const [lo, hi] = a < idx ? [a, idx] : [idx, a];
      onSelect(items.slice(Math.max(0, lo), hi + 1).map((m) => m.nid), anchor);
    } else if (e.ctrlKey || e.metaKey) {
      const n = new Set(selectedSet);
      if (n.has(nid)) n.delete(nid);
      else n.add(nid);
      onSelect([...n], nid);
      setAnchor(nid);
    } else {
      onSelect([nid], nid);
      setAnchor(nid);
    }
  };

  const keyNav = (e: React.KeyboardEvent) => {
    if (items.length === 0) return;
    const cur = anchor != null ? items.findIndex((m) => m.nid === anchor) : -1;
    let next = cur;
    if (e.key === "ArrowDown") next = Math.min(items.length - 1, cur + 1);
    else if (e.key === "ArrowUp") next = Math.max(0, cur - 1);
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = items.length - 1;
    else if (e.key === "PageDown") next = Math.min(items.length - 1, cur + 10);
    else if (e.key === "PageUp") next = Math.max(0, cur - 10);
    else if (e.key === "a" && e.ctrlKey) {
      e.preventDefault();
      onSelect(items.map((m) => m.nid));
      return;
    } else if (e.key === "Enter" && anchor != null) {
      onOpen?.(anchor);
      return;
    } else return;
    e.preventDefault();
    if (next < 0) next = 0;
    const nid = items[next].nid;
    if (e.shiftKey && anchor != null) {
      const a = items.findIndex((m) => m.nid === anchor);
      const [lo, hi] = a < next ? [a, next] : [next, a];
      onSelect(items.slice(lo, hi + 1).map((m) => m.nid), anchor);
    } else {
      onSelect([nid], nid);
      setAnchor(nid);
    }
    virt.scrollToIndex(next, { align: "auto" });
  };

  const Header = ({ k, label, className }: { k: SortKey; label: string; className?: string }) => (
    <button
      className={cn("flex items-center gap-1 text-[11px] font-medium text-fg-muted hover:text-fg", className)}
      onClick={() => onSortChange({ sort: k, descending: sort.sort === k ? !sort.descending : k === "date" })}
    >
      {label}
      {sort.sort === k && (sort.descending ? <ArrowDown size={11} /> : <ArrowUp size={11} />)}
    </button>
  );

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-8 shrink-0 items-center gap-3 border-b border-border bg-surface-2 px-3">
        <Header k="sender" label="From" className="w-40" />
        <Header k="subject" label="Subject" className="flex-1" />
        <Header k="size" label="Size" className="w-16 justify-end" />
        <Header k="date" label="Date" className="w-20 justify-end" />
      </div>
      {items.length === 0 && !loading ? (
        <EmptyState compact icon={<Mail size={20} />} title={emptyTitle ?? "No messages"} description={emptyDescription ?? "This folder holds no messages the index can see."} />
      ) : (
        <div ref={parentRef} className="min-h-0 flex-1 overflow-auto outline-none" tabIndex={0} onKeyDown={keyNav} role="listbox" aria-multiselectable>
          <div style={{ height: virt.getTotalSize(), position: "relative" }}>
            {virt.getVirtualItems().map((v) => {
              const m = items[v.index];
              const isSel = selectedSet.has(m.nid);
              const Icon = TYPE_ICON[m.itemType];
              const matched = matchedOf?.(m.nid);
              return (
                <div
                  key={m.nid}
                  role="option"
                  aria-selected={isSel}
                  onClick={(e) => click(e, v.index)}
                  onDoubleClick={() => onOpen?.(m.nid)}
                  onContextMenu={(e) => {
                    if (!isSel) {
                      onSelect([m.nid], m.nid);
                      setAnchor(m.nid);
                    }
                    onContextMenu?.(e, m.nid);
                  }}
                  data-context-menu
                  className={cn(
                    "absolute left-0 flex w-full cursor-default items-center gap-3 border-b border-border px-3",
                    isSel ? "bg-selection" : "hover:bg-surface-2",
                    !m.read && "font-semibold",
                  )}
                  style={{ top: v.start, height: v.size }}
                >
                  <span className={cn("w-1 shrink-0 self-stretch rounded-full", !m.read ? "bg-accent" : "bg-transparent")} />
                  <div className="w-40 min-w-0 shrink-0">
                    <div className="truncate text-[13px]" title={m.senderEmail || m.senderName}>{m.senderName || m.senderEmail || <span className="text-fg-subtle">Unknown sender</span>}</div>
                    <div className="truncate text-[11px] font-normal text-fg-subtle">{m.displayTo ? `To: ${m.displayTo}` : m.senderEmail}</div>
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-1.5">
                      {Icon && <Icon size={12} className="shrink-0 text-fg-muted" />}
                      {m.importance === 2 && <AlertCircle size={12} className="shrink-0 text-danger" aria-label="High importance" />}
                      {m.importance === 0 && <Flag size={12} className="shrink-0 text-fg-subtle" aria-label="Low importance" />}
                      <span className="truncate text-[13px]">{m.subject}</span>
                      {m.subject.startsWith("(unreadable") && <FileWarning size={12} className="shrink-0 text-danger" />}
                    </div>
                    <div className="truncate text-[11px] font-normal text-fg-subtle">
                      {matched ? <span className="text-accent">matched in {matched}</span> : m.messageClass || ""}
                    </div>
                  </div>
                  <div className="flex w-16 shrink-0 items-center justify-end gap-1 text-[11px] font-normal text-fg-muted">
                    {m.hasAttachments && <Paperclip size={12} />}
                    {m.size > 0 ? humanBytes(m.size) : ""}
                  </div>
                  <div className="w-20 shrink-0 text-right text-[11px] font-normal text-fg-muted" title={m.dateMs ? new Date(m.dateMs).toLocaleString() : ""}>
                    {formatDate(m.dateMs)}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}
      <div className="flex h-7 shrink-0 items-center gap-2 border-t border-border bg-surface-2 px-3 text-[11px] text-fg-muted">
        {loading && <Spinner size={12} />}
        <span>
          {items.length.toLocaleString()} of {total.toLocaleString()} shown
          {selected.length > 1 && ` · ${selected.length.toLocaleString()} selected`}
          {unreadable ? ` · ${unreadable} unreadable` : ""}
        </span>
      </div>
    </div>
  );
}
