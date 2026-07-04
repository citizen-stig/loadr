// Parsing of loadr's machine-readable run output for the results view:
//   • the end-of-run summary JSON (`loadr run --summary-export`), and
//   • the live one-line progress stream printed during a run.
// Both are pure so the results/live logic is headless-testable; the rendering
// and the actual spawn are exercised by the M4 unit + M6 acceptance tests.

export interface AggValues {
  count: number;
  sum: number;
  avg: number | null;
  min: number | null;
  max: number | null;
  med: number | null;
  p90: number | null;
  p95: number | null;
  p99: number | null;
  p999: number | null;
  rate: number | null;
  last: number | null;
  per_second: number | null;
}

export interface MetricSummary {
  metric: string;
  kind: string;
  agg: AggValues;
}

export interface CheckSummary {
  name: string;
  passes: number;
  fails: number;
}

export interface ThresholdStatus {
  [k: string]: unknown;
  passed?: boolean;
}

export interface TimelinePoint {
  elapsed_secs: number;
  rps: number;
  iterations_ps: number;
  active_vus: number;
  error_rate: number;
  latency_avg?: number;
  latency_p50?: number;
  latency_p95?: number;
  latency_p99?: number;
}

export interface Summary {
  name?: string;
  run_id: string;
  started_ms: number;
  ended_ms: number;
  duration_secs: number;
  scenarios: string[];
  metrics: MetricSummary[];
  checks: CheckSummary[];
  thresholds: ThresholdStatus[];
  thresholds_passed: boolean;
  aborted: string | null;
  timeline: TimelinePoint[];
}

export class SummaryParseError extends Error {}

/** Parse + minimally validate a `--summary-export` JSON document. */
export function parseSummary(json: unknown): Summary {
  if (!json || typeof json !== 'object') throw new SummaryParseError('summary is not an object');
  const s = json as Record<string, unknown>;
  if (typeof s.run_id !== 'string' || !Array.isArray(s.metrics)) {
    throw new SummaryParseError('not a loadr run summary (missing run_id/metrics)');
  }
  return s as unknown as Summary;
}

/** Headline figures derived from a summary, protocol-agnostic. */
export interface Results {
  name?: string;
  passed: boolean;
  aborted: string | null;
  durationSecs: number;
  totalRequests: number;
  iterations: number;
  errorRate: number; // [0,1]
  latency: { avg: number | null; p95: number | null; p99: number | null };
  checks: { passed: number; failed: number };
  thresholdsPassed: boolean;
  metrics: MetricSummary[];
  timeline: TimelinePoint[];
}

function metric(s: Summary, name: string): MetricSummary | undefined {
  return s.metrics.find((m) => m.metric === name);
}

export function deriveResults(s: Summary): Results {
  // Total requests = every request-counter family (http_reqs, grpc_reqs, plugin
  // <name>_reqs). Latency = the busiest *_req_duration trend.
  const reqFamilies = s.metrics.filter((m) => m.metric.endsWith('_reqs'));
  const totalRequests = reqFamilies.reduce((n, m) => n + (m.agg.sum || m.agg.count || 0), 0);

  const durations = s.metrics
    .filter((m) => m.metric.endsWith('_req_duration'))
    .sort((a, b) => b.agg.count - a.agg.count);
  const lat = durations[0]?.agg;

  const failed = metric(s, 'http_req_failed')?.agg;
  const iters = metric(s, 'iterations')?.agg;

  return {
    name: s.name,
    passed: s.thresholds_passed && s.aborted == null,
    aborted: s.aborted,
    durationSecs: s.duration_secs,
    totalRequests,
    iterations: iters?.sum ?? iters?.count ?? 0,
    errorRate: failed?.rate ?? 0,
    latency: { avg: lat?.avg ?? null, p95: lat?.p95 ?? null, p99: lat?.p99 ?? null },
    checks: {
      passed: s.checks.reduce((n, c) => n + c.passes, 0),
      failed: s.checks.reduce((n, c) => n + c.fails, 0),
    },
    thresholdsPassed: s.thresholds_passed,
    metrics: s.metrics,
    timeline: s.timeline,
  };
}

// ---------------------------------------------------------------------------
// Run-to-run comparison (the desktop compare view). Pure: every figure is
// derived from two Results objects — the GUI never shells out to
// `loadr compare` — but the verdict rules mirror it: direction-aware
// (latency/error-rate up = worse, throughput/checks-rate down = worse) with
// the same 5% default tolerance.

