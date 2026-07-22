// The step palette — a grouped, draw.io-style list. Steps are organised into
// collapsible categories; each row is a full-width, draggable entry with an
// icon, name and a one-line hint. Drag a row onto a scenario or branch on the
// canvas to add that step there (native HTML drag-and-drop; the canvas reads
// the kind off the drop event).

import { useState } from 'react';

import type { StepKind } from '../../shared/types';
import { ChevronDown, ChevronRight, STEP_ICON } from '../ui/icons';

export const DND_MIME = 'application/loadr-step-kind';

interface Group {
  label: string;
  kinds: StepKind[];
}

// Categories, in the order they read on the canvas.
const GROUPS: Group[] = [
  { label: 'Request', kinds: ['request'] },
  { label: 'Timing', kinds: ['think_time', 'rendezvous'] },
  { label: 'Grouping', kinds: ['group'] },
  { label: 'Loops', kinds: ['repeat', 'while', 'during', 'foreach', 'retry'] },
  { label: 'Branching', kinds: ['if', 'switch', 'random'] },
  { label: 'Concurrency', kinds: ['parallel'] },
  { label: 'Scripting', kinds: ['js'] },
];

const HINT: Record<StepKind, string> = {
  request: 'HTTP request',
  think_time: 'pause between steps',
  rendezvous: 'sync VUs at a barrier',
  group: 'named block of steps',
  repeat: 'run N times',
  while: 'loop while a condition holds',
  during: 'loop for a duration',
  foreach: 'iterate over items',
  retry: 'retry with backoff',
  if: 'then / else branches',
  switch: 'branch by value',
  random: 'weighted random branch',
  parallel: 'concurrent branches',
  js: 'run JavaScript',
};

export function Palette() {
  return (
    <div className="flex h-full flex-col bg-coal">
      <div className="shrink-0 px-3 py-2.5 text-[11px] font-semibold uppercase tracking-wider text-mist">Steps</div>
      <div className="min-h-0 flex-1 overflow-y-auto pb-2">
        {GROUPS.map((g) => <PaletteGroup key={g.label} group={g} />)}
      </div>
      <p className="shrink-0 border-t border-edge px-3 py-2 text-[11px] leading-snug text-mist">
        Drag a step onto a scenario or branch to add it there.
      </p>
    </div>
  );
}

function PaletteGroup({ group }: { group: Group }) {
  const [open, setOpen] = useState(true);
  return (
    <div className="border-b border-edge/50">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        className="flex w-full items-center gap-1 px-2 py-1.5 text-left text-[11px] font-semibold uppercase tracking-wide text-smoke transition-colors hover:text-ash"
      >
        <span className="text-mist">{open ? <ChevronDown /> : <ChevronRight />}</span>
        {group.label}
      </button>
      {open && (
        <div className="pb-1">
          {group.kinds.map((kind) => <PaletteRow key={kind} kind={kind} />)}
        </div>
      )}
    </div>
  );
}

function PaletteRow({ kind }: { kind: StepKind }) {
  const IconC = STEP_ICON[kind];
  return (
    <div
      draggable
      onDragStart={(e) => {
        e.dataTransfer.setData(DND_MIME, kind);
        e.dataTransfer.effectAllowed = 'copy';
      }}
      className="group flex cursor-grab items-center gap-2 px-2.5 py-1.5 text-sm transition-colors hover:bg-panel active:cursor-grabbing"
      title={`Drag to add a ${kind} step`}
    >
      <span className="grid h-6 w-6 shrink-0 place-items-center rounded border border-edge bg-panel text-smoke group-hover:border-edge-bright">
        {IconC ? <IconC /> : <span className="h-1.5 w-1.5 rounded-full bg-edge-bright" />}
      </span>
      <span className="min-w-0">
        <span className="block truncate font-medium text-ash">{kind}</span>
        <span className="block truncate text-[11px] text-mist">{HINT[kind]}</span>
      </span>
    </div>
  );
}
