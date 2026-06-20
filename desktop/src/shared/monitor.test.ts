import { describe, expect, it } from 'vitest';

import { barHeights, pushSample } from './monitor';
import type { LiveMetrics } from './results';

const s = (elapsedSecs: number, rps: number): LiveMetrics => ({ elapsedSecs, vus: 1, rps, p95Ms: 10, failed: 0 });

describe('pushSample', () => {
  it('appends new ticks', () => {
    let series: LiveMetrics[] = [];
    series = pushSample(series, s(1, 100));
    series = pushSample(series, s(2, 200));
    expect(series.map((x) => x.elapsedSecs)).toEqual([1, 2]);
  });

  it('replaces the last sample when the second repeats (live reprint)', () => {
    let series: LiveMetrics[] = [];
    series = pushSample(series, s(1, 100));
    series = pushSample(series, s(1, 150));
    expect(series).toHaveLength(1);
    expect(series[0].rps).toBe(150);
  });
});

describe('barHeights', () => {
  it('normalises to the max (tallest fills the chart)', () => {
    expect(barHeights([50, 100, 0])).toEqual([50, 100, 2]);
  });
  it('returns a baseline for an all-zero/empty series', () => {
    expect(barHeights([0, 0])).toEqual([2, 2]);
    expect(barHeights([])).toEqual([]);
  });
});
