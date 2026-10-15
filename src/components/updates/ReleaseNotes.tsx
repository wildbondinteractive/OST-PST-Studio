// Release notes come from the release's Markdown (docs/RELEASE_NOTES.md). They are rendered
// with a deliberately small reader: headings, bullets, paragraphs, bold and inline code.
// Everything becomes React text nodes, so nothing in the notes can inject markup, and links
// show their text only; the "View on GitHub" button is the way to the full page.

import { Fragment } from "react";

function inline(text: string, key: string) {
  // [label](url) -> label; then **bold** and `code`.
  const plain = text.replace(/\[([^\]]+)\]\([^)]*\)/g, "$1");
  const parts = plain.split(/(\*\*[^*]+\*\*|`[^`]+`)/g).filter(Boolean);
  return parts.map((p, i) => {
    if (p.startsWith("**") && p.endsWith("**")) return <strong key={`${key}-${i}`}>{p.slice(2, -2)}</strong>;
    if (p.startsWith("`") && p.endsWith("`")) return <code key={`${key}-${i}`} className="rounded bg-surface-3 px-1 font-mono text-[11px]">{p.slice(1, -1)}</code>;
    return <Fragment key={`${key}-${i}`}>{p}</Fragment>;
  });
}

type Block = { kind: "h"; level: number; text: string } | { kind: "p"; text: string } | { kind: "ul"; items: string[] };

function parse(md: string): Block[] {
  const blocks: Block[] = [];
  let para: string[] = [];
  let list: string[] | null = null;
  const flush = () => {
    if (para.length) blocks.push({ kind: "p", text: para.join(" ") });
    para = [];
    if (list) blocks.push({ kind: "ul", items: list });
    list = null;
  };
  for (const raw of md.replace(/\r\n/g, "\n").split("\n")) {
    const line = raw.trimEnd();
    const h = /^(#{1,4})\s+(.*)$/.exec(line);
    const li = /^\s*[-*]\s+(.*)$/.exec(line);
    if (!line.trim()) {
      flush();
    } else if (h) {
      flush();
      blocks.push({ kind: "h", level: h[1].length, text: h[2] });
    } else if (li) {
      if (para.length) {
        blocks.push({ kind: "p", text: para.join(" ") });
        para = [];
      }
      list ??= [];
      list.push(li[1]);
    } else if (list && /^\s{2,}\S/.test(raw)) {
      list[list.length - 1] += " " + line.trim();
    } else if (/^```/.test(line)) {
      // Code fences (the checksum block) are not useful inside the app.
      continue;
    } else {
      if (list) flush();
      para.push(line.trim());
    }
  }
  flush();
  return blocks;
}

export function ReleaseNotes({ notes }: { notes: string }) {
  const blocks = parse(notes);
  return (
    <div className="space-y-2 text-xs leading-relaxed text-fg selectable">
      {blocks.map((b, i) => {
        if (b.kind === "h")
          return b.level <= 1 ? (
            <h4 key={i} className="text-[13px] font-semibold">{inline(b.text, `h${i}`)}</h4>
          ) : (
            <h5 key={i} className="pt-1 text-xs font-semibold text-fg">{inline(b.text, `h${i}`)}</h5>
          );
        if (b.kind === "ul")
          return (
            <ul key={i} className="list-disc space-y-1 pl-4 text-fg-muted">
              {b.items.map((it, j) => <li key={j}>{inline(it, `l${i}-${j}`)}</li>)}
            </ul>
          );
        return <p key={i} className="text-fg-muted">{inline(b.text, `p${i}`)}</p>;
      })}
    </div>
  );
}
