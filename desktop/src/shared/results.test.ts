import { describe, expect, it } from 'vitest';

import fixture from './summary.fixture.json';
import { deriveResults, parseProgressLine, parseSummary, SummaryParseError } from './results';

describe('summary parsing', () => {
  it('parses a real loadr --summary-export document', () => {
    const s = parseSummary(fixture);
    expect(s.run_id).toBeTruthy();
    expect(s.metrics.length).toBeGreaterThan(0);
    expect(Array.isArray(s.timeline)).toBe(true);
  });

  it('rejects non-summaries', () => {
    expect(() => parseSummary({})).toThrow(SummaryParseError);
    expect(() => parseSummary(null)).toThrow(SummaryParseError);
  });

  it('derives headline figures', () => {
    const r = deriveResults(parseSummary(fixture));
    expect(typeof r.passed).toBe('boolean');
    expect(r.durationSecs).toBeGreaterThan(0);
    expect(r.errorRate).toBeGreaterThanOrEqual(0);
    expect(r.errorRate).toBeLessThanOrEqual(1);
    expect(r.metrics).toBe(parseSummary(fixture).metrics ?? r.metrics); // metrics passed through
  });

  it('sums request families across protocols', () => {
    const s = parseSummary(fixture);
    // Inject a plugin family + http to prove the rollup is protocol-agnostic.
    s.metrics.push({ metric: 'mongo_reqs', kind: 'counter', agg: { ...zero(), sum: 7 } });
    s.metrics.push({ metric: 'http_reqs', kind: 'counter', agg: { ...zero(), sum: 3 } });
    expect(deriveResults(s).totalRequests).toBeGreaterThanOrEqual(10);
  });
});

describe('live progress line', () => {
  it('parses a running line', () => {
    const m = parseProgressLine('  running 00:01:05  vus    4  rps 78429.3  p95      2ms  failed 0   ');
    expect(m).toEqual({ elapsedSecs: 65, vus: 4, rps: 78429.3, p95Ms: 2, failed: 0 });
  });
  it('handles a "-" p95 (no latency yet / plugin)', () => {
    const m = parseProgressLine('  running 00:00:03  vus 4  rps 0.0  p95        -  failed 0');
    expect(m?.p95Ms).toBeNull();
    expect(m?.vus).toBe(4);
  });
  it('ignores non-progress lines', () => {
    expect(parseProgressLine('INFO scenario finished')).toBeNull();
  });
});

function zero() {
  return {
    count: 0, sum: 0, avg: null, min: null, max: null, med: null,
    p90: null, p95: null, p99: null, p999: null, rate: null, last: null, per_second: 0,
  };
}
