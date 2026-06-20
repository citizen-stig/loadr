// Build a navigable tree of a plan for the left-hand outline: Plan → each
// scenario → its flow, recursing through every nested steps array (group,
// loops, if/then-else, switch cases, parallel branches, random choices,
// foreach). Pure → headless-testable. Each node's `id` equals the DOM anchor
// the form renders (path.join('.')), so clicking a node scrolls to its card.

import { asArr, asObj, stepSummary } from './step';
import type { Path } from './edit';
import { stepKind, type Plan, type Step } from './types';

export interface OutlineNode {
  id: string;
  label: string;
  /** 'plan' | 'scenario' | a StepKind | 'group-steps' (a labelled sub-list) */
  kind: string;
  summary?: string;
  path: Path;
  children: OutlineNode[];
}

const anchor = (path: Path): string => path.join('.');

// Nested steps-arrays exposed by each step kind, as [label, subPath-from-body].
function childLists(kind: string, body: Record<string, unknown>): { label: string; sub: Path }[] {
  switch (kind) {
    case 'group': case 'repeat': case 'while': case 'during': case 'retry': case 'foreach':
      return [{ label: 'steps', sub: ['steps'] }];
    case 'if':
      return [{ label: 'then', sub: ['then'] }, { label: 'else', sub: ['else'] }];
    case 'switch': {
      const cases = asObj(body.cases);
      return [
        ...Object.keys(cases).map((name) => ({ label: name, sub: ['cases', name] as Path })),
        { label: 'default', sub: ['default'] as Path },
      ];
    }
    case 'parallel':
      return asArr(body.branches).map((_, i) => ({ label: `branch ${i + 1}`, sub: ['branches', i] }));
    case 'random':
      return asArr(body.choices).map((c, i) => ({
        label: (asObj(c).name as string) || `choice ${i + 1}`,
        sub: ['choices', i, 'steps'],
      }));
    default:
      return [];
  }
}

function stepNode(step: Step, stepsPath: Path, index: number): OutlineNode {
  const kind = stepKind(step);
  const path = [...stepsPath, index];
  const body = asObj(step[kind ?? '']);
  const children: OutlineNode[] = [];

  for (const { label, sub } of childLists(kind ?? '', body)) {
    const listPath = [...path, kind ?? '', ...sub];
    const kids = (asArr(getAt(body, sub)) as Step[]).map((s, i) => stepNode(s, listPath, i));
    // Only surface a sub-list grouping when it (could) hold steps.
    children.push({ id: anchor(listPath), label, kind: 'sublist', path: listPath, children: kids });
  }

  return {
    id: anchor(path),
    label: kind ?? 'unknown',
    kind: kind ?? 'unknown',
    summary: stepSummary(step, kind),
    path,
    children,
  };
}

function getAt(body: Record<string, unknown>, sub: Path): unknown {
  let cur: unknown = body;
  for (const seg of sub) {
    if (cur == null || typeof cur !== 'object') return undefined;
    cur = (cur as Record<string | number, unknown>)[seg];
  }
  return cur;
}

export function buildOutline(plan: Plan): OutlineNode[] {
  const nodes: OutlineNode[] = [
    { id: 'plan', label: plan.name || 'Plan', kind: 'plan', path: ['name'], children: [] },
  ];
  for (const [name, sc] of Object.entries(plan.scenarios ?? {})) {
    const flowPath = ['scenarios', name, 'flow'];
    nodes.push({
      id: anchor(['scenarios', name]),
      label: name,
      kind: 'scenario',
      summary: sc.executor,
      path: ['scenarios', name],
      children: (sc.flow ?? []).map((s, i) => stepNode(s, flowPath, i)),
    });
  }
  return nodes;
}
