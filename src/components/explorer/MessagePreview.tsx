import { useEffect, useMemo, useState } from "react";
import DOMPurify from "dompurify";
import { save, open as openDialog } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { Code2, Download, FileText, Globe, Image as ImageIcon, Mail, Paperclip, Shield, ShieldAlert, Braces, AlertTriangle, FolderDown } from "lucide-react";
import { api, errorMessage, type AttachmentInfo, type AttachmentPreview, type MessageDetail } from "@/lib/api";
import { useApp } from "@/lib/store";
import { cn, formatDate, hex, humanBytes, itemTypeLabel } from "@/lib/utils";
import { Badge, Button, Dialog, DialogContent, EmptyState, Spinner, Tabs, TabsContent, TabsList, TabsTrigger, Tooltip } from "@/components/ui";

/// The HTML body is shown in a sandboxed iframe: no scripts, no forms, no navigation,
/// and a Content-Security-Policy inside the frame that blocks every remote fetch unless
/// the user allows remote content for this message.
function buildFrameDoc(html: string, allowRemote: boolean, dark: boolean): string {
  const clean = DOMPurify.sanitize(html, {
    USE_PROFILES: { html: true },
    FORBID_TAGS: ["script", "iframe", "object", "embed", "form", "input", "button", "meta", "link", "base", "svg", "math"],
    FORBID_ATTR: ["onerror", "onload", "formaction", "ping", "srcdoc"],
    ALLOW_UNKNOWN_PROTOCOLS: false,
  });
  const csp = allowRemote
    ? "default-src 'none'; img-src * data: cid:; style-src 'unsafe-inline'; font-src *;"
    : "default-src 'none'; img-src data: cid:; style-src 'unsafe-inline';";
  const base = dark ? "color:#f3f3f3;background:#272727" : "color:#1b1b1b;background:#ffffff";
  return `<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="${csp}"><style>html,body{margin:0;padding:12px 16px;font:14px/1.5 'Segoe UI',system-ui,sans-serif;${base};word-wrap:break-word;overflow-wrap:anywhere}img{max-width:100%;height:auto}a{color:#0f6cbd}blockquote{border-left:3px solid #999;margin:0;padding-left:10px;color:#666}pre{white-space:pre-wrap}table{max-width:100%}</style></head><body>${clean}</body></html>`;
}

