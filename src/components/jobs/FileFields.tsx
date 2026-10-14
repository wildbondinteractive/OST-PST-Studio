import { useEffect, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { AlertTriangle, CheckCircle2, FileSearch, FolderOpen, HardDrive } from "lucide-react";
import { api, type PathInfo, type ProbeResult } from "@/lib/api";
import { cn, humanBytes } from "@/lib/utils";
import { Button, Field, Input, Spinner } from "@/components/ui";

/// A source file field with a browse button and a live probe of what the file is.
export function SourceField({ value, onChange, probe, onProbe, accept = ["pst", "ost"], label = "Source file" }: {
  value: string;
  onChange: (v: string) => void;
  probe: ProbeResult | null;
  onProbe: (p: ProbeResult | null) => void;
  accept?: string[];
  label?: string;
}) {
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (!value.trim()) {
      onProbe(null);
      return;
    }
    let live = true;
    setBusy(true);
    const t = setTimeout(() => {
      api.probeFile(value).then((p) => live && onProbe(p)).catch(() => live && onProbe(null)).finally(() => live && setBusy(false));
    }, 250);
    return () => {
      live = false;
      clearTimeout(t);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value]);

  const browse = async () => {
    const picked = await open({ multiple: false, filters: [{ name: "Outlook data files", extensions: accept }, { name: "All files", extensions: ["*"] }] });
    if (typeof picked === "string") onChange(picked);
  };
  return (
    <Field label={label}>
      <div className="flex gap-2">
        <Input value={value} onChange={(e) => onChange(e.target.value)} placeholder="C:\path\to\mailbox.ost" spellCheck={false} />
        <Button onClick={browse}><FolderOpen size={14} /> Browse</Button>
      </div>
      <div className="min-h-[18px] text-xs">
        {busy && <span className="flex items-center gap-1 text-fg-subtle"><Spinner size={11} /> Checking…</span>}
        {!busy && probe && probe.exists && probe.isPstOrOst && (
          <span className="flex items-center gap-1 text-success">
            <CheckCircle2 size={12} />
            {probe.isOst ? "OST" : "PST"}, {probe.format === "ansi" ? "ANSI (Outlook 97-2002)" : probe.format === "unicode4k" ? "Outlook 2013+ 4K layout" : "Unicode"}, {humanBytes(probe.sizeBytes)}
            {probe.warnings.length > 0 && <span className="text-warning"> · {probe.warnings.length} header warning(s)</span>}
          </span>
        )}
        {!busy && probe && (!probe.exists || !probe.isPstOrOst) && (
          <span className="flex items-center gap-1 text-danger"><AlertTriangle size={12} /> {probe.error ?? "Not a PST or OST file"}</span>
        )}
      </div>
    </Field>
  );
}

/// An output path field: a file (with extension) or a directory.
export function OutputField({ value, onChange, kind, extension, label, hint }: {
  value: string;
  onChange: (v: string) => void;
  kind: "file" | "dir";
  extension?: string;
  label?: string;
  hint?: string;
}) {
  const [info, setInfo] = useState<PathInfo | null>(null);
  useEffect(() => {
    if (!value.trim()) {
      setInfo(null);
      return;
    }
    let live = true;
    const t = setTimeout(() => api.pathInfo(value).then((i) => live && setInfo(i)).catch(() => live && setInfo(null)), 250);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [value]);
  const browse = async () => {
    if (kind === "dir") {
      const d = await open({ directory: true, multiple: false, defaultPath: value || undefined });
      if (typeof d === "string") onChange(d);
    } else {
      const f = await save({ defaultPath: value || undefined, filters: extension ? [{ name: extension.toUpperCase(), extensions: [extension] }] : undefined });
      if (f) onChange(f);
    }
  };
  return (
    <Field label={label ?? (kind === "dir" ? "Output folder" : "Output file")} hint={hint}>
      <div className="flex gap-2">
        <Input value={value} onChange={(e) => onChange(e.target.value)} spellCheck={false} placeholder={kind === "dir" ? "C:\\exports\\mailbox" : "C:\\converted\\mailbox.pst"} />
        <Button onClick={browse}><FileSearch size={14} /> Choose</Button>
      </div>
      <div className="min-h-[18px] text-xs">
        {info && (
          <span className={cn("flex items-center gap-1", info.exists && kind === "file" ? "text-warning" : "text-fg-subtle")}>
            {info.exists && kind === "file" && <><AlertTriangle size={12} /> A file with this name already exists; the collision setting decides what happens.</>}
            {info.exists && kind === "dir" && info.isEmptyDir === false && <><AlertTriangle size={12} className="text-warning" /> The folder is not empty; new files are written beside what is there.</>}
            {info.freeSpace != null && <span className="ml-2 flex items-center gap-1"><HardDrive size={11} /> {humanBytes(info.freeSpace)} free</span>}
          </span>
        )}
      </div>
    </Field>
  );
}
