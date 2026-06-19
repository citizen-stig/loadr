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
