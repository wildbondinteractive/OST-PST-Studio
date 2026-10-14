import { useEffect, useState } from "react";
import { FileDown } from "lucide-react";

/// A full-window hint while a file is being dragged over the application. The drop
/// itself is handled by the Tauri window event in App.tsx.
export function DropOverlay() {
  const [dragging, setDragging] = useState(false);
  useEffect(() => {
    const obs = new MutationObserver(() => setDragging(document.body.dataset.dragging === "1"));
    obs.observe(document.body, { attributes: true, attributeFilter: ["data-dragging"] });
    return () => obs.disconnect();
  }, []);
  if (!dragging) return null;
  return (
    <div className="pointer-events-none fixed inset-0 z-50 flex items-center justify-center bg-accent/10 backdrop-blur-[1px]">
      <div className="flex flex-col items-center gap-2 rounded-xl border-2 border-dashed border-accent bg-surface px-10 py-8 shadow-2xl">
        <FileDown size={28} className="text-accent" />
        <p className="text-sm font-semibold">Drop OST or PST files to open them</p>
        <p className="text-xs text-fg-muted">The original file is never modified.</p>
      </div>
    </div>
  );
}
