import { describe, expect, it } from 'vitest';

import { addRun, compareResults, runsForPlan, type RunRecord } from './history';
import type { Results } from './results';

function results(over: Partial<Results> = {}): Results {
  return {
    passed: true, aborted: null, durationSecs: 1, totalRequests: 100, iterations: 100,
    errorRate: 0, latency: { avg: 5, p95: 10, p99: 20 }, checks: { passed: 0, failed: 0 },
    thresholdsPassed: true, metrics: [], timeline: [], ...over,
  };
}

function rec(planName: string, at: number): RunRecord {
  return { id: `r${at}`, planName, at, passed: true, results: results() };
}

describe('run history', () => {
  it('prepends and caps per plan', () => {
    let h: RunRecord[] = [];
    for (let i = 0; i < 30; i++) h = addRun(h, rec('p', i), 25);
    expect(runsForPlan(h, 'p')).toHaveLength(25);
    expect(h[0].id).toBe('r29'); // newest first
  });

  it('caps each plan independently', () => {
    let h: RunRecord[] = [];
    for (let i = 0; i < 30; i++) h = addRun(h, rec('a', i), 5);
    h = addRun(h, rec('b', 100), 5);
    expect(runsForPlan(h, 'a')).toHaveLength(5);
    expect(runsForPlan(h, 'b')).toHaveLength(1);
  });
});

describe('compareResults', () => {
  it('computes signed deltas with lower-is-better flags', () => {
    const a = results({ latency: { avg: 5, p95: 10, p99: 20 }, errorRate: 0.01, totalRequests: 100 });
    const b = results({ latency: { avg: 6, p95: 15, p99: 25 }, errorRate: 0.02, totalRequests: 120 });
    const diff = compareResults(a, b);
    const p95 = diff.find((d) => d.label.startsWith('p95'))!;
    expect(p95.deltaPct).toBeCloseTo(50);
    expect(p95.lowerIsBetter).toBe(true);
    const reqs = diff.find((d) => d.label === 'requests')!;
    expect(reqs.deltaPct).toBeCloseTo(20);
    expect(reqs.lowerIsBetter).toBe(false);
  });

  it('returns null delta when a baseline is zero/absent', () => {
    const a = results({ latency: { avg: null, p95: null, p99: null } });
    const b = results({ latency: { avg: 5, p95: 10, p99: 20 } });
    expect(compareResults(a, b).find((d) => d.label.startsWith('p95'))!.deltaPct).toBeNull();
  });
});
