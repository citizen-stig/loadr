// Small shared helpers for describing a flow step — used by both the form's
// step cards and the plan outline so their labels never drift apart.

import { stepKind, type Step, type StepKind } from './types';

export const asObj = (v: unknown): Record<string, unknown> =>
  v && typeof v === 'object' && !Array.isArray(v) ? (v as Record<string, unknown>) : {};
export const asArr = (v: unknown): unknown[] => (Array.isArray(v) ? v : []);

/** A short, human one-liner for a step (e.g. `GET /login`, `×3`, `2 branches`). */
export function stepSummary(step: Step, kind: StepKind | null = stepKind(step)): string {
  const b = asObj(step[kind ?? '']);
  switch (kind) {
    case 'request': return `${(b.method as string) ?? 'GET'} ${(b.url as string) ?? ''}`.trim();
    case 'think_time': return (b.type as string) ?? '';
    case 'js': return typeof step.js === 'string' ? step.js.slice(0, 48) : '';
    case 'group': return (b.name as string) ?? '';
    case 'if':
    case 'while': return (b.condition as string) ?? '';
    case 'repeat': return b.times != null ? `×${b.times}` : '';
    case 'foreach': return typeof b.items === 'string' ? b.items : '';
    case 'switch': return (b.value as string) ?? '';
    case 'during': return (b.duration as string) ?? '';
    case 'retry': return b.times != null ? `≤${b.times} attempts` : 'retry';
    case 'parallel': return `${asArr(b.branches).length} branches`;
    case 'random': return `${asArr(b.choices).length} choices`;
    case 'rendezvous': return `${(b.name as string) ?? ''} · ${b.users ?? '?'}`;
    default: return '';
  }
}
