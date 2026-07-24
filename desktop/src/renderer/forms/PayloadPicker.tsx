// Insert an adversarial payload straight into a request — no separate lab, no
// copy-paste. A small dropdown over the request Body: pick a kind and the body
// becomes the matching ${payload:…} template and the Content-Type is set. The
// catalog is fetched once and cached across every request editor.

import { useEffect, useMemo, useRef, useState } from 'react';

import type { PayloadInfo } from '../../shared/payload';

let catalogPromise: Promise<PayloadInfo[]> | null = null;
function loadCatalog(): Promise<PayloadInfo[]> {
  if (!catalogPromise) {
    catalogPromise = window.loadr?.payloadCatalog
      ? window.loadr.payloadCatalog().catch(() => [])
      : Promise.resolve([]);
  }
  return catalogPromise;
}

export function PayloadPicker({ onPick }: { onPick: (p: PayloadInfo) => void }) {
  const [catalog, setCatalog] = useState<PayloadInfo[]>([]);
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => { loadCatalog().then(setCatalog); }, []);
  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', onDoc);
    return () => document.removeEventListener('mousedown', onDoc);
  }, [open]);

  const groups = useMemo(() => {
    const by = new Map<string, PayloadInfo[]>();
    for (const p of catalog) {
      const g = by.get(p.category) ?? [];
      g.push(p);
      by.set(p.category, g);
    }
    return [...by.entries()];
  }, [catalog]);

  if (catalog.length === 0) return null;

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-haspopup="menu"
        aria-expanded={open}
        className="rounded-md border border-edge bg-panel px-2 py-0.5 text-[11px] text-smoke transition-colors hover:border-ember/60 hover:text-flare"
        title="Insert an adversarial payload as the request body"
      >
        + Payload
      </button>
      {open && (
        <div
          role="menu"
          className="absolute right-0 z-30 mt-1 max-h-72 w-64 overflow-y-auto rounded-lg border border-edge bg-coal p-2 shadow-xl shadow-black/50"
        >
          {groups.map(([cat, kinds]) => (
            <div key={cat}>
              <div className="px-1 pb-0.5 pt-1 text-[10px] font-bold uppercase tracking-wide text-mist">{cat}</div>
              {kinds.map((k) => (
                <button
                  key={k.name}
                  type="button"
                  role="menuitem"
                  onClick={() => { onPick(k); setOpen(false); }}
                  className="flex w-full items-baseline justify-between gap-2 rounded px-2 py-1 text-left font-mono text-[11px] text-smoke transition-colors hover:bg-panel hover:text-ash"
                >
                  <span className="truncate">{k.name}</span>
                  <span className="shrink-0 text-mist">{k.param}</span>
                </button>
              ))}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
