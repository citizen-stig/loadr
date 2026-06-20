// Live monitoring dashboard — mirrors the loadr.io web-UI overview: a live
// header, four headline tiles (Requests/s · Active VUs · p95 · Error rate), a
// throughput chart that streams while the run is live (and shows the run's
// timeline afterwards), and threshold pills. Driven by the live progress
// stream and, post-run, the summary timeline.

import { barHeights } from '../shared/monitor';
import type { LiveMetrics, Results } from '../shared/results';
import { Badge, Button } from './ui/controls';
import { Stop } from './ui/icons';

function fmt(n: number | null | undefined, digits = 0): string {
  return n == null ? '—' : n.toLocaleString(undefined, { maximumFractionDigits: digits, minimumFractionDigits: digits });
}

export function RunMonitor({
  running, live, series, results, onStop,
}: {
  running: boolean;
  live: LiveMetrics | null;
  series: LiveMetrics[];
  results: Results | null;
  onStop?: () => void;
}) {
  const timeline = results?.timeline ?? [];
  const lastTl = timeline.length ? timeline[timeline.length - 1] : null;

  const rps = running ? live?.rps ?? null : lastTl?.rps ?? (results ? results.totalRequests / Math.max(results.durationSecs, 1) : null);
  const vus = running ? live?.vus ?? null : timeline.reduce((m, p) => Math.max(m, p.active_vus), 0) || null;
  const p95 = running ? live?.p95Ms ?? null : results?.latency.p95 ?? null;
  const errPct = running ? null : results ? results.errorRate * 100 : null;

  const chartValues = running
    ? series.map((s) => s.rps)
    : timeline.length
      ? timeline.map((p) => p.rps)
      : series.map((s) => s.rps);
  const heights = barHeights(chartValues);

  return (
    <div className="overflow-hidden rounded-xl border border-edge bg-panel">
      <div className="flex items-center justify-between border-b border-edge px-4 py-2.5">
        <div className="flex items-center gap-2 font-mono text-xs text-smoke">
          <span className="text-flare">▲</span> loadr · overview
        </div>
        <div className="flex items-center gap-2 text-xs">
          {running ? (
            <span className="flex items-center gap-1.5 text-flare">
              <span className="livedot h-2 w-2 rounded-full bg-ember" /> live · {fmt(live?.elapsedSecs)}s
            </span>
          ) : results ? (
            <span className={results.passed ? 'text-ok' : 'text-flare'}>
              {results.passed ? '✓ passed' : '✗ failed'} · {results.durationSecs.toFixed(1)}s
            </span>
          ) : null}
          {running && onStop && (
            <Button size="sm" variant="danger" icon={Stop} onClick={onStop}>Stop</Button>
          )}
        </div>
      </div>

      <div className="grid gap-3 p-4 sm:grid-cols-2 lg:grid-cols-4">
        <Tile label="Requests / s" value={fmt(rps, 1)} />
        <Tile label="Active VUs" value={fmt(vus)} />
        <Tile label="p95 latency" value={p95 == null ? '—' : `${fmt(p95, p95 < 10 ? 1 : 0)} ms`} accent />
        {running
          ? <Tile label="Failed" value={fmt(live?.failed)} accent={!!live?.failed} />
          : <Tile label="Error rate" value={errPct == null ? '—' : `${errPct.toFixed(2)}%`} accent={!!errPct} />}
      </div>

      <div className="px-4 pb-2">
        <div className="flex h-28 items-end gap-px rounded-lg border border-edge bg-coal p-3">
          {heights.length === 0 ? (
            <span className="m-auto text-xs text-mist">waiting for throughput…</span>
          ) : (
            heights.map((h, i) => <div key={i} className="chartbar flex-1" style={{ height: `${h}%` }} />)
          )}
        </div>
        <p className="mt-1 text-right text-[10px] uppercase tracking-wider text-mist">requests / sec over time</p>
      </div>

      {results && (
        <div className="flex flex-wrap items-center gap-2 px-4 pb-4 text-xs">
          <Badge tone={results.thresholdsPassed ? 'ok' : 'ember'}>{results.thresholdsPassed ? '✓' : '✗'} thresholds</Badge>
          <Badge tone={results.errorRate < 0.01 ? 'ok' : 'ember'}>error rate {(results.errorRate * 100).toFixed(2)}%</Badge>
          {(results.checks.passed > 0 || results.checks.failed > 0) && (
            <Badge tone={results.checks.failed === 0 ? 'ok' : 'ember'}>
              checks {results.checks.passed}✓ {results.checks.failed}✗
            </Badge>
          )}
          <Badge>p99 {results.latency.p99 == null ? '—' : `${results.latency.p99.toFixed(1)}ms`}</Badge>
          <Badge>{fmt(results.totalRequests)} reqs</Badge>
        </div>
      )}
    </div>
  );
}

function Tile({ label, value, accent }: { label: string; value: string; accent?: boolean }) {
  return (
    <div className="rounded-lg border border-edge bg-coal p-3">
      <div className="text-[10px] font-bold uppercase tracking-wider text-smoke">{label}</div>
      <div className={`mt-1 text-2xl font-black tabular-nums ${accent ? 'text-flare' : 'text-white'}`}>{value}</div>
    </div>
  );
}
