// Run history: a small, pure store of past run results per plan, plus a
// run-to-run comparison. Persisted to userData by the main process; the logic
// here is pure so it's headless-testable.

import type { Results } from './results';

export interface RunRecord {
  id: string;
  planName: string;
  at: number; // epoch ms
  passed: boolean;
  results: Results;
}

const DEFAULT_KEEP = 25;

/** Prepend a run; keep at most `keepPerPlan` records for that plan (newest first). */
export function addRun(history: RunRecord[], rec: RunRecord, keepPerPlan = DEFAULT_KEEP): RunRecord[] {
  const next = [rec, ...history];
  const counts: Record<string, number> = {};
  return next.filter((r) => {
    counts[r.planName] = (counts[r.planName] ?? 0) + 1;
    return counts[r.planName] <= keepPerPlan;
  });
}

/** Runs for a plan, newest first. */
export function runsForPlan(history: RunRecord[], planName: string): RunRecord[] {
  return history.filter((r) => r.planName === planName).sort((a, b) => b.at - a.at);
}

export interface MetricDelta {
  label: string;
  a: number | null;
  b: number | null;
  /** Percentage change from a→b (null if not computable). Lower-is-better noted per field. */
  deltaPct: number | null;
  lowerIsBetter: boolean;
}

function pct(a: number | null, b: number | null): number | null {
  if (a == null || b == null || a === 0) return null;
  return ((b - a) / a) * 100;
}

/** Compare two runs' headline figures (a = baseline, b = candidate). */
export function compareResults(a: Results, b: Results): MetricDelta[] {
  return [
    { label: 'p95 latency (ms)', a: a.latency.p95, b: b.latency.p95, deltaPct: pct(a.latency.p95, b.latency.p95), lowerIsBetter: true },
    { label: 'p99 latency (ms)', a: a.latency.p99, b: b.latency.p99, deltaPct: pct(a.latency.p99, b.latency.p99), lowerIsBetter: true },
    { label: 'error rate', a: a.errorRate, b: b.errorRate, deltaPct: pct(a.errorRate, b.errorRate), lowerIsBetter: true },
    { label: 'requests', a: a.totalRequests, b: b.totalRequests, deltaPct: pct(a.totalRequests, b.totalRequests), lowerIsBetter: false },
    { label: 'iterations', a: a.iterations, b: b.iterations, deltaPct: pct(a.iterations, b.iterations), lowerIsBetter: false },
  ];
}
