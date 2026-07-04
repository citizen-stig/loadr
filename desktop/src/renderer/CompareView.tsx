// Baseline-vs-current delta table for the run panel: one row per headline
// metric (latency percentiles, throughput, error rate, checks rate) with
// baseline / current / Δ / Δ% columns. Pure presentation over compareRuns()
// — the figures come from two saved Results objects, never the CLI.

import {
  compareRuns,
  compareVerdict,
  REGRESSION_TOLERANCE_PCT,
  type CompareUnit,
  type CompareVerdict,
  type Results,
} from '../shared/results';

const VERDICT_CLASS: Record<CompareVerdict, string> = {
  improved: 'text-ok',
  regressed: 'text-flare',
  flat: 'text-mist',
};

function fmt(v: number | null, unit: CompareUnit, signed = false): string {
  if (v == null) return '—';
  const digits = unit === '%' ? 2 : 1;
  const n = v.toLocaleString(undefined, { maximumFractionDigits: digits, minimumFractionDigits: digits });
  const s = signed && v > 0 ? `+${n}` : n;
  return unit === 'ms' ? `${s} ms` : unit === '%' ? `${s}%` : s;
}

function fmtPct(deltaPct: number | null, delta: number | null): string {
  if (deltaPct != null) return `${deltaPct > 0 ? '+' : ''}${deltaPct.toFixed(1)}%`;
  return delta != null && delta !== 0 ? '∞' : '—'; // baseline 0 → any move is unbounded
}

export function CompareView({ baseline, current }: { baseline: Results; current: Results }) {
  const rows = compareRuns(baseline, current);
  const regressions = rows.filter((r) => compareVerdict(r) === 'regressed').length;

  return (
    <div className="mt-2 overflow-hidden rounded-xl border border-edge bg-panel">
      <div className="flex items-center justify-between border-b border-edge px-3 py-2 text-xs">
        <span className="flex items-center gap-2 font-mono text-smoke">
          <span className="text-flare">▲</span> loadr · baseline vs current
        </span>
        <span className={regressions > 0 ? 'text-flare' : 'text-ok'}>
          {regressions > 0
            ? `✗ ${regressions} regression${regressions === 1 ? '' : 's'} beyond ${REGRESSION_TOLERANCE_PCT}%`
            : `✓ no regressions beyond ${REGRESSION_TOLERANCE_PCT}%`}
        </span>
      </div>
      <table className="w-full text-xs">
        <thead>
          <tr className="text-left text-mist">
            <th className="px-3 py-1.5 font-medium">metric</th>
            <th className="px-3 py-1.5 text-right font-medium">baseline</th>
            <th className="px-3 py-1.5 text-right font-medium">current</th>
            <th className="px-3 py-1.5 text-right font-medium">Δ</th>
            <th className="px-3 py-1.5 text-right font-medium">Δ%</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => {
            const tone = VERDICT_CLASS[compareVerdict(r)];
            return (
              <tr key={r.label} className="border-t border-edge/50">
                <td className="px-3 py-1 text-smoke">{r.label}</td>
                <td className="px-3 py-1 text-right font-mono tabular-nums text-mist">{fmt(r.baseline, r.unit)}</td>
                <td className="px-3 py-1 text-right font-mono tabular-nums text-white">{fmt(r.current, r.unit)}</td>
                <td className={`px-3 py-1 text-right font-mono tabular-nums ${tone}`}>{fmt(r.delta, r.unit, true)}</td>
                <td className={`px-3 py-1 text-right font-mono tabular-nums ${tone}`}>{fmtPct(r.deltaPct, r.delta)}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
