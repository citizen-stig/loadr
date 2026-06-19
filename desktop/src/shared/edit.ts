// Pure, immutable edit operations on a plan model. Every form mutation goes
// through here, which keeps editing logic headless-testable and independent of
// React. Paths are arrays of string keys / numeric indices.

import type { Json, Plan, Scenario, Step, StepKind } from './types';

export type Path = (string | number)[];

function clone<T>(v: T): T {
  return structuredClone(v);
}

/** Read the value at `path` (undefined if any segment is missing). */
export function getIn(obj: unknown, path: Path): unknown {
  let cur: unknown = obj;
  for (const seg of path) {
    if (cur == null || typeof cur !== 'object') return undefined;
    cur = (cur as Record<string | number, unknown>)[seg];
  }
  return cur;
}

/** Return a copy of `root` with `path` set to `value` (creating containers). */
export function setIn<T extends object>(root: T, path: Path, value: unknown): T {
  if (path.length === 0) return value as T;
  const next = clone(root);
  let cur = next as Record<string | number, unknown>;
  for (let i = 0; i < path.length - 1; i++) {
    const seg = path[i];
    const existing = cur[seg];
    if (existing == null || typeof existing !== 'object') {
      cur[seg] = typeof path[i + 1] === 'number' ? [] : {};
    }
    cur = cur[seg] as Record<string | number, unknown>;
  }
  cur[path[path.length - 1]] = value;
  return next;
}

/** Return a copy of `root` with `path` removed (array splice or key delete). */
export function deleteIn<T extends object>(root: T, path: Path): T {
  if (path.length === 0) return root;
  const next = clone(root);
  let cur = next as Record<string | number, unknown>;
  for (let i = 0; i < path.length - 1; i++) {
    const seg = path[i];
    if (cur[seg] == null || typeof cur[seg] !== 'object') return next;
    cur = cur[seg] as Record<string | number, unknown>;
  }
  const last = path[path.length - 1];
  if (Array.isArray(cur) && typeof last === 'number') cur.splice(last, 1);
  else delete cur[last];
  return next;
}

// ---- domain operations ----------------------------------------------------

const EXECUTOR_DEFAULTS: Record<string, Partial<Scenario>> = {
  'constant-vus': { vus: 1, duration: '30s' },
  'ramping-vus': { stages: [{ duration: '30s', target: 10 }] },
  'constant-arrival-rate': { rate: 10, duration: '30s', pre_allocated_vus: 5 },
  'per-vu-iterations': { vus: 1, iterations: 1 },
  'shared-iterations': { vus: 1, iterations: 1 },
};

/** Add a new, valid scenario with a unique name. */
export function addScenario(plan: Plan, name = 'scenario'): Plan {
  const scenarios = { ...(plan.scenarios ?? {}) };
  let key = name;
  let n = 2;
  while (key in scenarios) key = `${name}_${n++}`;
  scenarios[key] = { executor: 'constant-vus', ...EXECUTOR_DEFAULTS['constant-vus'], flow: [] };
  return { ...plan, scenarios };
}

/** Set a scenario's executor, seeding sensible defaults for the new kind. */
export function setExecutor(plan: Plan, scenario: string, executor: string): Plan {
  const sc = plan.scenarios?.[scenario];
  if (!sc) return plan;
  const updated: Scenario = {
    ...sc,
    executor: executor as Scenario['executor'],
    ...(EXECUTOR_DEFAULTS[executor] ?? {}),
  };
  return setIn(plan, ['scenarios', scenario], updated as unknown as Json);
}

const NEW_STEP: Record<StepKind, () => Step> = {
  request: () => ({ request: { method: 'GET', url: '' } }),
  think_time: () => ({ think_time: { type: 'constant', duration: '1s' } }),
  js: () => ({ js: '' }),
  group: () => ({ group: { name: 'group', steps: [] } }),
  repeat: () => ({ repeat: { times: 2, steps: [] } }),
  while: () => ({ while: { condition: 'true', steps: [] } }),
  if: () => ({ if: { condition: 'true', then: [] } }),
  random: () => ({ random: { choices: [] } }),
  foreach: () => ({ foreach: { items: [], steps: [] } }),
  switch: () => ({ switch: { value: '', cases: {} } }),
  during: () => ({ during: { duration: '10s', steps: [] } }),
  retry: () => ({ retry: { steps: [] } }),
  parallel: () => ({ parallel: { branches: [] } }),
  rendezvous: () => ({ rendezvous: { name: 'sync', users: 2 } }),
};

/** Append a new step of the given kind to a scenario's flow. */
export function addStep(plan: Plan, scenario: string, kind: StepKind, index?: number): Plan {
  const sc = plan.scenarios?.[scenario];
  if (!sc) return plan;
  const flow = [...(sc.flow ?? [])];
  const step = NEW_STEP[kind]();
  flow.splice(index ?? flow.length, 0, step);
  return setIn(plan, ['scenarios', scenario, 'flow'], flow as unknown as Json);
}

/** Move a step within a scenario's flow (drag-and-drop reorder). */
export function moveStep(plan: Plan, scenario: string, from: number, to: number): Plan {
  const sc = plan.scenarios?.[scenario];
  if (!sc?.flow) return plan;
  const flow = [...sc.flow];
  if (from < 0 || from >= flow.length || to < 0 || to >= flow.length) return plan;
  const [moved] = flow.splice(from, 1);
  flow.splice(to, 0, moved);
  return setIn(plan, ['scenarios', scenario, 'flow'], flow as unknown as Json);
}
