// @vitest-environment jsdom
import { act, renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { parsePlan } from '../../shared/plan';
import { usePlanDoc } from './usePlanDoc';

describe('usePlanDoc two-way sync', () => {
  it('form edits flow into the YAML', () => {
    const { result } = renderHook(() => usePlanDoc('name: t\n'));
    act(() => result.current.update(['name'], 'edited'));
    expect(result.current.plan.name).toBe('edited');
    expect(parsePlan(result.current.yaml).name).toBe('edited');
    expect(result.current.dirty).toBe(true);
  });

  it('YAML edits flow into the model', () => {
    const { result } = renderHook(() => usePlanDoc(''));
    act(() => result.current.setYaml('name: from-yaml\nscenarios:\n  s:\n    executor: constant-vus\n'));
    expect(result.current.plan.name).toBe('from-yaml');
    expect(result.current.plan.scenarios?.s.executor).toBe('constant-vus');
    expect(result.current.parseError).toBeNull();
  });

  it('surfaces a parse error without losing the text', () => {
    const { result } = renderHook(() => usePlanDoc(''));
    act(() => result.current.setYaml('name: [unclosed'));
    expect(result.current.parseError).not.toBeNull();
    expect(result.current.yaml).toBe('name: [unclosed');
  });

  it('load resets dirty', () => {
    const { result } = renderHook(() => usePlanDoc('name: a\n'));
    act(() => result.current.update(['name'], 'b'));
    expect(result.current.dirty).toBe(true);
    act(() => result.current.load('name: c\n'));
    expect(result.current.dirty).toBe(false);
    expect(result.current.plan.name).toBe('c');
  });
});