export function MessagePreview({ mailboxId, nid, onOpenEmbedded }: { mailboxId: string; nid: number | null; onOpenEmbedded?: (d: MessageDetail) => void }) {
  const [detail, setDetail] = useState<MessageDetail | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [allowRemote, setAllowRemote] = useState(false);
  const [tab, setTab] = useState<string>("body");
  const [attachment, setAttachment] = useState<AttachmentInfo | null>(null);
  const settings = useApp((s) => s.settings);
  const toast = useApp((s) => s.toast);
  const dark = document.documentElement.classList.contains("dark");

  useEffect(() => {
    setAllowRemote(false);
    if (nid == null) {
      setDetail(null);
      return;
    }
    let live = true;
    setLoading(true);
    setError(null);
    api
      .getMessage(mailboxId, nid)
      .then((d) => live && setDetail(d))
      .catch((e) => live && setError(errorMessage(e)))
      .finally(() => live && setLoading(false));
    return () => {
      live = false;
    };
  }, [mailboxId, nid]);

  const frameDoc = useMemo(() => {
    if (!detail?.bodyHtml) return null;
    return buildFrameDoc(detail.bodyHtml, allowRemote && !(settings?.blockRemoteContent ?? true) ? true : allowRemote, dark);
  }, [detail, allowRemote, dark, settings]);

  const hasRemote = useMemo(() => !!detail?.bodyHtml && /\b(src|href)\s*=\s*["']?\s*https?:/i.test(detail.bodyHtml), [detail]);

  if (nid == null) {
    return <EmptyState icon={<Mail size={22} />} title="No message selected" description="Select a message to read it here. Bodies are rendered in a sandbox with scripts disabled and remote content blocked." />;
  }
  if (loading && !detail) {
    return (
      <div className="flex h-full items-center justify-center gap-2 text-xs text-fg-muted">
        <Spinner /> Reading message…
      </div>
    );
  }
  if (error) {
    return <EmptyState icon={<AlertTriangle size={22} />} title="This message could not be read" description={error} />;
  }
  if (!detail) return null;
  const s = detail.summary;

  const saveAttachment = async (a: AttachmentInfo) => {
    try {
      const dest = await save({ defaultPath: a.name, title: "Save attachment" });
      if (!dest) return;
      const r = await api.saveAttachment(mailboxId, nid, a.nid, dest);
      toast({ tone: "success", title: "Attachment saved", detail: r.path });
    } catch (e) {
      toast({ tone: "danger", title: "Could not save the attachment", detail: errorMessage(e) });
    }
  };
  const saveAll = async () => {
    try {
      const dir = await openDialog({ directory: true, title: "Save all attachments to" });
      if (!dir || Array.isArray(dir)) return;
      const r = await api.extractAttachments(mailboxId, [nid], dir);
      toast({ tone: r.failed.length ? "warning" : "success", title: `${r.written.length} attachment(s) saved`, detail: r.failed.join("\n") || dir });
      if (r.written.length) void revealItemInDir(r.written[0].path);
    } catch (e) {
      toast({ tone: "danger", title: "Could not save attachments", detail: errorMessage(e) });
    }
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="shrink-0 border-b border-border px-4 pt-3 pb-2 selectable">
        <div className="flex items-start gap-2">
          <h2 className="min-w-0 flex-1 text-[15px] font-semibold leading-snug">{s.subject}</h2>
          <Badge tone="neutral">{itemTypeLabel[s.itemType]}</Badge>
        </div>
        <dl className="mt-2 grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 text-xs">
          {(s.senderName || s.senderEmail) && <dt className="text-fg-subtle">From</dt>}
          {(s.senderName || s.senderEmail) && <dd className="truncate">
            {s.senderName}
            {s.senderEmail && <span className="text-fg-muted"> &lt;{s.senderEmail}&gt;</span>}
            {detail.senderRepresentingName && detail.senderRepresentingName !== s.senderName && (
              <span className="text-fg-muted"> on behalf of {detail.senderRepresentingName}</span>
            )}
          </dd>}
          {[1, 2, 3].map((k) => {
            const list = detail.recipients.filter((r) => r.kind === k);
            const label = k === 1 ? "To" : k === 2 ? "Cc" : "Bcc";
            if (list.length === 0 && !(k === 1 && s.displayTo)) return null;
            return (
              <RecipientRow key={k} label={label} value={list.length ? list.map((r) => (r.email && r.name ? `${r.name} <${r.email}>` : r.email || r.name)).join(", ") : s.displayTo} />
            );
          })}
          <dt className="text-fg-subtle">Date</dt>
          <dd>
            {formatDate(detail.receivedMs ?? detail.sentMs ?? s.dateMs, "long") || <span className="text-fg-subtle">no timestamp in the file</span>}
            {detail.sentMs && detail.receivedMs && detail.sentMs !== detail.receivedMs && (
              <span className="text-fg-muted"> · sent {formatDate(detail.sentMs, "long")}</span>
            )}
          </dd>
          {detail.extra.map(([k, v]) => (
            <RecipientRow key={k} label={k} value={v} />
          ))}
        </dl>
        {detail.attachments.length > 0 && (
          <div className="mt-2 flex flex-wrap items-center gap-1.5">
            {detail.attachments.map((a) => (
              <button
                key={a.nid}
                onClick={() => setAttachment(a)}
                className="flex max-w-[260px] items-center gap-1.5 rounded-md border border-border bg-surface-2 px-2 py-1 text-xs hover:bg-surface-3"
                title={`${a.name} · ${a.mime}`}
              >
                {a.isEmbeddedMessage ? <Mail size={12} /> : a.mime.startsWith("image/") ? <ImageIcon size={12} /> : <Paperclip size={12} />}
                <span className="truncate">{a.name}</span>
                <span className="shrink-0 text-fg-subtle">{humanBytes(a.size)}</span>
              </button>
            ))}
            {detail.attachments.filter((a) => !a.isEmbeddedMessage).length > 1 && (
              <Button size="sm" variant="ghost" onClick={saveAll}>
                <FolderDown size={13} /> Save all
              </Button>
            )}
          </div>
        )}
        {detail.problems.length > 0 && (
          <div className="mt-2 rounded-md border border-warning/40 bg-warning-soft px-2.5 py-1.5 text-xs text-warning">
            {detail.problems.map((p, i) => (
              <div key={i}>{p}</div>
            ))}
          </div>
        )}
      </div>

      <Tabs value={tab} onValueChange={setTab} className="flex min-h-0 flex-1 flex-col">
        <div className="flex shrink-0 items-center justify-between border-b border-border px-2">
          <TabsList className="border-0">
            <TabsTrigger value="body">Message</TabsTrigger>
            {detail.bodyHtml && detail.bodyText && <TabsTrigger value="text">Plain text</TabsTrigger>}
            {detail.transportHeaders && <TabsTrigger value="headers">Headers</TabsTrigger>}
            <TabsTrigger value="props">Properties ({detail.properties.length})</TabsTrigger>
          </TabsList>
          {detail.bodyHtml && hasRemote && (
            <Tooltip content={allowRemote ? "Remote images are loading for this message" : "Remote images are blocked. Loading them tells the sender's server you opened this message."}>
              <Button size="sm" variant={allowRemote ? "subtle" : "ghost"} onClick={() => setAllowRemote((v) => !v)}>
                {allowRemote ? <ShieldAlert size={13} className="text-warning" /> : <Shield size={13} />}
                {allowRemote ? "Remote content on" : "Load remote content"}
              </Button>
            </Tooltip>
          )}
        </div>
        <TabsContent value="body" className="min-h-0 flex-1 overflow-hidden data-[state=inactive]:hidden">
          {detail.bodyHtml && (settings?.showHtmlBodies ?? true) ? (
            <iframe
              title="Message body"
              sandbox=""
              referrerPolicy="no-referrer"
              srcDoc={frameDoc ?? ""}
              className="h-full w-full border-0 bg-surface"
            />
          ) : detail.bodyText ? (
            <pre className="h-full overflow-auto whitespace-pre-wrap break-words px-4 py-3 font-sans text-[13px] selectable">{detail.bodyText}</pre>
          ) : (
            <EmptyState compact icon={<FileText size={18} />} title="No body" description="This item has no text or HTML body." />
          )}
        </TabsContent>
        <TabsContent value="text" className="min-h-0 flex-1 overflow-auto data-[state=inactive]:hidden">
          <pre className="whitespace-pre-wrap break-words px-4 py-3 font-sans text-[13px] selectable">{detail.bodyText}</pre>
        </TabsContent>
        <TabsContent value="headers" className="min-h-0 flex-1 overflow-auto data-[state=inactive]:hidden">
          <pre className="whitespace-pre-wrap break-all px-4 py-3 font-mono text-[11px] selectable">{detail.transportHeaders}</pre>
          {detail.internetMessageId && <div className="px-4 pb-3 text-xs text-fg-muted selectable">Message-ID: {detail.internetMessageId}</div>}
        </TabsContent>
        <TabsContent value="props" className="min-h-0 flex-1 overflow-auto data-[state=inactive]:hidden">
          <PropertyTable properties={detail.properties} nid={nid} />
        </TabsContent>
      </Tabs>

      <Dialog open={!!attachment} onOpenChange={(o) => !o && setAttachment(null)}>
        {attachment && (
          <DialogContent title={attachment.name} description={`${attachment.mime} · ${humanBytes(attachment.size)}${attachment.isEmbeddedMessage ? " · embedded message" : ""}`} wide>
            <AttachmentView mailboxId={mailboxId} nid={nid} a={attachment} onSave={() => saveAttachment(attachment)} onOpenEmbedded={onOpenEmbedded} />
          </DialogContent>
        )}
      </Dialog>
    </div>
  );
}

function RecipientRow({ label, value }: { label: string; value: string }) {
  const [expanded, setExpanded] = useState(false);
  return (
    <>
      <dt className="text-fg-subtle">{label}</dt>
      <dd className={cn(!expanded && "truncate", "cursor-text")} onClick={() => setExpanded((v) => !v)} title={value}>
        {value}
      </dd>
    </>
  );
}

export function PropertyTable({ properties, nid }: { properties: MessageDetail["properties"]; nid: number }) {
  const [filter, setFilter] = useState("");
  const q = filter.toLowerCase();
  const rows = properties.filter((p) => !q || (p.name ?? "").toLowerCase().includes(q) || hex(p.tag).toLowerCase().includes(q) || p.value.toLowerCase().includes(q));
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-border px-3 py-1.5 text-xs">
        <Braces size={13} className="text-fg-muted" />
        <span className="text-fg-muted">Node {hex(nid)} · {properties.length} properties as stored</span>
        <input className="ml-auto h-6 w-48 rounded border border-border-strong bg-surface px-2 text-xs selectable" placeholder="Filter" value={filter} onChange={(e) => setFilter(e.target.value)} />
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        <table className="ring-table w-full text-[11px] selectable">
          <thead className="text-left text-fg-subtle">
            <tr>
              <th className="px-3 py-1 font-medium">Tag</th>
              <th className="px-2 py-1 font-medium">Name</th>
              <th className="px-2 py-1 font-medium">Type</th>
              <th className="px-2 py-1 font-medium">Value</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-border font-mono">
            {rows.map((p) => (
              <tr key={p.tag} className="align-top hover:bg-surface-2">
                <td className="whitespace-nowrap px-3 py-1 text-fg-muted">0x{p.tag.toString(16).toUpperCase().padStart(4, "0")}</td>
                <td className="whitespace-nowrap px-2 py-1 font-sans">{p.name ?? <span className="text-fg-subtle">(unnamed)</span>}</td>
                <td className="whitespace-nowrap px-2 py-1 font-sans text-fg-muted">{p.typeName}</td>
                <td className="max-w-[520px] break-all px-2 py-1">{p.value.length > 400 ? p.value.slice(0, 400) + "…" : p.value}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function AttachmentView({ mailboxId, nid, a, onSave, onOpenEmbedded }: { mailboxId: string; nid: number; a: AttachmentInfo; onSave: () => void; onOpenEmbedded?: (d: MessageDetail) => void }) {
  const [preview, setPreview] = useState<AttachmentPreview | null>(null);
  const [embedded, setEmbedded] = useState<MessageDetail | null>(null);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => {
    if (a.isEmbeddedMessage) {
      api.getEmbeddedMessage(mailboxId, nid, a.nid).then(setEmbedded).catch((e) => setErr(errorMessage(e)));
    } else {
      api.attachmentPreview(mailboxId, nid, a.nid).then(setPreview).catch((e) => setErr(errorMessage(e)));
    }
  }, [mailboxId, nid, a]);

  if (err) return <p className="text-xs text-danger">{err}</p>;
  if (a.isEmbeddedMessage) {
    if (!embedded) return <Spinner />;
    return (
      <div className="text-xs">
        <p className="font-semibold">{embedded.summary.subject}</p>
        <p className="text-fg-muted">From {embedded.summary.senderName || embedded.summary.senderEmail || "unknown"} · {formatDate(embedded.summary.dateMs, "long")}</p>
        <pre className="mt-3 max-h-[50vh] overflow-auto whitespace-pre-wrap rounded-md bg-surface-2 p-3 font-sans selectable">
          {embedded.bodyText ?? (embedded.bodyHtml ? DOMPurify.sanitize(embedded.bodyHtml, { ALLOWED_TAGS: [] }) : "(no body)")}
        </pre>
        {onOpenEmbedded && (
          <Button className="mt-3" onClick={() => onOpenEmbedded(embedded)}>
            <Code2 size={13} /> Show properties
          </Button>
        )}
      </div>
    );
  }
  if (!preview) return <Spinner />;
  return (
    <div className="flex flex-col gap-3">
      {preview.base64 ? (
        <img src={`data:${preview.mime};base64,${preview.base64}`} alt={a.name} className="max-h-[55vh] w-auto self-center rounded border border-border object-contain" />
      ) : preview.text != null ? (
        <pre className="max-h-[55vh] overflow-auto whitespace-pre-wrap rounded-md bg-surface-2 p-3 font-mono text-[11px] selectable">{preview.text}</pre>
      ) : (
        <div className="rounded-md bg-surface-2 p-4 text-xs text-fg-muted">
          {preview.tooLarge ? "Too large to preview here. Save it to open it with another program." : "No preview for this type. Save it to open it with another program. Attachments are never executed by OST/PST Studio."}
        </div>
      )}
      <div className="flex items-center justify-between text-[11px] text-fg-muted">
        <span className="selectable">SHA-256: {preview.info.sha256 ?? "-"}</span>
        <div className="flex items-center gap-2">
          {a.mime.startsWith("image/") && <Globe size={12} />}
          <Button variant="primary" size="sm" onClick={onSave}>
            <Download size={13} /> Save as…
          </Button>
        </div>
      </div>
    </div>
  );
}
