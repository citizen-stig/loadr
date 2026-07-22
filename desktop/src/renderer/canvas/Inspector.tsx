// The canvas detail pane. Selecting a node shows its full GUI form — the same
// per-kind editors the Form view uses (reused, not reimplemented) — plus a
// collapsible raw-YAML editor of the node's subtree as a universal escape
// hatch. Every change flows through the shared PlanDoc, so the YAML text,
// validation and the graph stay in lock-step.

import { useEffect, useMemo, useState } from 'react';
import yaml from 'js-yaml';

import { deleteIn, getIn, moveStepAt, setIn, type Path } from '../../shared/edit';
import type { Json, Scenario, Step, StepKind } from '../../shared/types';
import type { PlanDoc } from '../state/usePlanDoc';
import { PlanMetaForm, ScenarioParamFields, StepFields } from '../forms/PlanForms';
import { Button, Disclosure, IconButton } from '../ui/controls';
import { ArrowDown, ArrowUp, Copy, Trash, X } from '../ui/icons';
import type { NodeData } from './graph';

export function Inspector({
  node,
  doc,
  onClose,
  onSelect,
}: {
  node: NodeData | null;
  doc: PlanDoc;
  onClose: () => void;
  onSelect: (id: string) => void;
}) {
  if (!node) {
    return (
      <div className="grid h-full place-items-center p-6 text-center text-sm text-mist">
        Select a node on the canvas to edit it.
      </div>
    );
  }

  const value = getIn(doc.plan, node.path);
  const { listPath, index } = node;

  const remove = () => {
    doc.apply((p) => deleteIn(p, node.path));
    onClose();
  };
  const duplicate = () => {
    if (!listPath || index == null) return;
    doc.apply((p) => {
      const arr = [...((getIn(p, listPath) as Json[]) ?? [])];
      arr.splice(index + 1, 0, structuredClone(arr[index]));
      return setIn(p, listPath, arr as unknown as Json);
    });
  };
  const move = (delta: number) => {
    if (!listPath || index == null) return;
    doc.apply((p) => moveStepAt(p, listPath, index, index + delta));
    onSelect([...listPath, index + delta].join('.'));
  };

  return (
    <div className="flex h-full flex-col bg-coal" data-testid="inspector">
      <div className="flex items-center justify-between border-b border-edge px-3 py-2">
        <div className="min-w-0">
          <div className="text-[10px] font-semibold uppercase tracking-wider text-mist" data-testid="inspector-kind">
            {node.kind}
          </div>
          <div className="truncate text-sm font-semibold text-white">{node.label}</div>
        </div>
        <IconButton icon={X} label="close inspector" onClick={onClose} />
      </div>

      {listPath && index != null && (
        <div className="flex items-center gap-1 border-b border-edge px-3 py-1.5">
          <IconButton icon={ArrowUp} label="move up" onClick={() => move(-1)} />
          <IconButton icon={ArrowDown} label="move down" onClick={() => move(1)} />
          <IconButton icon={Copy} label="duplicate" onClick={duplicate} />
          <span className="flex-1" />
          <IconButton icon={Trash} tone="danger" label="delete" onClick={remove} />
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto p-3">
        <NodeForm node={node} doc={doc} />
        <div className="mt-4 border-t border-edge pt-3">
          <Disclosure label="Raw (YAML)">
            <RawEditor path={node.path} value={value} doc={doc} />
          </Disclosure>
        </div>
      </div>
    </div>
  );
}

// Dispatch to the real, tested per-kind forms.
function NodeForm({ node, doc }: { node: NodeData; doc: PlanDoc }) {
  if (node.kind === 'plan') return <PlanMetaForm doc={doc} />;

  if (node.kind === 'scenario') {
    const name = String(node.path[node.path.length - 1]);
    const sc = getIn(doc.plan, node.path) as Scenario | undefined;
    if (!sc) return null;
    return <ScenarioParamFields doc={doc} name={name} sc={sc} />;
  }

  if (node.kind === 'sublist') {
    return (
      <p className="text-xs leading-relaxed text-mist">
        A branch container. Add or arrange its steps on the canvas, or edit them
        as YAML below.
      </p>
    );
  }

  // Any step kind → its full form editor. StepFields writes to [...base, key]
  // and reads step[kind], so base must include the kind segment (same
  // convention StepCard uses in the Form view).
  const step = getIn(doc.plan, node.path) as Step | undefined;
  if (!step) return null;
  return <StepFields doc={doc} base={[...node.path, node.kind]} step={step} kind={node.kind as StepKind} />;
}

// Universal escape hatch: edit the node's subtree as YAML. Applies on a
// successful parse; a bad edit shows the error and leaves the model untouched.
function RawEditor({ path, value, doc }: { path: Path; value: unknown; doc: PlanDoc }) {
  const initial = useMemo(() => (value === undefined ? '' : yaml.dump(value).trimEnd()), [value]);
  const [text, setText] = useState(initial);
  const [error, setError] = useState<string | null>(null);
  const [dirty, setDirty] = useState(false);

  useEffect(() => {
    if (!dirty) setText(initial);
  }, [initial, dirty]);

  const commit = () => {
    setDirty(false);
    try {
      const parsed = (text.trim() === '' ? undefined : yaml.load(text)) as Json;
      setError(null);
      doc.apply((p) => setIn(p, path, parsed));
    } catch (e) {
      setError((e as Error).message);
    }
  };

  return (
    <div>
      {dirty && (
        <div className="mb-1.5 flex justify-end">
          <Button onClick={commit}>Apply</Button>
        </div>
      )}
      <textarea
        aria-label="raw yaml"
        value={text}
        onChange={(e) => { setText(e.target.value); setDirty(true); }}
        onBlur={commit}
        spellCheck={false}
        rows={10}
        className="w-full resize-y rounded-lg border border-edge bg-ink px-2.5 py-2 font-mono text-xs text-ash outline-none focus:border-ember/50"
      />
      {error && <p className="mt-1 text-xs text-flare">{error}</p>}
    </div>
  );
}
