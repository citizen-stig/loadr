import { describe, expect, it } from 'vitest';

import { addScenario, addStep, deleteIn, getIn, moveStep, setExecutor, setIn } from './edit';
import { serializePlan } from './plan';
import { stepKind, type Plan } from './types';

describe('immutable path ops', () => {
  it('getIn reads nested values', () => {
    const o = { a: { b: [{ c: 1 }] } };
    expect(getIn(o, ['a', 'b', 0, 'c'])).toBe(1);
    expect(getIn(o, ['a', 'x'])).toBeUndefined();
  });

  it('setIn creates containers and does not mutate the input', () => {
    const before: Plan = { name: 'p' };
    const after = setIn(before, ['defaults', 'http', 'base_url'], 'https://x');
    expect(getIn(after, ['defaults', 'http', 'base_url'])).toBe('https://x');
    expect(before).toEqual({ name: 'p' }); // unchanged
  });

  it('setIn creates an array when the next segment is numeric', () => {
    const after = setIn({}, ['xs', 0], 'a');
    expect(Array.isArray((after as { xs: unknown }).xs)).toBe(true);
  });

  it('deleteIn removes object keys and array elements', () => {
    expect(deleteIn({ a: 1, b: 2 }, ['a'])).toEqual({ b: 2 });
    expect(deleteIn({ xs: [1, 2, 3] }, ['xs', 1])).toEqual({ xs: [1, 3] });
  });
});

describe('scenario + flow operations', () => {
  it('adds scenarios with unique names and a valid executor', () => {
    let plan: Plan = {};
    plan = addScenario(plan, 'load');
    plan = addScenario(plan, 'load');
    const names = Object.keys(plan.scenarios!);
    expect(names).toEqual(['load', 'load_2']);
    expect(plan.scenarios!.load.executor).toBe('constant-vus');
    expect(plan.scenarios!.load.vus).toBe(1);
  });

  it('setExecutor swaps params for the new kind', () => {
    let plan = addScenario({}, 's');
    plan = setExecutor(plan, 's', 'constant-arrival-rate');
    expect(plan.scenarios!.s.executor).toBe('constant-arrival-rate');
    expect(plan.scenarios!.s.rate).toBe(10);
    expect(plan.scenarios!.s.pre_allocated_vus).toBe(5);
  });

  it('addStep appends a typed step', () => {
    let plan = addScenario({}, 's');
    plan = addStep(plan, 's', 'request');
    plan = addStep(plan, 's', 'think_time');
    const flow = plan.scenarios!.s.flow!;
    expect(flow.map(stepKind)).toEqual(['request', 'think_time']);
  });

  it('moveStep reorders the flow', () => {
    let plan = addScenario({}, 's');
    plan = addStep(plan, 's', 'request');
    plan = addStep(plan, 's', 'think_time');
    plan = addStep(plan, 's', 'group');
    plan = moveStep(plan, 's', 2, 0); // group to front
    expect(plan.scenarios!.s.flow!.map(stepKind)).toEqual(['group', 'request', 'think_time']);
  });

  it('a composed plan serializes to YAML', () => {
    let plan: Plan = { name: 'composed' };
    plan = addScenario(plan, 'load');
    plan = addStep(plan, 'load', 'request');
    plan = setIn(plan, ['scenarios', 'load', 'flow', 0, 'request', 'url'], 'https://api.example.com');
    const yaml = serializePlan(plan);
    expect(yaml).toContain('constant-vus');
    expect(yaml).toContain('https://api.example.com');
  });
});
