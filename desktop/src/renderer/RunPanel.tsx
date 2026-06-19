import { useEffect, useState } from 'react';

import { compareResults, runsForPlan, type RunRecord } from '../shared/history';
import { deriveResults, parseProgressLine, type LiveMetrics, type Results } from '../shared/results';

// M4: run the current plan via the CLI, show live metrics while it runs, then
// the results; persist to history and compare against a previous run.
export function RunPanel({ yaml, planName }: { yaml: string; planName: string }) {
  const [running, setRunning] = useState(false);
  const [live, setLive] = useState<LiveMetrics | null>(null);
  const [results, setResults] = useState<Results | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [history, setHistory] = useState<RunRecord[]>([]);
  const [compareId, setCompareId] = useState<string | null>(null);

  useEffect(() => {
    window.loadr?.historyList().then(setHistory).catch(() => {});
  }, []);

  async function run() {
    setRunning(true);
    setLive(null);
    setResults(null);
    setError(null);
    try {
      const summary = await window.loadr.run(yaml, (line) => {
        const m = parseProgressLine(line);
        if (m) setLive(m);
      });
      const r = deriveResults(summary);
      setResults(r);
      const rec: RunRecord = { id: String(Date.now()), planName, at: Date.now(), passed: r.passed, results: r };
      setHistory(await window.loadr.historyAppend(rec));
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setRunning(false);
    }
  }

  const planRuns = runsForPlan(history, planName);
  const baseline = planRuns.find((r) => r.id === compareId)?.results;

  return (
    <div className="flex max-h-[40vh] flex-col overflow-y-auto border-t border-[#232330] bg-[#0d0d12] p-3 text-sm">
      <div className="flex items-center gap-3">
        <button
          onClick={run}
          disabled={running}
          className="rounded bg-[#4ade80] px-3 py-1 font-semibold text-[#06240f] disabled:opacity-50"
        >
          {running ? 'Running…' : '▶ Run'}
        </button>
        {live && (
          <span className="font-mono text-xs text-[#9ca3af]">
            {live.elapsedSecs}s · vus {live.vus} · rps {live.rps.toFixed(0)} ·
            p95 {live.p95Ms == null ? '—' : `${live.p95Ms}ms`} · failed {live.failed}
          </span>
        )}
      </div>

      {error && <p className="mt-2 text-xs text-[#fca5a5]">✗ {error}</p>}

      {results && (
        <div className="mt-3">
          <div className="flex items-center gap-3">
            <span className={results.passed ? 'text-[#86efac]' : 'text-[#fca5a5]'}>
              {results.passed ? '✓ passed' : '✗ failed'}
            </span>
            <span className="text-xs text-[#6b7280]">{results.durationSecs.toFixed(1)}s</span>
          </div>
          <div className="mt-2 grid grid-cols-2 gap-x-6 gap-y-1 text-xs sm:grid-cols-4">
            <Stat label="requests" value={results.totalRequests.toLocaleString()} />
            <Stat label="error rate" value={`${(results.errorRate * 100).toFixed(2)}%`} />
            <Stat label="p95" value={results.latency.p95 == null ? '—' : `${results.latency.p95.toFixed(1)}ms`} />
            <Stat label="p99" value={results.latency.p99 == null ? '—' : `${results.latency.p99.toFixed(1)}ms`} />
            <Stat label="iterations" value={results.iterations.toLocaleString()} />
            <Stat label="checks" value={`${results.checks.passed}✓ ${results.checks.failed}✗`} />
            <Stat label="thresholds" value={results.thresholdsPassed ? 'pass' : 'fail'} />
          </div>
        </div>
      )}

      {planRuns.length > 0 && (
        <div className="mt-3">
          <p className="text-xs font-semibold uppercase tracking-wide text-[#6b7280]">History · compare</p>
          <ul className="mt-1 space-y-0.5">
            {planRuns.slice(0, 8).map((r) => (
              <li key={r.id} className="flex items-center gap-2 text-xs">
                <input type="radio" name="cmp" checked={compareId === r.id} onChange={() => setCompareId(r.id)} aria-label={`compare run ${r.id}`} />
                <span className={r.passed ? 'text-[#86efac]' : 'text-[#fca5a5]'}>{r.passed ? '✓' : '✗'}</span>
                <span className="text-[#9ca3af]">{new Date(r.at).toLocaleString()}</span>
                <span className="text-[#6b7280]">p95 {r.results.latency.p95?.toFixed(0) ?? '—'}ms · err {(r.results.errorRate * 100).toFixed(1)}%</span>
              </li>
            ))}
          </ul>
          {results && baseline && (
            <table className="mt-2 w-full text-xs">
              <thead>
                <tr className="text-left text-[#6b7280]"><th>metric</th><th>baseline</th><th>current</th><th>Δ</th></tr>
              </thead>
              <tbody>
                {compareResults(baseline, results).map((d) => (
                  <tr key={d.label}>
                    <td className="text-[#9ca3af]">{d.label}</td>
                    <td>{d.a ?? '—'}</td>
                    <td>{d.b ?? '—'}</td>
                    <td className={deltaClass(d.deltaPct, d.lowerIsBetter)}>
                      {d.deltaPct == null ? '—' : `${d.deltaPct > 0 ? '+' : ''}${d.deltaPct.toFixed(1)}%`}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      )}
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="text-[#6b7280]">{label}</div>
      <div className="font-mono text-[#e5e7eb]">{value}</div>
    </div>
  );
}

function deltaClass(deltaPct: number | null, lowerIsBetter: boolean): string {
  if (deltaPct == null || deltaPct === 0) return 'text-[#6b7280]';
  const better = lowerIsBetter ? deltaPct < 0 : deltaPct > 0;
  return better ? 'text-[#86efac]' : 'text-[#fca5a5]';
}
