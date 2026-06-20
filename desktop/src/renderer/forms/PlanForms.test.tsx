// @vitest-environment jsdom
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { ScenariosForm } from './PlanForms';
import { parsePlan } from '../../shared/plan';
import { stepKind } from '../../shared/types';
import { usePlanDoc } from '../state/usePlanDoc';

// Harness: drive the real form through usePlanDoc and expose the live YAML so
// we can assert the full form → model → YAML path.
function Harness() {
  const doc = usePlanDoc('');
  return (
    <div>
      <ScenariosForm doc={doc} />
      <pre data-testid="yaml">{doc.yaml}</pre>
    </div>
  );
}

const yamlText = () => screen.getByTestId('yaml').textContent ?? '';

describe('ScenariosForm composes a plan', () => {
  it('adds a scenario, a request step, and edits its URL — all reflected in YAML', () => {
    render(<Harness />);

    fireEvent.click(screen.getByText('Scenario'));
    expect(parsePlan(yamlText()).scenarios?.scenario.executor).toBe('constant-vus');

    fireEvent.change(screen.getByLabelText('add step'), { target: { value: 'request' } });
    const flow = parsePlan(yamlText()).scenarios!.scenario.flow!;
    expect(flow.map(stepKind)).toEqual(['request']);

    fireEvent.change(screen.getByLabelText('URL'), {
      target: { value: 'https://api.example.com/v1' },
    });
    const req = parsePlan(yamlText()).scenarios!.scenario.flow![0].request as { url: string };
    expect(req.url).toBe('https://api.example.com/v1');
  });

  it('composes a foreach with a nested request — no YAML editing required', () => {
    render(<Harness />);
    fireEvent.click(screen.getByText('Scenario'));

    fireEvent.change(screen.getByLabelText('add step'), { target: { value: 'foreach' } });
    expect(parsePlan(yamlText()).scenarios!.scenario.flow!.map(stepKind)).toEqual(['foreach']);

    fireEvent.change(screen.getByLabelText('Items'), { target: { value: '${users}' } });

    // the nested per-item flow has its own "add step" control
    const adders = screen.getAllByLabelText('add step');
    fireEvent.change(adders[adders.length - 1], { target: { value: 'request' } });
    fireEvent.change(screen.getByLabelText('URL'), { target: { value: '/u/${item}' } });

    const fe = parsePlan(yamlText()).scenarios!.scenario.flow![0].foreach as {
      items: string; steps: { request: { url: string } }[];
    };
    expect(fe.items).toBe('${users}');
    expect(fe.steps[0].request.url).toBe('/u/${item}');
  });

  it('changing the executor reshapes the params', () => {
    render(<Harness />);
    fireEvent.click(screen.getByText('Scenario'));
    fireEvent.change(screen.getByLabelText('Executor'), {
      target: { value: 'constant-arrival-rate' },
    });
    const sc = parsePlan(yamlText()).scenarios!.scenario;
    expect(sc.executor).toBe('constant-arrival-rate');
    expect(sc.rate).toBe(10);
  });
});
