import { useEffect, useMemo, useState } from 'react';

import {
  classifyExponent, type ComplexityPoint, type ComplexityResult, type PayloadInfo,
} from '../shared/payload';
import { Button, Field, IconButton, NumberInput, TextInput } from './ui/controls';
import { Code, FolderOpen, Layers, Play, X } from './ui/icons';

// Payload Lab (M7): generate adversarial payloads from the catalog and run a
// complexity probe (`loadr sweep --complexity`) that fits the response-time
// growth exponent so a super-linear parser shows up as a red O(n^k) verdict.
// A modal over the workspace, mirroring PluginsPanel's structure and tokens.

const fmtBytes = (n: number): string =>
  n >= 1e6 ? `${(n / 1e6).toFixed(2)} MB` : n >= 1e3 ? `${(n / 1e3).toFixed(1)} KB` : `${n} B`;
const fmtSize = (n: number): string =>
  n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e3 ? `${(n / 1e3).toFixed(1)}k` : `${n}`;
const fmtMs = (n: number): string => `${n.toFixed(n < 10 ? 2 : 1)}ms`;

/** Colour band for a fitted exponent: green ≤1.2, amber ≤1.6, red above. */
function bandClass(k: number): string {
  if (k <= 1.2) return 'border-ok/40 bg-ok/10 text-ok';
  if (k <= 1.6) return 'border-warn/40 bg-warn/10 text-warn';
  return 'border-blood/40 bg-blood/15 text-flare';
}

