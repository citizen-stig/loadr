// Schema-shaped forms over a PlanDoc. Every edit goes through doc.update/apply,
// so the YAML pane stays in sync and the change is the same data the CLI sees.
// Drag-and-drop reordering is added in M3; M2 ships add/remove/up/down.

import { addScenario, addStep, deleteIn, moveStep, setExecutor } from '../../shared/edit';
import { STEP_KINDS, stepKind, type ExecutorKind, type Scenario, type Step, type StepKind } from '../../shared/types';
import type { PlanDoc } from '../state/usePlanDoc';

const EXECUTORS: ExecutorKind[] = [
  'constant-vus', 'ramping-vus', 'constant-arrival-rate', 'ramping-arrival-rate',
  'per-vu-iterations', 'shared-iterations', 'externally-controlled',
];

function Text({ label, value, onChange, placeholder }: {
  label: string; value: string; onChange: (v: string) => void; placeholder?: string;
}) {
  return (
    <label className="block">
      <span className="text-xs font-semibold text-[#9ca3af]">{label}</span>
      <input
        className="mt-1 w-full rounded border border-[#232330] bg-[#0d0d12] px-2 py-1 text-sm text-[#e5e7eb]"
        value={value}
        placeholder={placeholder}
        onChange={(e) => onChange(e.target.value)}
      />
    </label>
  );
}

function Num({ label, value, onChange }: { label: string; value: number | undefined; onChange: (v: number) => void }) {
  return (
    <label className="block">
      <span className="text-xs font-semibold text-[#9ca3af]">{label}</span>
      <input
        type="number"
        className="mt-1 w-full rounded border border-[#232330] bg-[#0d0d12] px-2 py-1 text-sm text-[#e5e7eb]"
        value={value ?? ''}
        onChange={(e) => onChange(Number(e.target.value))}
      />
    </label>
  );
}

export function PlanMetaForm({ doc }: { doc: PlanDoc }) {
  return (
    <Section title="Plan">
      <Text label="Name" value={doc.plan.name ?? ''} onChange={(v) => doc.update(['name'], v || undefined)} />
      <Text label="Description" value={doc.plan.description ?? ''} onChange={(v) => doc.update(['description'], v || undefined)} />
      <Text
        label="Base URL (defaults.http.base_url)"
        value={(doc.plan.defaults?.http?.base_url as string) ?? ''}
        placeholder="https://api.example.com"
        onChange={(v) => doc.update(['defaults', 'http', 'base_url'], v || undefined)}
      />
    </Section>
  );
}

export function ScenariosForm({ doc }: { doc: PlanDoc }) {
  const scenarios = Object.entries(doc.plan.scenarios ?? {});
  return (
    <Section
      title="Scenarios"
      action={
        <button className="rounded bg-[#dc2626] px-2 py-0.5 text-xs font-semibold text-white" onClick={() => doc.apply((p) => addScenario(p))}>
          + scenario
        </button>
      }
    >
      {scenarios.length === 0 && <p className="text-xs text-[#6b7280]">No scenarios yet.</p>}
      {scenarios.map(([name, sc]) => (
        <ScenarioForm key={name} doc={doc} name={name} sc={sc} />
      ))}
    </Section>
  );
}

function ScenarioForm({ doc, name, sc }: { doc: PlanDoc; name: string; sc: Scenario }) {
  const base = ['scenarios', name];
  return (
    <div className="rounded-lg border border-[#232330] bg-[#141419] p-3" data-testid={`scenario-${name}`}>
      <div className="flex items-center justify-between">
        <strong className="text-sm text-white">{name}</strong>
        <button className="text-xs text-[#fca5a5]" onClick={() => doc.apply((p) => deleteIn(p, ['scenarios', name]))}>
          remove
        </button>
      </div>
      <div className="mt-2 grid grid-cols-2 gap-2">
        <label className="block">
          <span className="text-xs font-semibold text-[#9ca3af]">Executor</span>
          <select
            className="mt-1 w-full rounded border border-[#232330] bg-[#0d0d12] px-2 py-1 text-sm text-[#e5e7eb]"
            value={sc.executor}
            onChange={(e) => doc.apply((p) => setExecutor(p, name, e.target.value))}
          >
            {EXECUTORS.map((ex) => <option key={ex} value={ex}>{ex}</option>)}
          </select>
        </label>
        {'vus' in sc && <Num label="VUs" value={sc.vus} onChange={(v) => doc.update([...base, 'vus'], v)} />}
        {'duration' in sc && <Text label="Duration" value={sc.duration ?? ''} onChange={(v) => doc.update([...base, 'duration'], v)} />}
        {'rate' in sc && <Num label="Rate" value={sc.rate} onChange={(v) => doc.update([...base, 'rate'], v)} />}
        {'iterations' in sc && <Num label="Iterations" value={sc.iterations} onChange={(v) => doc.update([...base, 'iterations'], v)} />}
      </div>
      <FlowForm doc={doc} scenario={name} flow={sc.flow ?? []} />
    </div>
  );
}