export type CompareUnit = 'ms' | 'rps' | '%';

export interface CompareRow {
  label: string;
  unit: CompareUnit;
  baseline: number | null;
  current: number | null;
  /** current − baseline, in the row's unit (null when either side is missing). */
  delta: number | null;
  /** % change baseline→current (null when missing, or baseline is 0 and current isn't). */
  deltaPct: number | null;
  lowerIsBetter: boolean;
}

export type CompareVerdict = 'improved' | 'regressed' | 'flat';

/** Default regression tolerance (%), matching `loadr compare --max-regression`. */
export const REGRESSION_TOLERANCE_PCT = 5;

function changePct(baseline: number | null, current: number | null): number | null {
  if (baseline == null || current == null) return null;
  if (baseline === 0) return current === 0 ? 0 : null; // 0→x is an unbounded change
  return ((current - baseline) / baseline) * 100;
}

function row(label: string, unit: CompareUnit, baseline: number | null, current: number | null, lowerIsBetter: boolean): CompareRow {
  const delta = baseline == null || current == null ? null : current - baseline;
  return { label, unit, baseline, current, delta, deltaPct: changePct(baseline, current), lowerIsBetter };
}

/** p50 of the busiest *_req_duration trend (the same family deriveResults headlines). */
function latencyP50(r: Results): number | null {
  const durations = r.metrics
    .filter((m) => m.metric.endsWith('_req_duration'))
    .sort((a, b) => b.agg.count - a.agg.count);
  return durations[0]?.agg.med ?? null;
}

function meanRps(r: Results): number | null {
  return r.durationSecs > 0 ? r.totalRequests / r.durationSecs : null;
}

/** Merged checks pass percentage (null when the run had no checks). */
function checksRate(r: Results): number | null {
  const total = r.checks.passed + r.checks.failed;
  return total > 0 ? (r.checks.passed / total) * 100 : null;
}

/** Headline delta table between a baseline run and the current run. */
export function compareRuns(baseline: Results, current: Results): CompareRow[] {
  return [
    row('avg latency', 'ms', baseline.latency.avg, current.latency.avg, true),
    row('p50 latency', 'ms', latencyP50(baseline), latencyP50(current), true),
    row('p95 latency', 'ms', baseline.latency.p95, current.latency.p95, true),
    row('p99 latency', 'ms', baseline.latency.p99, current.latency.p99, true),
    row('requests / s', 'rps', meanRps(baseline), meanRps(current), false),
    row('error rate', '%', baseline.errorRate * 100, current.errorRate * 100, true),
    row('checks rate', '%', checksRate(baseline), checksRate(current), false),
  ];
}

/**
 * Direction-aware verdict: any improvement counts, a regression only beyond
 * the tolerance. A 0→non-zero move in the worse direction has no finite Δ%
 * and always counts as a regression.
 */
export function compareVerdict(r: CompareRow, tolerancePct = REGRESSION_TOLERANCE_PCT): CompareVerdict {
  if (r.delta == null || r.delta === 0) return 'flat';
  const worse = r.lowerIsBetter ? r.delta > 0 : r.delta < 0;
  if (!worse) return 'improved';
  return r.deltaPct == null || Math.abs(r.deltaPct) > tolerancePct ? 'regressed' : 'flat';
}

export interface LiveMetrics {
  elapsedSecs: number;
  vus: number;
  rps: number;
  p95Ms: number | null;
  failed: number;
}

const PROGRESS = /running\s+(\d{2}):(\d{2}):(\d{2})\s+vus\s+(\d+)\s+rps\s+([\d.]+)\s+p95\s+(-|[\d.]+ms)\s+failed\s+(\d+)/;

/** Parse one live progress line (`running HH:MM:SS vus N rps X p95 Yms failed Z`). */
export function parseProgressLine(line: string): LiveMetrics | null {
  const m = PROGRESS.exec(line);
  if (!m) return null;
  const [, hh, mm, ss, vus, rps, p95, failed] = m;
  return {
    elapsedSecs: Number(hh) * 3600 + Number(mm) * 60 + Number(ss),
    vus: Number(vus),
    rps: Number(rps),
    p95Ms: p95 === '-' ? null : Number(p95.replace('ms', '')),
    failed: Number(failed),
  };
}
