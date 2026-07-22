// Custom React Flow node components for the canvas editor. Each node carries
// the four handles the graph's edges use: left/right for nesting (a container
// → its contents) and top/bottom for sequence (step N → step N+1). Styling
// mirrors the loadr design tokens so the canvas reads as part of the app.

import { Handle, Position, type NodeProps, type NodeTypes } from '@xyflow/react';

import { Layers, Rows, STEP_ICON, type Icon } from '../ui/icons';
import type { PlanNode } from './graph';

// Per-step accent — loops/branches warm, requests neutral, waits cool.
const KIND_ACCENT: Record<string, string> = {
  request: 'border-l-ok',
  think_time: 'border-l-mist',
  js: 'border-l-flare',
  group: 'border-l-ember',
  repeat: 'border-l-ember',
  while: 'border-l-ember',
  during: 'border-l-ember',
  foreach: 'border-l-ember',
  retry: 'border-l-warn',
  if: 'border-l-warn',
  switch: 'border-l-warn',
  random: 'border-l-warn',
  parallel: 'border-l-flare',
  rendezvous: 'border-l-mist',
};

function handles() {
  // ids match the edges in graph.ts: nesting r→l, sequence b→t.
  return (
    <>
      <Handle id="t" type="target" position={Position.Top} className="!h-2 !w-2 !border-edge !bg-edge-bright" />
      <Handle id="l" type="target" position={Position.Left} className="!h-2 !w-2 !border-edge !bg-edge-bright" />
      <Handle id="r" type="source" position={Position.Right} className="!h-2 !w-2 !border-edge !bg-edge-bright" />
      <Handle id="b" type="source" position={Position.Bottom} className="!h-2 !w-2 !border-edge !bg-edge-bright" />
    </>
  );
}

function shell(selected: boolean, extra: string): string {
  return [
    'rounded-xl border bg-panel px-3 py-2 shadow-sm transition-colors',
    'w-[232px] cursor-pointer select-none',
    selected ? 'border-ember ring-2 ring-ember/40' : 'border-edge hover:border-edge-bright',
    extra,
  ].join(' ');
}

export function PlanNode({ data, selected }: NodeProps<PlanNode>) {
  return (
    <div className={shell(!!selected, 'bg-coal')}>
      {handles()}
      <div className="flex items-center gap-2">
        <span className="text-flare"><Rows /></span>
        <div className="min-w-0">
          <div className="text-[10px] font-semibold uppercase tracking-wider text-mist">Plan</div>
          <div className="truncate text-sm font-bold text-white">{data.label}</div>
        </div>
      </div>
    </div>
  );
}

export function ScenarioNode({ data, selected }: NodeProps<PlanNode>) {
  return (
    <div className={shell(!!selected, 'bg-coal')}>
      {handles()}
      <div className="flex items-center gap-2">
        <span className="text-flare"><Layers /></span>
        <div className="min-w-0 flex-1">
          <div className="text-[10px] font-semibold uppercase tracking-wider text-mist">Scenario</div>
          <div className="truncate text-sm font-bold text-white">{data.label}</div>
        </div>
      </div>
      {data.summary && (
        <div className="mt-1 inline-block rounded bg-edge/60 px-1.5 py-0.5 font-mono text-[10px] text-smoke">
          {data.summary}
        </div>
      )}
    </div>
  );
}

export function StepNode({ data, selected }: NodeProps<PlanNode>) {
  const IconC: Icon | undefined = STEP_ICON[data.kind];
  return (
    <div className={shell(!!selected, `border-l-[3px] ${KIND_ACCENT[data.kind] ?? 'border-l-edge-bright'}`)}>
      {handles()}
      <div className="flex items-center gap-2">
        {IconC ? <span className="text-smoke"><IconC /></span> : <span className="h-1.5 w-1.5 rounded-full bg-edge-bright" />}
        <span className="truncate text-sm font-medium text-ash">{data.label}</span>
      </div>
      {data.summary && <div className="mt-0.5 truncate pl-6 text-xs text-mist">{data.summary}</div>}
    </div>
  );
}

// A labelled branch container: then/else, a switch case, a parallel branch,
// a group's steps. It's a drop target for new/moved steps.
export function SublistNode({ data, selected }: NodeProps<PlanNode>) {
  return (
    <div
      className={[
        'rounded-lg border border-dashed px-3 py-1.5 text-center transition-colors',
        'w-[200px] cursor-pointer select-none',
        selected ? 'border-ember text-white' : 'border-edge-bright text-smoke hover:border-ember/60',
      ].join(' ')}
    >
      {handles()}
      <span className="font-mono text-[11px] uppercase tracking-wider">{data.label}</span>
    </div>
  );
}

export const NODE_TYPES: NodeTypes = {
  plan: PlanNode,
  scenario: ScenarioNode,
  step: StepNode,
  sublist: SublistNode,
};
