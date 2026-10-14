import { save } from "@tauri-apps/plugin-dialog";
import { AlertTriangle, CheckCircle2, FileJson, Info } from "lucide-react";
import { api, errorMessage, type OpenedMailbox } from "@/lib/api";
import { useApp } from "@/lib/store";
import { healthLabel, healthTone, hex, humanBytes } from "@/lib/utils";
import { Badge, Button, Dialog, DialogContent } from "@/components/ui";

export function FileHealthDialog({ open, onOpenChange, mailbox }: { open: boolean; onOpenChange: (o: boolean) => void; mailbox: OpenedMailbox }) {
  const info = mailbox.info;
  const toast = useApp((s) => s.toast);
  const openMailbox = useApp((s) => s.openMailbox);
  const setPage = useApp((s) => s.setPage);
  const setPendingSource = useApp((s) => s.setPendingSource);

  const exportJson = async () => {
    const path = await save({ defaultPath: info.fileName.replace(/\.[^.]+$/, "") + "-structure.json", filters: [{ name: "JSON", extensions: ["json"] }] });
    if (!path) return;
    try {
      await api.exportStructureJson(mailbox.id, path);
      toast({ tone: "success", title: "Structure exported", detail: path });
    } catch (e) {
      toast({ tone: "danger", title: "Export failed", detail: errorMessage(e) });
    }
  };

  const rows: [string, React.ReactNode][] = [
    ["Path", <span className="selectable break-all">{info.path}</span>],
    ["Type", `${info.isOst ? "OST (Outlook offline store)" : "PST (personal folders)"}`],
    ["Layout", info.format === "ansi" ? "ANSI, Outlook 97 to 2002, 32-bit ids" : info.format === "unicode4k" ? "Unicode, 4K pages, compressed blocks (Outlook 2013 and later OST)" : "Unicode, 512-byte pages"],
    ["Format version", String(info.formatVersion)],
    ["Block encoding", info.encoding],
    ["Size", `${humanBytes(info.sizeBytes)} on disk, ${humanBytes(info.declaredBytes)} declared in the header`],
    ["Allocation maps", info.amapValid ? "marked valid" : "marked invalid (the file was not closed cleanly)"],
    ["Index", info.indexSource === "header" ? "read from the header's B-trees" : "recovered by sweeping the file"],
    ["Contents", `${info.folderCount.toLocaleString()} folders, ${info.messageCount.toLocaleString()} messages, ${info.nodeCount.toLocaleString()} nodes, ${info.blockCount.toLocaleString()} blocks`],
  ];

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent title={info.fileName} description="What the file is, and what is wrong with it" wide>
        <div className="flex items-center gap-2">
          <Badge tone={healthTone[info.health]}>{healthLabel[info.health]}</Badge>
          <span className="text-xs text-fg-muted">
            {info.health === "healthy" && "Header, index and every structure read so far check out."}
            {info.health === "damaged" && "The file opened from its own index, but problems were found along the way."}
            {info.health === "recovered" && "The header's index was unusable. Contents were recovered by sweeping surviving pages and carving blocks."}
            {info.health === "critical" && "Essential structures are missing. Export is the way to get the mail out."}
          </span>
        </div>
        <table className="mt-3 w-full text-xs">
          <tbody className="divide-y divide-border">
            {rows.map(([k, v]) => (
              <tr key={k}>
                <th className="w-36 py-1.5 pr-3 text-left align-top font-medium text-fg-muted">{k}</th>
                <td className="py-1.5">{v}</td>
              </tr>
            ))}
          </tbody>
        </table>

        {info.essentialsMissing.length > 0 && (
          <div className="mt-3 flex items-start gap-2 rounded-md border border-danger/40 bg-danger-soft p-3 text-xs text-danger">
            <AlertTriangle size={14} className="mt-0.5 shrink-0" />
            <div>Missing: {info.essentialsMissing.join(", ")}. A PST written from this file will not open in Outlook.</div>
          </div>
        )}

        {info.recovery && (
          <div className="mt-3 rounded-md border border-border bg-surface-2 p-3 text-xs">
            <div className="mb-1 font-semibold">Recovery</div>
            <div className="text-fg-muted">
              {info.recovery.pagesScanned.toLocaleString()} pages swept: {info.recovery.nbtPages} node pages and {info.recovery.bbtPages} block pages survived, {info.recovery.damagedPages} failed their checksum.
              {info.recovery.carvedBlocks > 0 && ` ${info.recovery.carvedBlocks.toLocaleString()} blocks carved out of the file with no index at all.`}
              {" "}{info.recovery.sweptNodes.toLocaleString()} nodes recovered; {info.recovery.superseded} superseded entries ignored, {info.recovery.unresolved} unresolved.
            </div>
            {info.recovery.stale.length > 0 && (
              <div className="mt-2">
                <div className="font-medium text-warning">{info.recovery.stale.length} node(s) recovered at a revision that cannot be confirmed</div>
                <ul className="mt-1 max-h-32 overflow-auto font-mono text-[11px] text-fg-muted">
                  {info.recovery.stale.slice(0, 50).map((s) => (
                    <li key={s.nid}>
                      {hex(s.nid)}: {s.versions} revision(s){s.dangling ? ", data block missing" : ""}
                    </li>
                  ))}
                </ul>
                <p className="mt-1 text-fg-subtle">These read normally but may be an older copy. Nothing in the file says which.</p>
              </div>
            )}
          </div>
        )}

        <div className="mt-3">
          <div className="mb-1 flex items-center gap-1.5 text-xs font-semibold">
            {info.warnings.length === 0 ? <CheckCircle2 size={14} className="text-success" /> : <AlertTriangle size={14} className="text-warning" />}
            {info.warnings.length === 0 ? "No structural damage found" : `${info.problemCount} problem(s) found${info.problemCount > info.warnings.length ? `, ${info.warnings.length} listed` : ""}`}
          </div>
          {info.warnings.length > 0 && (
            <ul className="max-h-48 overflow-auto rounded-md border border-border bg-surface-2 p-2 text-[11px] selectable">
              {info.warnings.map((w, i) => (
                <li key={i} className="py-0.5">{w}</li>
              ))}
            </ul>
          )}
        </div>

        <div className="mt-3 rounded-md border border-border p-3 text-xs">
          <div className="mb-1 font-semibold">Node types</div>
          <div className="grid grid-cols-2 gap-x-4 gap-y-0.5 font-mono text-[11px] text-fg-muted md:grid-cols-3">
            {info.nodeTypes.map((t) => (
              <div key={t.nidType} className="flex justify-between gap-2">
                <span className="truncate">0x{t.nidType.toString(16).padStart(2, "0")} {t.name}</span>
                <span>{t.count.toLocaleString()}</span>
              </div>
            ))}
          </div>
        </div>

        <div className="mt-4 flex flex-wrap items-center gap-2">
          <Button size="sm" onClick={exportJson}><FileJson size={13} /> Export structure as JSON</Button>
          <Button
            size="sm"
            onClick={() => {
              setPendingSource(info.path);
              setPage("repair");
              onOpenChange(false);
            }}
          >
            <Info size={13} /> Verify or repair
          </Button>
          {info.indexSource === "header" && (
            <Button
              size="sm"
              variant="ghost"
              onClick={async () => {
                onOpenChange(false);
                try {
                  await openMailbox(info.path, true);
                  toast({ tone: "info", title: "Reopened in salvage mode", detail: "The header's index was ignored and rebuilt by sweeping. Deleted items whose entries survived may now appear." });
                } catch (e) {
                  toast({ tone: "danger", title: "Salvage open failed", detail: errorMessage(e) });
                }
              }}
            >
              Reopen in salvage mode
            </Button>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
