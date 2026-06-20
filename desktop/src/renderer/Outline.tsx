// The left-hand plan outline — a VS Code-explorer-style tree of the plan:
// Plan → scenarios → flow steps, recursing through every nested steps array.
// Clicking a node selects its anchor (the form scrolls + highlights the card).

import { useState } from 'react';

import { buildOutline, type OutlineNode } from '../shared/outline';
import type { Plan } from '../shared/types';
import { useSelection } from './state/selection';
import { ChevronDown, ChevronRight, Layers, Rows, STEP_ICON, type Icon } from './ui/icons';

export function Outline({ plan }: { plan: Plan }) {
  const nodes = buildOutline(plan);
  const stepCount = nodes.slice(1).reduce((n, s) => n + countSteps(s), 0);
  return (
    <nav aria-label="plan outline" className="flex h-full flex-col bg-coal">
      <div className="flex items-center justify-between px-3 py-2.5 text-[11px] font-semibold uppercase tracking-wider text-mist">
        <span>Outline</span>
        <span className="rounded bg-edge/60 px-1.5 py-0.5 text-[10px] text-smoke">{stepCount} steps</span>
      </div>
      <div className="flex-1 overflow-y-auto pb-3">
        {nodes.map((n) => <Row key={n.id} node={n} depth={0} />)}
      </div>
    </nav>
  );
}

function countSteps(node: OutlineNode): number {
  const self = node.kind !== 'sublist' && node.kind !== 'scenario' && node.kind !== 'plan' ? 1 : 0;
  return self + node.children.reduce((n, c) => n + countSteps(c), 0);
}

function iconFor(node: OutlineNode): Icon | null {
  if (node.kind === 'plan') return Rows;
  if (node.kind === 'scenario') return Layers;
  if (node.kind === 'sublist') return null;
  return STEP_ICON[node.kind] ?? null;
}

function Row({ node, depth }: { node: OutlineNode; depth: number }) {
  const { selectedId, select } = useSelection();
  const [open, setOpen] = useState(true);
  const hasKids = node.children.length > 0;
  const selected = selectedId === node.id;
  const IconC = iconFor(node);
  const sub = node.kind === 'sublist';

  return (
    <>
      <div
        className={`group flex cursor-pointer items-center gap-1.5 py-1 pr-2 text-sm transition-colors ${
          selected ? 'bg-ember/15 text-white shadow-[inset_2px_0_0_0_var(--color-ember)]' : 'text-ash hover:bg-panel/70'
        }`}
        style={{ paddingLeft: `${8 + depth * 14}px` }}
        onClick={() => select(node.id)}
        role="button"
        tabIndex={0}
        aria-current={selected || undefined}
        onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); select(node.id); } }}
      >
        <button
          className={`grid h-4 w-4 shrink-0 place-items-center text-mist ${hasKids ? 'hover:text-ash' : 'invisible'}`}
          aria-label={open ? 'collapse' : 'expand'}
          onClick={(e) => { e.stopPropagation(); setOpen((o) => !o); }}
        >
          {open ? <ChevronDown /> : <ChevronRight />}
        </button>
        {IconC ? (
          <span className={sub ? 'text-mist' : node.kind === 'scenario' || node.kind === 'plan' ? 'text-flare' : 'text-smoke'}>
            <IconC />
          </span>
        ) : (
          <span className="h-1 w-1 shrink-0 rounded-full bg-edge-bright" />
        )}
        <span className={`truncate ${sub ? 'text-xs uppercase tracking-wide text-mist' : 'font-medium'}`}>
          {node.label}
        </span>
        {node.summary && !sub && <span className="truncate text-xs text-mist">{node.summary}</span>}
      </div>
      {hasKids && open && node.children.map((c) => <Row key={c.id} node={c} depth={depth + 1} />)}
    </>
  );
}
