import { FileDown, FileX, Layers, FolderOpen } from "lucide-react";
import { useApp } from "@/lib/store";
import { cn, fileName, pluralize } from "@/lib/utils";
import { Button, Dialog, DialogContent, DialogFooter } from "@/components/ui";

/// A full-window hint while files are dragged over the application, saying what will
/// happen on drop. The drop itself is handled in App.tsx.
export function DropOverlay() {
  const drag = useApp((s) => s.drag);
  const page = useApp((s) => s.page);
  if (!drag) return null;
  const ok = drag.supported.length;
  const bad = drag.unsupported.length;
  const asSource = ok === 1 && (page === "convert" || page === "export" || page === "repair");
  const title =
    ok === 0 ? (bad === 1 ? "This is not an OST or PST file" : "These are not OST or PST files") :
    ok === 1 ? (asSource ? `Use ${fileName(drag.supported[0])} as the source` : `Open ${fileName(drag.supported[0])}`) :
    `Drop ${ok} mailboxes`;
  const detail =
    ok === 0 ? "Only .pst and .ost files can be opened." :
    ok > 1 ? "You can open them in the explorer or add them to a batch." :
    "The original file is only read, never modified.";
  return (
    <div className={cn("pointer-events-none fixed inset-0 z-50 flex items-center justify-center backdrop-blur-[1px]", ok ? "bg-accent/10" : "bg-danger/5")}>
      <div className={cn("flex max-w-md flex-col items-center gap-2 rounded-xl border-2 border-dashed bg-surface px-10 py-8 text-center shadow-2xl", ok ? "border-accent" : "border-danger/60")}>
        {ok ? <FileDown size={28} className="text-accent" /> : <FileX size={28} className="text-danger" />}
        <p className="text-sm font-semibold">{title}</p>
        <p className="text-xs text-fg-muted">{detail}</p>
        {ok > 0 && bad > 0 && <p className="text-xs text-warning">{pluralize(bad, "other file")} will be ignored.</p>}
      </div>
    </div>
  );
}

/// Several mailboxes dropped at once: open them all, or queue them for a batch job. Nothing
/// is started automatically either way.
export function MultiDropDialog({ onOpenAll }: { onOpenAll: (paths: string[]) => void }) {
  const paths = useApp((s) => s.multiDrop);
  const setPaths = useApp((s) => s.setMultiDrop);
  const setBatch = useApp((s) => s.setPendingBatchFiles);
  const setPage = useApp((s) => s.setPage);
  if (!paths) return null;
  return (
    <Dialog open onOpenChange={(o) => !o && setPaths(null)}>
      <DialogContent title={`${paths.length} mailboxes dropped`} description="Choose what to do with them. Nothing starts until you say so.">
        <ul className="max-h-48 space-y-0.5 overflow-auto rounded-md border border-border bg-surface-2 px-3 py-2 text-xs">
          {paths.map((p) => (
            <li key={p} className="truncate" title={p}>{fileName(p)}</li>
          ))}
        </ul>
        <DialogFooter>
          <Button onClick={() => setPaths(null)}>Cancel</Button>
          <Button onClick={() => { setBatch(paths); setPaths(null); setPage("batch"); }}>
            <Layers size={14} /> Add to batch
          </Button>
          <Button variant="primary" autoFocus onClick={() => { setPaths(null); onOpenAll(paths); }}>
            <FolderOpen size={14} /> Open all
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
