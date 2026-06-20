import { describe, expect, it } from 'vitest';

import { buildOutline } from './outline';
import { parsePlan } from './plan';

const PLAN = `
name: demo
scenarios:
  default:
    executor: constant-vus
    flow:
      - request: { method: GET, url: /a }
      - foreach:
          items: \${users}
          steps:
            - request: { url: /u }
      - if:
          condition: "true"
          then:
            - request: { url: /t }
          else:
            - request: { url: /e }
`;

describe('buildOutline', () => {
  const nodes = buildOutline(parsePlan(PLAN));

  it('starts with a Plan node then one node per scenario', () => {
    expect(nodes[0]).toMatchObject({ id: 'plan', kind: 'plan', label: 'demo' });
    expect(nodes[1]).toMatchObject({ id: 'scenarios.default', kind: 'scenario', summary: 'constant-vus' });
  });

  it('lists flow steps as children with anchor ids matching their form path', () => {
    const flow = nodes[1].children;
    expect(flow.map((n) => n.kind)).toEqual(['request', 'foreach', 'if']);
    expect(flow[0].id).toBe('scenarios.default.flow.0');
    expect(flow[0].summary).toBe('GET /a');
  });

  it('recurses into a foreach steps sub-list', () => {
    const foreach = nodes[1].children[1];
    const steps = foreach.children.find((c) => c.label === 'steps')!;
    expect(steps.id).toBe('scenarios.default.flow.1.foreach.steps');
    expect(steps.children[0]).toMatchObject({ kind: 'request', id: 'scenarios.default.flow.1.foreach.steps.0' });
  });

  it('exposes if then/else branches', () => {
    const ifNode = nodes[1].children[2];
    expect(ifNode.children.map((c) => c.label)).toEqual(['then', 'else']);
    expect(ifNode.children[0].children[0].summary).toBe('GET /t');
  });
});