export function PayloadLab({ onClose }: { onClose: () => void }) {
  const [catalog, setCatalog] = useState<PayloadInfo[]>([]);
  const [selected, setSelected] = useState<PayloadInfo | null>(null);
  const [magnitude, setMagnitude] = useState(0);
  const [gen, setGen] = useState<{ bytes: number; preview: string } | null>(null);
  const [genBusy, setGenBusy] = useState(false);
  const [genError, setGenError] = useState<string | null>(null);

  const [planPath, setPlanPath] = useState('');
  const [axis, setAxis] = useState('depth');
  const [sizesText, setSizesText] = useState('4000,8000,16000,32000,64000');
  const [maxExpText, setMaxExpText] = useState('');
  const [probe, setProbe] = useState<ComplexityResult | null>(null);
  const [probeBusy, setProbeBusy] = useState(false);
  const [probeError, setProbeError] = useState<string | null>(null);

  useEffect(() => {
    window.loadr.payloadCatalog().then((c) => {
      setCatalog(c);
      setSelected((s) => s ?? c[0] ?? null);
      setMagnitude((m) => (m === 0 && c[0] ? c[0].default : m));
    }).catch(() => {});
  }, []);

  // Category → kinds, preserving catalog order within each group.
  const groups = useMemo(() => {
    const by = new Map<string, PayloadInfo[]>();
    for (const p of catalog) {
      const g = by.get(p.category) ?? [];
      g.push(p);
      by.set(p.category, g);
    }
    return [...by.entries()];
  }, [catalog]);

  function pick(p: PayloadInfo) {
    setSelected(p);
    setMagnitude(p.default);
    setGen(null);
    setGenError(null);
  }

  async function generate() {
    if (!selected) return;
    setGenBusy(true);
    setGenError(null);
    try {
      setGen(await window.loadr.payloadGenerate(selected.name, magnitude));
    } catch (e) {
      setGen(null);
      setGenError((e as Error).message);
    } finally {
      setGenBusy(false);
    }
  }

  async function browsePlan() {
    const o = await window.loadr.openPlan();
    if (o) setPlanPath(o.path);
  }

  async function runProbe() {
    const values = sizesText.split(',').map((s) => Number(s.trim())).filter((n) => Number.isFinite(n) && n > 0);
    if (!planPath.trim() || values.length < 2) {
      setProbeError('Pick a plan file and at least two positive numeric sizes.');
      return;
    }
    const maxExp = maxExpText.trim() ? Number(maxExpText.trim()) : undefined;
    setProbeBusy(true);
    setProbeError(null);
    try {
      setProbe(await window.loadr.payloadComplexity(
        planPath.trim(),
        axis.trim() || 'depth',
        values,
        maxExp != null && Number.isFinite(maxExp) ? maxExp : undefined,
      ));
    } catch (e) {
      setProbe(null);
      setProbeError((e as Error).message);
    } finally {
      setProbeBusy(false);
    }
  }

  return (
    <div className="absolute inset-0 z-50 flex items-center justify-center bg-black/70 p-6 backdrop-blur-sm" role="dialog" aria-label="Payload Lab">
      <div className="flex max-h-[88vh] w-[68rem] flex-col rounded-2xl border border-edge bg-panel shadow-2xl shadow-black/60">
        <div className="flex items-center justify-between border-b border-edge px-4 py-3">
          <h2 className="flex items-center gap-2 font-bold text-white"><span className="text-flare"><Layers /></span>Payload Lab</h2>
          <IconButton icon={X} label="close payload lab" onClick={onClose} />
        </div>

        <div className="grid flex-1 grid-cols-2 gap-4 overflow-y-auto p-4">
          {/* ---- Generator ---------------------------------------------- */}
          <section className="flex min-w-0 flex-col gap-3">
            <h3 className="text-[11px] font-semibold uppercase tracking-wide text-smoke">Adversarial payloads</h3>
            <div className="space-y-3 rounded-xl border border-edge bg-coal p-3">
              {groups.map(([cat, kinds]) => (
                <div key={cat}>
                  <div className="mb-1 text-[10px] font-bold uppercase tracking-wide text-mist">{cat}</div>
                  <div className="flex flex-wrap gap-1.5">
                    {kinds.map((k) => {
                      const active = selected?.name === k.name;
                      return (
                        <button
                          key={k.name}
                          onClick={() => pick(k)}
                          className={`rounded-md border px-2 py-1 font-mono text-[11px] transition-colors ${
                            active ? 'border-ember bg-ink text-white' : 'border-edge bg-panel text-smoke hover:border-edge-bright hover:text-ash'
                          }`}
                        >
                          {k.name}
                        </button>
                      );
                    })}
                  </div>
                </div>
              ))}
            </div>

            {selected && (
              <div className="space-y-3 rounded-xl border border-edge bg-coal p-3">
                <div>
                  <div className="font-mono text-sm text-white">{selected.name}</div>
                  <p className="mt-1 text-xs leading-relaxed text-smoke">{selected.about}</p>
                  <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-[11px] text-mist">
                    <span>param <span className="text-ash">{selected.param}</span></span>
                    <span>default <span className="text-ash">{selected.default.toLocaleString()}</span></span>
                    <span>max <span className="text-ash">{selected.max.toLocaleString()}</span></span>
                    <span>content-type <span className="font-mono text-ash">{selected.contentType}</span></span>
                  </div>
                </div>

                <Field label={`magnitude (${selected.param}, max ${selected.max.toLocaleString()})`}>
                  <div className="flex items-center gap-2">
                    <input
                      type="range"
                      min={0}
                      max={selected.max}
                      step={Math.max(1, Math.floor(selected.max / 1000))}
                      value={Math.min(magnitude, selected.max)}
                      onChange={(e) => setMagnitude(Number(e.target.value))}
                      className="flex-1 accent-ember"
                      aria-label="magnitude slider"
                    />
                    <NumberInput
                      className="w-32"
                      min={0}
                      max={selected.max}
                      value={magnitude}
                      onChange={(e) => setMagnitude(Math.min(Number(e.target.value) || 0, selected.max))}
                      aria-label="magnitude"
                    />
                  </div>
                </Field>

                <Button variant="primary" icon={Code} onClick={generate} disabled={genBusy}>
                  {genBusy ? 'Generating…' : 'Generate'}
                </Button>

                {genError && (
                  <pre className="max-h-24 overflow-y-auto whitespace-pre-wrap rounded-lg border border-blood/40 bg-blood/10 p-2 text-xs text-flare">{genError}</pre>
                )}
                {gen && (
                  <div>
                    <div className="mb-1 flex items-center justify-between text-[11px] text-mist">
                      <span>preview (first {Math.min(gen.preview.length, 2048)} bytes)</span>
                      <span className="font-mono text-ash">{fmtBytes(gen.bytes)}</span>
                    </div>
                    <pre className="max-h-40 overflow-auto whitespace-pre-wrap break-all rounded-lg border border-edge bg-ink p-2 font-mono text-[11px] leading-relaxed text-smoke">
                      {gen.preview}
                      {gen.bytes > gen.preview.length ? '\n…' : ''}
                    </pre>
                  </div>
                )}
              </div>
            )}
          </section>

          {/* ---- Complexity probe --------------------------------------- */}
          <section className="flex min-w-0 flex-col gap-3">
            <h3 className="text-[11px] font-semibold uppercase tracking-wide text-smoke">Complexity probe</h3>
            <div className="space-y-3 rounded-xl border border-edge bg-coal p-3">
              <Field label="target plan">
                <div className="flex gap-2">
                  <TextInput value={planPath} placeholder="/path/to/plan.yaml" onChange={(e) => setPlanPath(e.target.value)} aria-label="plan path" />
                  <Button icon={FolderOpen} onClick={browsePlan}>Browse…</Button>
                </div>
              </Field>
              <div className="grid grid-cols-2 gap-2">
                <Field label="size axis"><TextInput value={axis} onChange={(e) => setAxis(e.target.value)} aria-label="axis" /></Field>
                <Field label="max exponent (optional)"><TextInput value={maxExpText} placeholder="e.g. 1.5" onChange={(e) => setMaxExpText(e.target.value)} aria-label="max exponent" /></Field>
              </div>
              <Field label="sizes (comma-separated)" hint="Exported to the plan as LOADR_SWEEP_<AXIS> — reference it from a ${payload:…} body.">
                <TextInput value={sizesText} onChange={(e) => setSizesText(e.target.value)} aria-label="sizes" />
              </Field>
              <Button variant="primary" icon={Play} onClick={runProbe} disabled={probeBusy}>
                {probeBusy ? 'Running probe…' : 'Run probe'}
              </Button>
              {probeError && (
                <pre className="max-h-24 overflow-y-auto whitespace-pre-wrap rounded-lg border border-blood/40 bg-blood/10 p-2 text-xs text-flare">{probeError}</pre>
              )}
            </div>

            {probe && <ProbeResult result={probe} axis={axis.trim() || 'depth'} maxExp={maxExpText.trim() ? Number(maxExpText.trim()) : null} />}
          </section>
        </div>
      </div>
    </div>
  );
}

