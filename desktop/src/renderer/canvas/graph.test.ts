import { describe, expect, it } from 'vitest';

import type { Plan } from '../../shared/types';
import { planToGraph } from './graph';

const plan: Plan = {
  name: 'demo',
  scenarios: {
    main: {
      executor: 'constant-vus',
      flow: [
        { request: { url: '/a' } },
        {
          group: {
            name: 'g',
            steps: [{ request: { url: '/b' } }, { think_time: { type: 'constant', duration: '1s' } }],
          },
        },
        {
          if: {
            condition: 'true',
            then: [{ request: { url: '/t' } }],
            else: [{ request: { url: '/e' } }],
          },
        },
      ],
    },
  },
};

describe('planToGraph', () => {
  const { nodes, edges } = planToGraph(plan);
  const byId = (id: string) => nodes.find((n) => n.id === id);

  it('renders a node per outline entry, including nested steps', () => {
    expect(byId('plan')).toBeTruthy();
    expect(byId('scenarios.main')).toBeTruthy();
    expect(byId('scenarios.main.flow.0')?.data.kind).toBe('request');
    expect(byId('scenarios.main.flow.1')?.data.kind).toBe('group');
    // group's nested steps
    expect(byId('scenarios.main.flow.1.group.steps.0')?.data.kind).toBe('request');
    // if/then and if/else sublists + their steps
    expect(byId('scenarios.main.flow.2.if.then.0')?.data.kind).toBe('request');
    expect(byId('scenarios.main.flow.2.if.else.0')?.data.kind).toBe('request');
  });

  it('nests scenarios under the plan and steps under scenarios', () => {
    expect(edges).toContainEqual(expect.objectContaining({ source: 'plan', target: 'scenarios.main', type: 'nest' }));
    expect(edges).toContainEqual(
      expect.objectContaining({ source: 'scenarios.main', target: 'scenarios.main.flow.0', type: 'nest' }),
    );
  });

  it('draws sequence arrows between consecutive steps of an ordered list', () => {
    expect(edges).toContainEqual(
      expect.objectContaining({
        source: 'scenarios.main.flow.0',
        target: 'scenarios.main.flow.1',
        type: 'seq',
      }),
    );
    // but NOT between the two `if` branches (then/else are alternatives)
    const branchSeq = edges.find(
      (e) => e.type === 'seq' && e.source.includes('if.then') && e.target.includes('if.else'),
    );
    expect(branchSeq).toBeUndefined();
  });

  it('gives ordered list elements their list coordinates for reparenting', () => {
    const step = byId('scenarios.main.flow.1.group.steps.1');
    expect(step?.data.listPath).toEqual(['scenarios', 'main', 'flow', 1, 'group', 'steps']);
    expect(step?.data.index).toBe(1);
    // the scenario itself is keyed by name, not a list index
    expect(byId('scenarios.main')?.data.listPath).toBeUndefined();
  });

  it('honours dragged position overrides', () => {
    const g = planToGraph(plan, { 'scenarios.main': { x: 999, y: 42 } });
    expect(g.nodes.find((n) => n.id === 'scenarios.main')?.position).toEqual({ x: 999, y: 42 });
  });

  it('handles an empty plan without throwing', () => {
    expect(planToGraph({}).nodes.length).toBeGreaterThanOrEqual(1); // just the plan root
    expect(planToGraph({} as Plan).edges).toEqual([]);
  });
});
