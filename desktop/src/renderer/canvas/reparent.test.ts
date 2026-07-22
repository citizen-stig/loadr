import { describe, expect, it } from 'vitest';
import { getIn, moveStepBetween } from '../../shared/edit';
import type { Plan } from '../../shared/types';

const base = (): Plan => ({
  scenarios: {
    main: {
      executor: 'constant-vus',
      flow: [
        { request: { url: '/a' } },
        { group: { steps: [{ request: { url: '/b' } }] } },
      ],
    },
  },
});
const FLOW = ['scenarios', 'main', 'flow'];
const GROUP_STEPS = ['scenarios', 'main', 'flow', 1, 'group', 'steps'];
type Step = Record<string, { url?: string; steps?: unknown[] }>;

describe('moveStepBetween', () => {
  it('reparents a top-level step into a nested container (index-shift safe)', () => {
    const next = moveStepBetween(base(), FLOW, 0, GROUP_STEPS);
    const flow = getIn(next, FLOW) as Step[];
    expect(flow.length).toBe(1); // /a left the flow; only the group remains
    const group = flow.find((s) => 'group' in s)!;
    expect((group.group.steps as { request: { url: string } }[]).map((s) => s.request.url)).toEqual(['/b', '/a']);
  });

  it('reparents a nested step back out to the flow at an index', () => {
    const next = moveStepBetween(base(), GROUP_STEPS, 0, FLOW, 0);
    const flow = getIn(next, FLOW) as Step[];
    expect((flow[0] as Record<string, { url: string }>).request.url).toBe('/b'); // inserted first
    const group = flow.find((s) => 'group' in s)!;
    expect((group.group.steps as unknown[]).length).toBe(0);
  });

  it('same-array move is a reorder', () => {
    const next = moveStepBetween(base(), FLOW, 0, FLOW, 1);
    const flow = getIn(next, FLOW) as Record<string, unknown>[];
    expect(Object.keys(flow[0])[0]).toBe('group');
    expect(Object.keys(flow[1])[0]).toBe('request');
  });

  it('refuses to move a container into its own subtree', () => {
    const p = base();
    const next = moveStepBetween(p, FLOW, 1, GROUP_STEPS);
    expect(next).toEqual(p);
  });

  it('ignores out-of-range indices', () => {
    const p = base();
    expect(moveStepBetween(p, FLOW, 9, GROUP_STEPS)).toEqual(p);
  });
});