function ProbeResult({ result, axis, maxExp }: { result: ComplexityResult; axis: string; maxExp: number | null }) {
  const { points, exponent, passed, exitCode } = result;
  return (
    <div className="space-y-3 rounded-xl border border-edge bg-coal p-3">
      <div className="flex items-center justify-between">
        {exponent != null ? (
          <span className={`inline-flex items-center gap-1 rounded-full border px-2.5 py-0.5 text-xs font-bold ${bandClass(exponent)}`}>
            O(n^{exponent.toFixed(2)})
          </span>
        ) : (
          <span className="text-xs text-mist">not enough distinct points to fit</span>
        )}
        {exponent != null && <span className="text-xs text-smoke">{classifyExponent(exponent)}</span>}
      </div>

      {maxExp != null && passed != null && (
        <div className={`text-xs font-semibold ${passed ? 'text-ok' : 'text-flare'}`}>
          {passed ? `✓ within the O(n^${maxExp.toFixed(2)}) bound` : `✗ exceeds the O(n^${maxExp.toFixed(2)}) bound`}
          {exitCode !== 0 && <span className="ml-2 font-normal text-mist">(sweep exit {exitCode})</span>}
        </div>
      )}

      <LogLogChart points={points} exponent={exponent} axis={axis} />

      {points.length > 0 && (
        <table className="w-full text-xs">
          <thead>
            <tr className="text-left text-mist"><th className="py-1 font-medium">{axis}</th><th className="py-1 text-right font-medium">p95 latency</th></tr>
          </thead>
          <tbody>
            {points.map((p) => (
              <tr key={p.size} className="border-t border-edge/50">
                <td className="py-1 font-mono text-ash">{fmtSize(p.size)}</td>
                <td className="py-1 text-right font-mono tabular-nums text-white">{fmtMs(p.latencyMs)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

// Self-contained log-log scatter + fitted line. No chart deps: the growth trend
// reads as a straight line whose slope is the fitted exponent.
function LogLogChart({ points, exponent, axis }: { points: ComplexityPoint[]; exponent: number | null; axis: string }) {
  const valid = points.filter((p) => p.size > 0 && p.latencyMs > 0);
  if (valid.length < 2) return null;

  const W = 460;
  const H = 220;
  const pad = 38;
  const xs = valid.map((p) => Math.log10(p.size));
  const ys = valid.map((p) => Math.log10(p.latencyMs));
  const xmin = Math.min(...xs);
  const xmax = Math.max(...xs);
  const ymin = Math.min(...ys);
  const ymax = Math.max(...ys);
  const sx = (x: number) => pad + ((x - xmin) / (xmax - xmin || 1)) * (W - 2 * pad);
  const sy = (y: number) => H - pad - ((y - ymin) / (ymax - ymin || 1)) * (H - 2 * pad);

  // Fitted line through the log10 centroid using the (scale-invariant) slope k.
  let fitLine: { x1: number; y1: number; x2: number; y2: number } | null = null;
  if (exponent != null) {
    const mx = xs.reduce((s, v) => s + v, 0) / xs.length;
    const my = ys.reduce((s, v) => s + v, 0) / ys.length;
    const b = my - exponent * mx;
    fitLine = { x1: sx(xmin), y1: sy(exponent * xmin + b), x2: sx(xmax), y2: sy(exponent * xmax + b) };
  }

  const poly = valid.map((p) => `${sx(Math.log10(p.size))},${sy(Math.log10(p.latencyMs))}`).join(' ');
  const lo = valid[0];
  const hi = valid[valid.length - 1];

  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="w-full" role="img" aria-label={`response time vs ${axis}, log-log`}>
      {/* frame */}
      <line x1={pad} y1={H - pad} x2={W - pad} y2={H - pad} stroke="currentColor" className="text-edge" strokeWidth={1} />
      <line x1={pad} y1={pad} x2={pad} y2={H - pad} stroke="currentColor" className="text-edge" strokeWidth={1} />
      {/* fitted trend */}
      {fitLine && (
        <line x1={fitLine.x1} y1={fitLine.y1} x2={fitLine.x2} y2={fitLine.y2} stroke="currentColor" className="text-ember" strokeWidth={1.5} strokeDasharray="4 3" />
      )}
      {/* measured points */}
      <polyline points={poly} fill="none" stroke="currentColor" className="text-smoke" strokeWidth={1.25} />
      {valid.map((p) => (
        <circle key={p.size} cx={sx(Math.log10(p.size))} cy={sy(Math.log10(p.latencyMs))} r={3} fill="currentColor" className="text-flare" />
      ))}
      {/* axis extents */}
      <text x={pad} y={H - pad + 14} className="fill-current text-mist" fontSize={9}>{fmtSize(lo.size)}</text>
      <text x={W - pad} y={H - pad + 14} textAnchor="end" className="fill-current text-mist" fontSize={9}>{fmtSize(hi.size)}</text>
      <text x={pad - 4} y={H - pad} textAnchor="end" className="fill-current text-mist" fontSize={9}>{fmtMs(Math.pow(10, ymin))}</text>
      <text x={pad - 4} y={pad + 6} textAnchor="end" className="fill-current text-mist" fontSize={9}>{fmtMs(Math.pow(10, ymax))}</text>
      <text x={W / 2} y={H - 4} textAnchor="middle" className="fill-current text-mist" fontSize={9}>{axis} (log)</text>
    </svg>
  );
}
