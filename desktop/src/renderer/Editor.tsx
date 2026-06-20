import { useCallback, useEffect, useRef, useState } from 'react';

import { PlanMetaForm, ScenariosForm } from './forms/PlanForms';
import { Outline } from './Outline';
import { RunPanel } from './RunPanel';
import { usePlanDoc } from './state/usePlanDoc';
import { SelectionContext } from './state/selection';
import { YamlEditor } from './YamlEditor';
import { Button, IconButton, Segmented } from './ui/controls';
import { Alert, Check, Code, Columns, PanelLeft, Rows, Save } from './ui/icons';

export interface EditorState {
  path: string | null;
  dirty: boolean;
  yaml: string;
}

type View = 'form' | 'split' | 'yaml';

// One open plan. Three panes: an outline rail (navigate the plan), the
// forms-first composer, and an *optional* Monaco YAML view (Form / Split /
// YAML). You never have to touch YAML to build a plan. State is local to the
// tab (switching tabs preserves edits); the tab bar is told via onState.
export function Editor({
  seedContent,
  seedPath,
  onState,
}: {
  seedContent: string;
  seedPath: string | null;
  onState: (s: EditorState) => void;
}) {
  const doc = usePlanDoc(seedContent);
  const [path, setPath] = useState<string | null>(seedPath);
  const [view, setView] = useState<View>('form');
  const [showOutline, setShowOutline] = useState(true);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const formsRef = useRef<HTMLDivElement>(null);
  const reported = useRef('');

  useEffect(() => {
    const key = `${path} ${doc.dirty} ${doc.yaml}`;
    if (key === reported.current) return;
    reported.current = key;
    onState({ path, dirty: doc.dirty, yaml: doc.yaml });
  }, [path, doc.dirty, doc.yaml, onState]);

  // Outline → form: select an anchor, scroll its card into view (scoped to this
  // tab's container, since every tab's Editor stays mounted with the same ids).
  const select = useCallback((id: string) => {
    setSelectedId(id);
    if (view === 'yaml') setView('form');
    requestAnimationFrame(() => {
      formsRef.current?.querySelector(`[id="${id}"]`)?.scrollIntoView({ block: 'start', behavior: 'smooth' });
    });
  }, [view]);

  async function save(saveAs = false) {
    const saved = await window.loadr.savePlan(saveAs ? null : path, doc.yaml);
    if (saved) {
      setPath(saved);
      doc.markSaved();
    }
  }

  const status = doc.parseError
    ? { tone: 'text-flare', icon: <Alert />, text: doc.parseError }
    : doc.validation
      ? doc.validation.ok
        ? { tone: 'text-ok', icon: <Check />, text: 'valid' }
        : { tone: 'text-flare', icon: <Alert />, text: `${doc.validation.diagnostics.filter((d) => d.severity === 'error').length} error(s)` }
      : { tone: 'text-mist', icon: null, text: 'not yet validated' };

  const forms = (
    <div ref={formsRef} className="h-full overflow-y-auto px-6 py-5">
      {/* Fill the container — no centered max-width cap. In split view this is
          the left pane; in form view it's the whole editor width. */}
      <div className="w-full space-y-8">
        <PlanMetaForm doc={doc} />
        <ScenariosForm doc={doc} />
      </div>
    </div>
  );
  const yaml = <YamlEditor value={doc.yaml} onChange={doc.setYaml} />;

  return (
    <SelectionContext.Provider value={{ selectedId, select }}>
      <div className="flex h-full flex-col bg-ink">
        <div className="flex items-center justify-between gap-2 border-b border-edge bg-coal px-3 py-2">
          <div className="flex items-center gap-2">
            {view !== 'yaml' && (
              <IconButton
                icon={PanelLeft}
                label={showOutline ? 'hide outline' : 'show outline'}
                className={showOutline ? 'text-ash' : ''}
                onClick={() => setShowOutline((s) => !s)}
              />
            )}
            <Segmented
              ariaLabel="editor view"
              value={view}
              onChange={setView}
              options={[
                { value: 'form', label: 'Form', icon: Rows },
                { value: 'split', label: 'Split', icon: Columns },
                { value: 'yaml', label: 'YAML', icon: Code },
              ]}
            />
          </div>
          <div className="flex gap-2">
            <Button variant="primary" icon={Save} onClick={() => save(false)}>Save</Button>
            <Button onClick={() => save(true)}>Save as…</Button>
          </div>
        </div>

        <div className="flex min-h-0 flex-1">
          {showOutline && view !== 'yaml' && (
            <aside className="w-60 shrink-0 border-r border-edge">
              <Outline plan={doc.plan} />
            </aside>
          )}
          <div className="min-w-0 flex-1">
            {view === 'form' && forms}
            {view === 'yaml' && <div className="h-full">{yaml}</div>}
            {view === 'split' && (
              <div className="grid h-full grid-cols-2 overflow-hidden">
                {forms}
                <div className="min-h-0 border-l border-edge">{yaml}</div>
              </div>
            )}
          </div>
        </div>

        <RunPanel yaml={doc.yaml} planName={doc.plan.name ?? 'untitled'} />

        <div className="flex items-center justify-between border-t border-edge bg-coal px-4 py-1.5 text-xs">
          <span className={`inline-flex items-center gap-1.5 ${status.tone}`}>
            {status.icon}{status.text}
          </span>
          <span className="font-mono text-mist">{path ?? 'unsaved'}</span>
        </div>
      </div>
    </SelectionContext.Provider>
  );
}
