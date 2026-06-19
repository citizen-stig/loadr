import { useEffect, useState } from 'react';

import { PlanMetaForm, ScenariosForm } from './forms/PlanForms';
import { usePlanDoc } from './state/usePlanDoc';
import { YamlEditor } from './YamlEditor';

const STARTER = `name: new plan
scenarios:
  default:
    executor: constant-vus
    vus: 1
    duration: 30s
    flow:
      - request:
          method: GET
          url: https://example.com
`;

// M2: a two-pane composer — schema-shaped forms on the left, the live YAML
// (Monaco) on the right, kept in sync both ways by usePlanDoc, with CLI-backed
// validation. Tabs + drag-and-drop land in M3.
export default function App() {
  const doc = usePlanDoc(STARTER);
  const [path, setPath] = useState<string | null>(null);
  const [version, setVersion] = useState('');

  useEffect(() => {
    window.loadr?.version().then(setVersion).catch(() => setVersion('loadr not found'));
  }, []);

  async function open() {
    const opened = await window.loadr.openPlan();
    if (!opened) return;
    setPath(opened.path);
    doc.load(opened.content);
  }

  async function save() {
    const saved = await window.loadr.savePlan(path, doc.yaml);
    if (saved) {
      setPath(saved);
      doc.markSaved();
    }
  }

  const status = doc.parseError
    ? { cls: 'text-[#fca5a5]', text: `✗ ${doc.parseError}` }
    : doc.validation
      ? doc.validation.ok
        ? { cls: 'text-[#86efac]', text: '✓ valid' }
        : { cls: 'text-[#fca5a5]', text: `✗ ${doc.validation.diagnostics.filter((d) => d.severity === 'error').length} error(s)` }
      : { cls: 'text-[#6b7280]', text: '—' };

  return (
    <div className="flex h-screen flex-col">
      <header className="flex items-center justify-between border-b border-[#232330] px-4 py-2">
        <div className="flex items-center gap-3">
          <span className="font-extrabold">loadr <span className="font-medium text-[#9ca3af]">Desktop</span></span>
          <span className="rounded-full border border-[#ef4444]/40 bg-[#ef4444]/10 px-2 py-0.5 text-[10px] font-bold uppercase text-[#f87171]">Beta</span>
          {doc.dirty && <span className="text-xs text-[#fbbf24]">● unsaved</span>}
        </div>
        <div className="flex gap-2">
          <button onClick={open} className="rounded border border-[#232330] px-3 py-1 text-sm text-[#e5e7eb]">Open…</button>
          <button onClick={save} className="rounded bg-[#dc2626] px-3 py-1 text-sm font-semibold text-white">Save</button>
        </div>
      </header>

      <div className="grid flex-1 grid-cols-2 overflow-hidden">
        <div className="space-y-5 overflow-y-auto border-r border-[#232330] p-4">
          <PlanMetaForm doc={doc} />
          <ScenariosForm doc={doc} />
        </div>
        <div className="min-h-0">
          <YamlEditor value={doc.yaml} onChange={doc.setYaml} />
        </div>
      </div>

      <footer className="flex items-center justify-between border-t border-[#232330] px-4 py-1.5 text-xs">
        <span className={status.cls}>{status.text}</span>
        <span className="text-[#6b7280]">{path ?? 'unsaved'} · {version}</span>
      </footer>
    </div>
  );
}
