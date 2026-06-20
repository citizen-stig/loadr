// Proves the composer's data layer produces plans the CLI accepts: we build a
// plan with EVERY step kind through the same edit operations the forms call
// (addScenario / addStepAt / setIn over NEW_STEP skeletons), serialize it, and
// validate it with `loadr` — the same authority the round-trip test uses. If a
// step form can emit a shape the CLI rejects, this fails.

import { execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { describe, expect, it } from 'vitest';

import { addScenario, addStepAt, setIn } from './edit';
import { serializePlan } from './plan';
import { STEP_KINDS, type Plan } from './types';

function resolveLoadr(): string | null {
  if (process.env.LOADR_BIN && existsSync(process.env.LOADR_BIN)) return process.env.LOADR_BIN;
  for (const rel of ['../../../target/release/loadr', '../../../target/debug/loadr']) {
    const p = fileURLToPath(new URL(rel, import.meta.url));
    if (existsSync(p)) return p;
  }
  try {
    return execFileSync('bash', ['-lc', 'command -v loadr']).toString().trim() || null;
  } catch {
    return null;
  }
}
const loadr = resolveLoadr();

// Build a plan containing one of every step kind, filling the same required
// fields the forms collect. `flow.<i>` indices follow STEP_KINDS order.
function composeAllKinds(): Plan {
  let plan = addScenario({}, 's');
  const flow = ['scenarios', 's', 'flow'];
  for (const kind of STEP_KINDS) plan = addStepAt(plan, flow, kind);
  const at = (i: number, ...rest: (string | number)[]) => [...flow, i, ...rest];
  const idx = (k: string) => STEP_KINDS.indexOf(k as never);

  plan = setIn(plan, at(idx('request'), 'request', 'url'), '/home');
  plan = setIn(plan, at(idx('request'), 'request', 'assert'), [
    { type: 'status', equals: 200 },
    { type: 'jsonpath', expression: '$.id', exists: true },
  ]);
  plan = setIn(plan, at(idx('request'), 'request', 'checks'), [{ type: 'duration', max: '500ms' }]);
  plan = setIn(plan, at(idx('request'), 'request', 'extract'), [
    { type: 'jsonpath', name: 'id', expression: '$.id' },
    { type: 'regex', name: 'tok', expression: 't=(\\w+)', group: 1 },
  ]);
  plan = setIn(plan, at(idx('request'), 'request', 'timeout'), '5s');
  plan = setIn(plan, at(idx('js'), 'js'), 'session.vars.x = 1');
  plan = setIn(plan, at(idx('foreach'), 'foreach', 'items'), '${users}');
  plan = setIn(plan, at(idx('foreach'), 'foreach', 'steps'), [{ request: { url: '/u' } }]);
  plan = setIn(plan, at(idx('group'), 'group', 'steps'), [{ request: { url: '/g' } }]);
  plan = setIn(plan, at(idx('repeat'), 'repeat', 'steps'), [{ request: { url: '/r' } }]);
  plan = setIn(plan, at(idx('while'), 'while', 'steps'), [{ request: { url: '/w' } }]);
  plan = setIn(plan, at(idx('if'), 'if', 'then'), [{ request: { url: '/a' } }]);
  plan = setIn(plan, at(idx('during'), 'during', 'steps'), [{ request: { url: '/d' } }]);
  plan = setIn(plan, at(idx('retry'), 'retry', 'steps'), [{ request: { url: '/x' } }]);
  plan = setIn(plan, at(idx('switch'), 'switch', 'value'), '${tier}');
  plan = setIn(plan, at(idx('switch'), 'switch', 'cases'), { gold: [{ request: { url: '/gold' } }] });
  plan = setIn(plan, at(idx('parallel'), 'parallel', 'branches'), [[{ request: { url: '/p1' } }], [{ request: { url: '/p2' } }]]);
  plan = setIn(plan, at(idx('random'), 'random', 'choices'), [{ steps: [{ request: { url: '/c' } }] }]);
  return plan;
}

describe('composer produces CLI-valid plans', () => {
  it('builds one of every step kind', () => {
    const flow = composeAllKinds().scenarios!.s.flow!;
    expect(flow.length).toBe(STEP_KINDS.length);
  });

  it.runIf(loadr !== null)('the composed plan is accepted by `loadr validate`', () => {
    const yaml = serializePlan(composeAllKinds());
    const tmp = join(mkdtempSync(join(tmpdir(), 'loadr-compose-')), 'plan.yaml');
    writeFileSync(tmp, yaml);
    expect(() => execFileSync(loadr!, ['validate', '--no-check-files', tmp], { stdio: 'pipe' })).not.toThrow();
  });
});