function FlowForm({ doc, scenario, flow }: { doc: PlanDoc; scenario: string; flow: Step[] }) {
  return (
    <div className="mt-3">
      <div className="flex items-center justify-between">
        <span className="text-xs font-semibold uppercase tracking-wide text-[#6b7280]">Flow</span>
        <select
          className="rounded border border-[#232330] bg-[#0d0d12] px-1 py-0.5 text-xs text-[#e5e7eb]"
          value=""
          onChange={(e) => {
            if (e.target.value) doc.apply((p) => addStep(p, scenario, e.target.value as StepKind));
          }}
          aria-label="add step"
        >
          <option value="">+ step…</option>
          {STEP_KINDS.map((k) => <option key={k} value={k}>{k}</option>)}
        </select>
      </div>
      <ol className="mt-2 space-y-2">
        {flow.map((step, i) => (
          <li key={i} className="rounded border border-[#232330] bg-[#0d0d12] p-2" data-testid={`step-${i}`}>
            <div className="flex items-center justify-between">
              <code className="text-xs text-[#fb923c]">{stepKind(step) ?? 'unknown'}</code>
              <div className="flex gap-2 text-xs text-[#6b7280]">
                <button disabled={i === 0} onClick={() => doc.apply((p) => moveStep(p, scenario, i, i - 1))}>↑</button>
                <button disabled={i === flow.length - 1} onClick={() => doc.apply((p) => moveStep(p, scenario, i, i + 1))}>↓</button>
                <button className="text-[#fca5a5]" onClick={() => doc.apply((p) => deleteIn(p, ['scenarios', scenario, 'flow', i]))}>✕</button>
              </div>
            </div>
            <StepFields doc={doc} scenario={scenario} index={i} step={step} />
          </li>
        ))}
      </ol>
    </div>
  );
}

function StepFields({ doc, scenario, index, step }: { doc: PlanDoc; scenario: string; index: number; step: Step }) {
  const kind = stepKind(step);
  const base = ['scenarios', scenario, 'flow', index, kind ?? ''];
  if (kind === 'request') {
    const req = (step.request ?? {}) as Record<string, unknown>;
    return (
      <div className="mt-2 grid grid-cols-[6rem_1fr] gap-2">
        <label className="block">
          <span className="text-xs font-semibold text-[#9ca3af]">Method</span>
          <select
            className="mt-1 w-full rounded border border-[#232330] bg-[#141419] px-2 py-1 text-sm text-[#e5e7eb]"
            value={(req.method as string) ?? 'GET'}
            onChange={(e) => doc.update([...base, 'method'], e.target.value)}
          >
            {['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS'].map((m) => <option key={m}>{m}</option>)}
          </select>
        </label>
        <Text label="URL" value={(req.url as string) ?? ''} onChange={(v) => doc.update([...base, 'url'], v)} placeholder="/path or https://…" />
      </div>
    );
  }
  if (kind === 'js' || typeof step[kind ?? ''] === 'string') {
    return (
      <Text label={kind ?? ''} value={String(step[kind ?? ''] ?? '')} onChange={(v) => doc.update([...base], v)} />
    );
  }
  return <p className="mt-1 text-xs text-[#6b7280]">Edit this {kind} step in the YAML pane (rich form in a later release).</p>;
}

function Section({ title, action, children }: { title: string; action?: React.ReactNode; children: React.ReactNode }) {
  return (
    <section className="space-y-2">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-bold uppercase tracking-wide text-[#9ca3af]">{title}</h2>
        {action}
      </div>
      {children}
    </section>
  );
}
