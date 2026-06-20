// Pure helpers for the live monitoring dashboard: accumulate per-tick samples
// during a run and turn a series into normalised bar heights for the chart.
// Headless-testable; the rendering lives in RunMonitor.

import type { LiveMetrics } from './results';

const MAX_SAMPLES = 180;

/**
 * Append a live sample, replacing the last one if it lands on the same elapsed
 * second (loadr reprints the line each frame), and cap the retained history.
 */
export function pushSample(series: LiveMetrics[], s: LiveMetrics): LiveMetrics[] {
  const next = series.length && series[series.length - 1].elapsedSecs === s.elapsedSecs
    ? series.slice(0, -1)
    : series.slice();
  next.push(s);
  return next.length > MAX_SAMPLES ? next.slice(next.length - MAX_SAMPLES) : next;
}

/**
 * Normalise values to 0–100 bar heights against the series max (so the tallest
 * bar fills the chart). A flat/empty series yields a low baseline, not NaN.
 */
export function barHeights(values: number[]): number[] {
  const max = values.reduce((m, v) => (v > m ? v : m), 0);
  if (max <= 0) return values.map(() => 2);
  return values.map((v) => Math.max(2, Math.round((v / max) * 100)));
}
