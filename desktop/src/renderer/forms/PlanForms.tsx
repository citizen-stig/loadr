// Schema-shaped forms over a PlanDoc. Every edit flows through doc.update/apply,
// so the model, the (optional) YAML pane and what the CLI sees never disagree.
//
// The flow editor is RECURSIVE: a single <FlowEditor> drives a steps array at
// any depth — a scenario's `flow`, an `if`'s `then`/`else`, a `parallel`
// branch, a `switch` case — addressed by a path. Every loadr step kind has a
// real form here, so composing a plan never requires dropping to YAML.

import { useRef, useState } from 'react';
import {
  DndContext, KeyboardSensor, PointerSensor, closestCenter, useSensor, useSensors, type DragEndEvent,
} from '@dnd-kit/core';
import {
  SortableContext, sortableKeyboardCoordinates, useSortable, verticalListSortingStrategy,
} from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';

import {
  addScenario, addStepAt, appendTo, deleteIn, moveStepAt, renameKey, setExecutor, type Path,
} from '../../shared/edit';
import {
  STEP_KINDS, stepKind, type ExecutorKind, type Json, type Scenario, type Step, type StepKind,
} from '../../shared/types';
import { asArr as arr, asObj as obj, stepSummary } from '../../shared/step';
import type { PlanDoc } from '../state/usePlanDoc';
import { useSelection } from '../state/selection';
import { dragEndIndices } from './dnd';
import {
  Badge, Button, Checkbox, Disclosure, Field, IconButton, NumberInput, Select, Textarea, TextInput,
} from '../ui/controls';
import {
  ArrowDown, ArrowUp, ChevronDown, ChevronRight, Grip, Plus, STEP_ICON, Trash, type Icon,
} from '../ui/icons';

const EXECUTORS: ExecutorKind[] = [
  'constant-vus', 'ramping-vus', 'constant-arrival-rate', 'ramping-arrival-rate',
  'per-vu-iterations', 'shared-iterations', 'externally-controlled',
];

// ---- top-level sections ---------------------------------------------------
export function PlanMetaForm({ doc }: { doc: PlanDoc }) {
  return (
    <Section id="plan" title="Plan" subtitle="Identity & shared HTTP defaults">
      <div className="grid gap-3">
        <Field label="Name">
          <TextInput value={doc.plan.name ?? ''} placeholder="my load test" onChange={(e) => doc.update(['name'], e.target.value || undefined)} />
        </Field>
        <Field label="Description">
          <TextInput value={doc.plan.description ?? ''} placeholder="what this plan exercises" onChange={(e) => doc.update(['description'], e.target.value || undefined)} />
        </Field>
        <Field label="Base URL" hint="defaults.http.base_url — prepended to relative request URLs">
          <TextInput
            value={(doc.plan.defaults?.http?.base_url as string) ?? ''}
            placeholder="https://api.example.com"
            onChange={(e) => doc.update(['defaults', 'http', 'base_url'], e.target.value || undefined)}
          />
        </Field>
      </div>
    </Section>
  );
}

export function ScenariosForm({ doc }: { doc: PlanDoc }) {
  const scenarios = Object.entries(doc.plan.scenarios ?? {});
  return (
    <Section
      title="Scenarios"
      subtitle="Workload shapes and their step flows"
      action={
        <Button size="sm" variant="primary" icon={Plus} onClick={() => doc.apply((p) => addScenario(p))}>
          Scenario
        </Button>
      }
    >
      {scenarios.length === 0 && (
        <p className="rounded-lg border border-dashed border-edge px-3 py-6 text-center text-sm text-mist">
          No scenarios yet. Add one to start composing a flow.
        </p>
      )}
      <div className="space-y-3">
        {scenarios.map(([name, sc]) => (
          <ScenarioForm key={name} doc={doc} name={name} sc={sc} />
        ))}
      </div>
    </Section>
  );
}

function ScenarioForm({ doc, name, sc }: { doc: PlanDoc; name: string; sc: Scenario }) {
  const base = ['scenarios', name];
  const { selectedId } = useSelection();
  const anchor = `scenarios.${name}`;
  return (
    <div
      id={anchor}
      className={`scroll-mt-4 rounded-xl border bg-panel transition-colors ${selectedId === anchor ? 'border-ember' : 'border-edge'}`}
      data-testid={`scenario-${name}`}
    >
      <div className="flex items-center justify-between gap-2 border-b border-edge px-3 py-2">
        <div className="flex items-center gap-2">
          <span className="h-2 w-2 rounded-full bg-ember" />
          <strong className="text-sm text-white">{name}</strong>
          <Badge>{sc.executor}</Badge>
        </div>
        <IconButton icon={Trash} tone="danger" label={`remove scenario ${name}`} onClick={() => doc.apply((p) => deleteIn(p, ['scenarios', name]))} />
      </div>
      <div className="space-y-4 p-3">
        <div className="grid grid-cols-2 gap-3">
          <Field label="Executor">
            <Select aria-label="Executor" value={sc.executor} onChange={(e) => doc.apply((p) => setExecutor(p, name, e.target.value))}>
              {EXECUTORS.map((ex) => <option key={ex} value={ex}>{ex}</option>)}
            </Select>
          </Field>
          {'vus' in sc && <Field label="VUs"><NumField value={sc.vus} onChange={(v) => doc.update([...base, 'vus'], v)} /></Field>}
          {'duration' in sc && <Field label="Duration"><TextInput value={sc.duration ?? ''} placeholder="30s" onChange={(e) => doc.update([...base, 'duration'], e.target.value)} /></Field>}
          {'rate' in sc && <Field label="Rate"><NumField value={sc.rate} onChange={(v) => doc.update([...base, 'rate'], v)} /></Field>}
          {'iterations' in sc && <Field label="Iterations"><NumField value={sc.iterations} onChange={(v) => doc.update([...base, 'iterations'], v)} /></Field>}
          {'pre_allocated_vus' in sc && <Field label="Pre-allocated VUs"><NumField value={sc.pre_allocated_vus} onChange={(v) => doc.update([...base, 'pre_allocated_vus'], v)} /></Field>}
        </div>
        <FlowEditor doc={doc} path={[...base, 'flow']} steps={sc.flow ?? []} title="Flow" />
      </div>
    </div>
  );
}

// ---- the recursive flow editor -------------------------------------------
function FlowEditor({
  doc, path, steps, title = 'Steps', dense = false,
}: { doc: PlanDoc; path: Path; steps: Step[]; title?: string; dense?: boolean }) {
  const scope = path.join('.');
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );
  const ids = steps.map((_, i) => `${scope}::${i}`);

  function onDragEnd(e: DragEndEvent) {
    const move = dragEndIndices(e);
    if (move) doc.apply((p) => moveStepAt(p, path, move.from, move.to));
  }

  return (
    <div id={scope} className={`scroll-mt-4 ${dense ? '' : 'mt-1'}`}>
      <div className="mb-2 flex items-center justify-between">
        <span className="text-[11px] font-semibold uppercase tracking-wider text-mist">
          {title} <span className="text-edge-bright">·</span> {steps.length}
        </span>
        <AddStepMenu onAdd={(k) => doc.apply((p) => addStepAt(p, path, k))} />
      </div>
      {steps.length === 0 ? (
        <p className="rounded-lg border border-dashed border-edge px-3 py-3 text-center text-xs text-mist">
          Empty — add a step.
        </p>
      ) : (
        <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={onDragEnd}>
          <SortableContext items={ids} strategy={verticalListSortingStrategy}>
            <ol className="space-y-2">
              {steps.map((step, i) => (
                <StepCard key={i} id={`${scope}::${i}`} doc={doc} path={path} index={i} step={step} count={steps.length} />
              ))}
            </ol>
          </SortableContext>
        </DndContext>
      )}
    </div>
  );
}

function AddStepMenu({ onAdd }: { onAdd: (k: StepKind) => void }) {
  return (
    <div className="relative">
      <Select
        aria-label="add step"
        value=""
        className="w-auto py-1 pl-2 pr-7 text-xs"
        onChange={(e) => {
          if (e.target.value) onAdd(e.target.value as StepKind);
        }}
      >
        <option value="">+ step…</option>
        {STEP_KINDS.map((k) => <option key={k} value={k}>{k}</option>)}
      </Select>
    </div>
  );
}

function StepCard({
  id, doc, path, index, step, count,
}: { id: string; doc: PlanDoc; path: Path; index: number; step: Step; count: number }) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id });
  const [open, setOpen] = useState(true);
  const { selectedId } = useSelection();
  const kind = stepKind(step);
  const anchor = [...path, index].join('.');
  const selected = selectedId === anchor;
  const IconC: Icon = (kind && STEP_ICON[kind]) || STEP_ICON.request;
  const style = { transform: CSS.Transform.toString(transform), transition, zIndex: isDragging ? 30 : undefined };

  return (
    <li
      id={anchor}
      ref={setNodeRef}
      style={style}
      className={`scroll-mt-4 overflow-hidden rounded-lg border bg-coal transition-colors ${
        isDragging ? 'border-ember/60 shadow-lg shadow-black/40' : selected ? 'border-ember ring-1 ring-ember/40' : 'border-edge'
      }`}
      data-testid={`step-${index}`}
    >
      <div className="flex items-center gap-2 px-2 py-1.5">
        <button className="cursor-grab touch-none text-mist hover:text-ash" aria-label={`drag ${kind ?? 'step'}`} {...attributes} {...listeners}>
          <Grip />
        </button>
        <button className="flex min-w-0 flex-1 items-center gap-2 text-left" aria-label={open ? 'collapse step' : 'expand step'} onClick={() => setOpen((o) => !o)}>
          <span className="text-mist">{open ? <ChevronDown /> : <ChevronRight />}</span>
          <span className="text-flare"><IconC /></span>
          <code className="text-xs font-semibold text-ash">{kind ?? 'unknown'}</code>
          <span className="truncate text-xs text-mist">{stepSummary(step, kind)}</span>
        </button>
        <div className="flex items-center">
          <IconButton icon={ArrowUp} label="move up" disabled={index === 0} onClick={() => doc.apply((p) => moveStepAt(p, path, index, index - 1))} />
          <IconButton icon={ArrowDown} label="move down" disabled={index === count - 1} onClick={() => doc.apply((p) => moveStepAt(p, path, index, index + 1))} />
          <IconButton icon={Trash} tone="danger" label="remove step" onClick={() => doc.apply((p) => deleteIn(p, [...path, index]))} />
        </div>
      </div>
      {open && (
        <div className="border-t border-edge/70 bg-panel/40 p-3">
          <StepFields doc={doc} base={[...path, index, kind ?? '']} step={step} kind={kind} />
        </div>
      )}
    </li>
  );
}

// ---- per-kind forms -------------------------------------------------------
function StepFields({
  doc, base, step, kind,
}: { doc: PlanDoc; base: Path; step: Step; kind: StepKind | null }) {
  const body = obj(step[kind ?? '']);
  const set = (k: string, v: unknown) => doc.update([...base, k], v);

  switch (kind) {
    case 'request':
      return (
        <div className="space-y-3">
          <div className="grid grid-cols-[7rem_1fr] gap-2">
            <Field label="Method">
              <Select value={(body.method as string) ?? 'GET'} onChange={(e) => set('method', e.target.value)}>
                {['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS'].map((m) => <option key={m}>{m}</option>)}
              </Select>
            </Field>
            <Field label="URL">
              <TextInput aria-label="URL" value={(body.url as string) ?? ''} placeholder="/path or https://…" onChange={(e) => set('url', e.target.value)} />
            </Field>
          </div>
          <Field label="Name" hint="optional — labels the request in metrics">
            <TextInput value={(body.name as string) ?? ''} placeholder="login" onChange={(e) => set('name', e.target.value || undefined)} />
          </Field>
          <KeyValueEditor doc={doc} path={[...base, 'headers']} value={obj(body.headers)} label="Headers" keyPlaceholder="Authorization" valPlaceholder="Bearer …" />
          <KeyValueEditor doc={doc} path={[...base, 'params']} value={obj(body.params)} label="Query params" keyPlaceholder="page" valPlaceholder="1" />
          <Field label="Body" hint="raw string or ${...} template">
            <Textarea rows={3} value={typeof body.body === 'string' ? body.body : body.body == null ? '' : JSON.stringify(body.body, null, 2)} placeholder={'{ "email": "${user.email}" }'} onChange={(e) => set('body', e.target.value || undefined)} />
          </Field>
          <Disclosure label={`Assertions, checks & extracts${advancedCount(body)}`}>
            <div className="grid grid-cols-2 gap-2">
              <Field label="Timeout" hint="per-request override"><TextInput value={(body.timeout as string) ?? ''} placeholder="5s" onChange={(e) => set('timeout', e.target.value || undefined)} /></Field>
              <Field label="Follow redirects">
                <Select value={body.follow_redirects == null ? '' : String(body.follow_redirects)} onChange={(e) => set('follow_redirects', e.target.value === '' ? undefined : e.target.value === 'true')}>
                  <option value="">default</option>
                  <option value="true">yes</option>
                  <option value="false">no</option>
                </Select>
              </Field>
            </div>
            <ConditionsEditor doc={doc} path={[...base, 'assert']} items={arr(body.assert) as Cond[]} label="Assertions" hint="failures mark the request failed" />
            <ConditionsEditor doc={doc} path={[...base, 'checks']} items={arr(body.checks) as Cond[]} label="Checks" hint="recorded to the checks metric, never fail the request" />
            <ExtractorsEditor doc={doc} path={[...base, 'extract']} items={arr(body.extract) as Record<string, unknown>[]} />
          </Disclosure>
        </div>
      );

    case 'think_time':
      return <ThinkTimeFields body={body} base={base} doc={doc} />;

    case 'js':
      return (
        <Field label="JavaScript" hint="runs in the VU sandbox (session, http, check…)">
          <Textarea rows={3} value={typeof step.js === 'string' ? step.js : ''} placeholder="session.vars.token = http.json().token" onChange={(e) => doc.update([...base], e.target.value)} />
        </Field>
      );

    case 'group':
      return (
        <div className="space-y-3">
          <Field label="Name"><TextInput value={(body.name as string) ?? ''} placeholder="checkout" onChange={(e) => set('name', e.target.value)} /></Field>
          <FlowEditor doc={doc} path={[...base, 'steps']} steps={arr(body.steps) as Step[]} title="Grouped steps" dense />
        </div>
      );

    case 'repeat':
      return (
        <div className="space-y-3">
          <div className="grid grid-cols-2 gap-2">
            <Field label="Times"><NumField value={body.times as number} onChange={(v) => set('times', v)} /></Field>
            <Field label="Counter var" hint="optional"><TextInput value={(body.counter as string) ?? ''} placeholder="attempt" onChange={(e) => set('counter', e.target.value || undefined)} /></Field>
          </div>
          <FlowEditor doc={doc} path={[...base, 'steps']} steps={arr(body.steps) as Step[]} title="Repeated steps" dense />
        </div>
      );

    case 'while':
      return (
        <div className="space-y-3">
          <Field label="Condition" hint="JS expression — loops while truthy"><TextInput value={(body.condition as string) ?? ''} placeholder="Number(session.vars.n) < 10" onChange={(e) => set('condition', e.target.value)} /></Field>
          <Field label="Max iterations" hint="optional safety cap"><NumField value={body.max_iterations as number} onChange={(v) => set('max_iterations', v)} /></Field>
          <FlowEditor doc={doc} path={[...base, 'steps']} steps={arr(body.steps) as Step[]} title="Loop body" dense />
        </div>
      );

    case 'if':
      return (
        <div className="space-y-3">
          <Field label="Condition" hint="JS expression"><TextInput value={(body.condition as string) ?? ''} placeholder="http.status === 200" onChange={(e) => set('condition', e.target.value)} /></Field>
          <FlowEditor doc={doc} path={[...base, 'then']} steps={arr(body.then) as Step[]} title="Then" dense />
          <FlowEditor doc={doc} path={[...base, 'else']} steps={arr(body.else) as Step[]} title="Else" dense />
        </div>
      );

    case 'foreach':
      return (
        <div className="space-y-3">
          <Field label="Items" hint="a ${...} template/array, an inline array, or a js: expression returning an array">
            <TextInput value={typeof body.items === 'string' ? body.items : body.items == null ? '' : JSON.stringify(body.items)} placeholder="${users}" onChange={(e) => set('items', e.target.value)} />
          </Field>
          <div className="grid grid-cols-2 gap-2">
            <Field label="Item var" hint="default: item"><TextInput value={(body.var as string) ?? ''} placeholder="item" onChange={(e) => set('var', e.target.value || undefined)} /></Field>
            <Field label="Index var" hint="default: index"><TextInput value={(body.index as string) ?? ''} placeholder="index" onChange={(e) => set('index', e.target.value || undefined)} /></Field>
          </div>
          <FlowEditor doc={doc} path={[...base, 'steps']} steps={arr(body.steps) as Step[]} title="Per-item steps" dense />
        </div>
      );

    case 'switch':
      return <SwitchFields doc={doc} base={base} body={body} />;

    case 'during':
      return (
        <div className="space-y-3">
          <div className="grid grid-cols-2 gap-2">
            <Field label="Duration"><TextInput value={(body.duration as string) ?? ''} placeholder="10s" onChange={(e) => set('duration', e.target.value)} /></Field>
            <Field label="Counter var" hint="optional"><TextInput value={(body.counter as string) ?? ''} placeholder="index" onChange={(e) => set('counter', e.target.value || undefined)} /></Field>
          </div>
          <FlowEditor doc={doc} path={[...base, 'steps']} steps={arr(body.steps) as Step[]} title="Repeated for duration" dense />
        </div>
      );

    case 'retry':
      return (
        <div className="space-y-3">
          <div className="grid grid-cols-3 gap-2">
            <Field label="Times" hint="default 3"><NumField value={body.times as number} onChange={(v) => set('times', v)} /></Field>
            <Field label="Backoff" hint="optional pause"><TextInput value={(body.backoff as string) ?? ''} placeholder="1s" onChange={(e) => set('backoff', e.target.value || undefined)} /></Field>
            <Field label="Until" hint="optional JS success cond"><TextInput value={(body.until as string) ?? ''} placeholder="http.status === 200" onChange={(e) => set('until', e.target.value || undefined)} /></Field>
          </div>
          <FlowEditor doc={doc} path={[...base, 'steps']} steps={arr(body.steps) as Step[]} title="Attempted steps" dense />
        </div>
      );

    case 'parallel':
      return <BranchesFields doc={doc} base={base} branches={arr(body.branches) as Step[][]} />;

    case 'rendezvous':
      return (
        <div className="grid grid-cols-3 gap-2">
          <Field label="Name" hint="VUs sharing a name sync together"><TextInput value={(body.name as string) ?? ''} placeholder="sync" onChange={(e) => set('name', e.target.value)} /></Field>
          <Field label="Users" hint="release at N waiting"><NumField value={body.users as number} onChange={(v) => set('users', v)} /></Field>
          <Field label="Timeout" hint="default 30s"><TextInput value={(body.timeout as string) ?? ''} placeholder="30s" onChange={(e) => set('timeout', e.target.value || undefined)} /></Field>
        </div>
      );

    case 'random':
      return <RandomFields doc={doc} base={base} body={body} />;

    default:
      return <p className="text-xs text-mist">Unknown step kind.</p>;
  }
}

function ThinkTimeFields({ doc, base, body }: { doc: PlanDoc; base: Path; body: Record<string, unknown> }) {
  const type = (body.type as string) ?? 'constant';
  const seeds: Record<string, Record<string, unknown>> = {
    constant: { type: 'constant', duration: '1s' },
    uniform: { type: 'uniform', min: '1s', max: '3s' },
    gaussian: { type: 'gaussian', mean: '1s', std_dev: '500ms' },
  };
  const set = (k: string, v: unknown) => doc.update([...base, k], v);
  return (
    <div className="space-y-3">
      <Field label="Distribution">
        <Select value={type} onChange={(e) => doc.update([...base], seeds[e.target.value] as unknown as Json)}>
          <option value="constant">constant</option>
          <option value="uniform">uniform</option>
          <option value="gaussian">gaussian</option>
        </Select>
      </Field>
      {type === 'constant' && <Field label="Duration"><TextInput value={(body.duration as string) ?? ''} placeholder="1s" onChange={(e) => set('duration', e.target.value)} /></Field>}
      {type === 'uniform' && (
        <div className="grid grid-cols-2 gap-2">
          <Field label="Min"><TextInput value={(body.min as string) ?? ''} placeholder="1s" onChange={(e) => set('min', e.target.value)} /></Field>
          <Field label="Max"><TextInput value={(body.max as string) ?? ''} placeholder="3s" onChange={(e) => set('max', e.target.value)} /></Field>
        </div>
      )}
      {type === 'gaussian' && (
        <div className="grid grid-cols-2 gap-2">
          <Field label="Mean"><TextInput value={(body.mean as string) ?? ''} placeholder="1s" onChange={(e) => set('mean', e.target.value)} /></Field>
          <Field label="Std dev"><TextInput value={(body.std_dev as string) ?? ''} placeholder="500ms" onChange={(e) => set('std_dev', e.target.value)} /></Field>
        </div>
      )}
    </div>
  );
}

function SwitchFields({ doc, base, body }: { doc: PlanDoc; base: Path; body: Record<string, unknown> }) {
  const cases = obj(body.cases);
  const names = Object.keys(cases);
  function addCase() {
    let name = 'case';
    let n = 2;
    while (name in cases) name = `case_${n++}`;
    doc.update([...base, 'cases', name], []);
  }
  return (
    <div className="space-y-3">
      <Field label="Value" hint="${...} template, rendered then matched to a case key">
        <TextInput value={(body.value as string) ?? ''} placeholder="${session.vars.tier}" onChange={(e) => doc.update([...base, 'value'], e.target.value)} />
      </Field>
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <span className="text-[11px] font-semibold uppercase tracking-wider text-mist">Cases · {names.length}</span>
          <Button size="sm" icon={Plus} onClick={addCase}>Case</Button>
        </div>
        {names.map((name) => (
          <div key={name} className="rounded-lg border border-edge bg-coal p-2">
            <div className="mb-2 flex items-center gap-2">
              <TextInput
                className="h-7 py-0.5 text-xs"
                defaultValue={name}
                aria-label={`case name ${name}`}
                onBlur={(e) => { if (e.target.value !== name) doc.apply((p) => renameKey(p, [...base, 'cases'], name, e.target.value)); }}
              />
              <IconButton icon={Trash} tone="danger" label={`remove case ${name}`} onClick={() => doc.apply((p) => deleteIn(p, [...base, 'cases', name]))} />
            </div>
            <FlowEditor doc={doc} path={[...base, 'cases', name]} steps={arr(cases[name]) as Step[]} title="When matched" dense />
          </div>
        ))}
      </div>
      <FlowEditor doc={doc} path={[...base, 'default']} steps={arr(body.default) as Step[]} title="Default (no match)" dense />
    </div>
  );
}

function BranchesFields({ doc, base, branches }: { doc: PlanDoc; base: Path; branches: Step[][] }) {
  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <span className="text-[11px] font-semibold uppercase tracking-wider text-mist">Branches · {branches.length}</span>
        <Button size="sm" icon={Plus} onClick={() => doc.apply((p) => appendTo(p, [...base, 'branches'], []))}>Branch</Button>
      </div>
      {branches.length === 0 && <p className="rounded-lg border border-dashed border-edge px-3 py-3 text-center text-xs text-mist">Add a branch to run steps concurrently.</p>}
      {branches.map((branch, i) => (
        <div key={i} className="rounded-lg border border-edge bg-coal p-2">
          <div className="mb-2 flex items-center justify-between">
            <Badge tone="ember">branch {i + 1}</Badge>
            <IconButton icon={Trash} tone="danger" label={`remove branch ${i + 1}`} onClick={() => doc.apply((p) => deleteIn(p, [...base, 'branches', i]))} />
          </div>
          <FlowEditor doc={doc} path={[...base, 'branches', i]} steps={arr(branch) as Step[]} title="Concurrent steps" dense />
        </div>
      ))}
    </div>
  );
}

function RandomFields({ doc, base, body }: { doc: PlanDoc; base: Path; body: Record<string, unknown> }) {
  const choices = arr(body.choices) as Record<string, unknown>[];
  return (
    <div className="space-y-3">
      <Field label="Strategy">
        <Select value={(body.strategy as string) ?? 'weighted'} onChange={(e) => doc.update([...base, 'strategy'], e.target.value)}>
          <option value="weighted">weighted</option>
          <option value="uniform">uniform</option>
          <option value="round_robin">round_robin</option>
        </Select>
      </Field>
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <span className="text-[11px] font-semibold uppercase tracking-wider text-mist">Choices · {choices.length}</span>
          <Button size="sm" icon={Plus} onClick={() => doc.apply((p) => appendTo(p, [...base, 'choices'], { steps: [] }))}>Choice</Button>
        </div>
        {choices.map((choice, i) => (
          <div key={i} className="rounded-lg border border-edge bg-coal p-2">
            <div className="mb-2 grid grid-cols-[1fr_6rem_2rem] items-end gap-2">
              <Field label="Name"><TextInput className="h-7 py-0.5 text-xs" value={(choice.name as string) ?? ''} placeholder={`choice ${i + 1}`} onChange={(e) => doc.update([...base, 'choices', i, 'name'], e.target.value || undefined)} /></Field>
              <Field label="Weight"><NumField className="h-7 py-0.5 text-xs" value={choice.weight as number} onChange={(v) => doc.update([...base, 'choices', i, 'weight'], v)} /></Field>
              <IconButton icon={Trash} tone="danger" label={`remove choice ${i + 1}`} onClick={() => doc.apply((p) => deleteIn(p, [...base, 'choices', i]))} />
            </div>
            <FlowEditor doc={doc} path={[...base, 'choices', i, 'steps']} steps={arr(choice.steps) as Step[]} title="Steps" dense />
          </div>
        ))}
      </div>
    </div>
  );
}

// ---- small helpers --------------------------------------------------------
function NumField({
  value, onChange, className, id,
}: { value: number | undefined; onChange: (v: number | undefined) => void; className?: string; id?: string }) {
  return (
    <NumberInput
      id={id}
      className={className}
      value={value ?? ''}
      onChange={(e) => onChange(e.target.value === '' ? undefined : Number(e.target.value))}
    />
  );
}

// A key/value object editor (request headers / query params). Rows are held in
// local state so a freshly-added empty row survives until it's filled in; only
// non-empty keys are written back to the model.
function KeyValueEditor({
  doc, path, value, label, keyPlaceholder, valPlaceholder,
}: { doc: PlanDoc; path: Path; value: Record<string, unknown>; label: string; keyPlaceholder?: string; valPlaceholder?: string }) {
  const seed = useRef(Object.entries(value).map(([k, v], i) => ({ id: i, k, v: String(v) })));
  const nextId = useRef(seed.current.length);
  const [rows, setRows] = useState(seed.current);

  function commit(next: { id: number; k: string; v: string }[]) {
    setRows(next);
    const out: Record<string, unknown> = {};
    for (const r of next) if (r.k.trim()) out[r.k] = r.v;
    doc.update(path, Object.keys(out).length ? (out as unknown as Json) : undefined);
  }

  return (
    <div role="group" aria-label={label} className="flex flex-col gap-1.5">
      <span className="text-[11px] font-semibold uppercase tracking-wide text-smoke">{label}</span>
      {rows.map((r, i) => (
        <div key={r.id} className="grid grid-cols-[1fr_1fr_2rem] gap-1.5">
          <TextInput className="py-1 text-xs" value={r.k} aria-label={`${label} key ${i + 1}`} placeholder={keyPlaceholder} onChange={(e) => commit(rows.map((x) => (x.id === r.id ? { ...x, k: e.target.value } : x)))} />
          <TextInput className="py-1 text-xs" value={r.v} aria-label={`${label} value ${i + 1}`} placeholder={valPlaceholder} onChange={(e) => commit(rows.map((x) => (x.id === r.id ? { ...x, v: e.target.value } : x)))} />
          <IconButton icon={Trash} tone="danger" label={`remove ${label} row ${i + 1}`} onClick={() => commit(rows.filter((x) => x.id !== r.id))} />
        </div>
      ))}
      <div>
        <Button size="sm" variant="ghost" icon={Plus} onClick={() => { const id = nextId.current++; setRows((rs) => [...rs, { id, k: '', v: '' }]); }}>
          {label.replace(/s$/, '')}
        </Button>
      </div>
    </div>
  );
}

// ---- request assertions / checks / extractors -----------------------------
type Cond = Record<string, unknown>;

const CONDITION_TYPES = ['status', 'jsonpath', 'body_contains', 'body_matches', 'header', 'duration', 'size', 'xpath', 'js'] as const;
const CONDITION_SEED: Record<string, Cond> = {
  status: { type: 'status', equals: 200 },
  jsonpath: { type: 'jsonpath', expression: '$.' },
  body_contains: { type: 'body_contains', value: '' },
  body_matches: { type: 'body_matches', pattern: '' },
  header: { type: 'header', header: '' },
  duration: { type: 'duration', max: '500ms' },
  size: { type: 'size' },
  xpath: { type: 'xpath', expression: '' },
  js: { type: 'js', expression: '' },
};

function advancedCount(body: Record<string, unknown>): string {
  const n = arr(body.assert).length + arr(body.checks).length + arr(body.extract).length
    + (body.timeout ? 1 : 0) + (body.follow_redirects != null ? 1 : 0);
  return n ? ` · ${n}` : '';
}

function ConditionsEditor({ doc, path, items, label, hint }: { doc: PlanDoc; path: Path; items: Cond[]; label: string; hint?: string }) {
  return (
    <div className="space-y-2">
      <div className="flex items-end justify-between">
        <div>
          <span className="text-[11px] font-semibold uppercase tracking-wider text-mist">{label} · {items.length}</span>
          {hint && <p className="text-[11px] text-mist">{hint}</p>}
        </div>
        <Button size="sm" icon={Plus} onClick={() => doc.apply((p) => appendTo(p, path, { ...CONDITION_SEED.status }))}>{label.replace(/s$/, '')}</Button>
      </div>
      {items.map((c, i) => {
        const t = (c.type as string) || 'status';
        const set = (k: string, v: unknown) => doc.update([...path, i, k], v);
        return (
          <div key={i} className="space-y-2 rounded-lg border border-edge bg-coal p-2">
            <div className="grid grid-cols-[8rem_1fr_2rem] items-end gap-2">
              <Field label="Type"><Select value={t} onChange={(e) => doc.update([...path, i], { ...CONDITION_SEED[e.target.value] } as unknown as Json)}>{CONDITION_TYPES.map((x) => <option key={x} value={x}>{x}</option>)}</Select></Field>
              <Field label="Label (optional)"><TextInput value={(c.name as string) ?? ''} onChange={(e) => set('name', e.target.value || undefined)} /></Field>
              <IconButton icon={Trash} tone="danger" label={`remove ${label} ${i + 1}`} onClick={() => doc.apply((p) => deleteIn(p, [...path, i]))} />
            </div>
            <ConditionFields t={t} c={c} set={set} />
          </div>
        );
      })}
    </div>
  );
}

function ConditionFields({ t, c, set }: { t: string; c: Cond; set: (k: string, v: unknown) => void }) {
  switch (t) {
    case 'status':
      return (
        <div className="grid grid-cols-2 gap-2">
          <Field label="Equals" hint="exact status"><NumField value={c.equals as number} onChange={(v) => set('equals', v)} /></Field>
          <Field label="Matches" hint="regex e.g. 2.."><TextInput value={(c.matches as string) ?? ''} onChange={(e) => set('matches', e.target.value || undefined)} /></Field>
        </div>
      );
    case 'jsonpath':
    case 'xpath':
      return (
        <div className="space-y-2">
          <Field label="Expression"><TextInput value={(c.expression as string) ?? ''} placeholder={t === 'jsonpath' ? '$.data.id' : '//div[@id]'} onChange={(e) => set('expression', e.target.value)} /></Field>
          <div className="grid grid-cols-[1fr_auto] items-center gap-3">
            <Field label="Equals (optional)"><TextInput value={(c.equals as string) ?? ''} onChange={(e) => set('equals', e.target.value || undefined)} /></Field>
            <Checkbox label="must exist" checked={!!c.exists} onChange={(v) => set('exists', v || undefined)} />
          </div>
        </div>
      );
    case 'body_contains':
      return (
        <div className="space-y-2">
          <Field label="Value"><TextInput value={(c.value as string) ?? ''} onChange={(e) => set('value', e.target.value)} /></Field>
          <Checkbox label="negate (must NOT contain)" checked={!!c.negate} onChange={(v) => set('negate', v || undefined)} />
        </div>
      );
    case 'body_matches':
      return (
        <div className="space-y-2">
          <Field label="Pattern"><TextInput value={(c.pattern as string) ?? ''} placeholder={'"id":\\s*\\d+'} onChange={(e) => set('pattern', e.target.value)} /></Field>
          <Checkbox label="negate (must NOT match)" checked={!!c.negate} onChange={(v) => set('negate', v || undefined)} />
        </div>
      );
    case 'header':
      return (
        <div className="space-y-2">
          <Field label="Header"><TextInput value={(c.header as string) ?? ''} placeholder="Content-Type" onChange={(e) => set('header', e.target.value)} /></Field>
          <div className="grid grid-cols-2 gap-2">
            <Field label="Equals"><TextInput value={(c.equals as string) ?? ''} onChange={(e) => set('equals', e.target.value || undefined)} /></Field>
            <Field label="Contains"><TextInput value={(c.contains as string) ?? ''} onChange={(e) => set('contains', e.target.value || undefined)} /></Field>
          </div>
          <Checkbox label="must exist" checked={!!c.exists} onChange={(v) => set('exists', v || undefined)} />
        </div>
      );
    case 'duration':
      return <Field label="Max" hint="e.g. 500ms"><TextInput value={(c.max as string) ?? ''} placeholder="500ms" onChange={(e) => set('max', e.target.value)} /></Field>;
    case 'size':
      return (
        <div className="grid grid-cols-3 gap-2">
          <Field label="Min"><NumField value={c.min as number} onChange={(v) => set('min', v)} /></Field>
          <Field label="Max"><NumField value={c.max as number} onChange={(v) => set('max', v)} /></Field>
          <Field label="Equals"><NumField value={c.equals as number} onChange={(v) => set('equals', v)} /></Field>
        </div>
      );
    case 'js':
      return <Field label="Expression" hint="truthy passes; `response` is in scope"><TextInput value={(c.expression as string) ?? ''} placeholder="http.json().ok === true" onChange={(e) => set('expression', e.target.value)} /></Field>;
    default:
      return null;
  }
}

const EXTRACTOR_TYPES = ['jsonpath', 'regex', 'xpath', 'css', 'boundary', 'header'] as const;
const EXTRACTOR_SEED: Record<string, Cond> = {
  jsonpath: { type: 'jsonpath', name: '', expression: '$.' },
  regex: { type: 'regex', name: '', expression: '' },
  xpath: { type: 'xpath', name: '', expression: '' },
  css: { type: 'css', name: '', expression: '' },
  boundary: { type: 'boundary', name: '', left: '', right: '' },
  header: { type: 'header', name: '', header: '' },
};

function ExtractorsEditor({ doc, path, items }: { doc: PlanDoc; path: Path; items: Record<string, unknown>[] }) {
  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <span className="text-[11px] font-semibold uppercase tracking-wider text-mist">Extract · {items.length}</span>
        <Button size="sm" icon={Plus} onClick={() => doc.apply((p) => appendTo(p, path, { ...EXTRACTOR_SEED.jsonpath }))}>Extractor</Button>
      </div>
      {items.map((x, i) => {
        const t = (x.type as string) || 'jsonpath';
        const set = (k: string, v: unknown) => doc.update([...path, i, k], v);
        return (
          <div key={i} className="space-y-2 rounded-lg border border-edge bg-coal p-2">
            <div className="grid grid-cols-[8rem_1fr_2rem] items-end gap-2">
              <Field label="Type"><Select value={t} onChange={(e) => doc.update([...path, i], { ...EXTRACTOR_SEED[e.target.value], name: x.name ?? '' } as unknown as Json)}>{EXTRACTOR_TYPES.map((k) => <option key={k} value={k}>{k}</option>)}</Select></Field>
              <Field label="Save as (var)"><TextInput value={(x.name as string) ?? ''} placeholder="token" onChange={(e) => set('name', e.target.value)} /></Field>
              <IconButton icon={Trash} tone="danger" label={`remove extractor ${i + 1}`} onClick={() => doc.apply((p) => deleteIn(p, [...path, i]))} />
            </div>
            <ExtractorFields t={t} x={x} set={set} />
            <Field label="Default (optional)" hint="used when nothing matches"><TextInput value={(x.default as string) ?? ''} onChange={(e) => set('default', e.target.value || undefined)} /></Field>
          </div>
        );
      })}
    </div>
  );
}

function ExtractorFields({ t, x, set }: { t: string; x: Cond; set: (k: string, v: unknown) => void }) {
  switch (t) {
    case 'jsonpath':
      return (
        <div className="grid grid-cols-[1fr_5rem] gap-2">
          <Field label="Expression"><TextInput value={(x.expression as string) ?? ''} placeholder="$.token" onChange={(e) => set('expression', e.target.value)} /></Field>
          <Field label="Index"><NumField value={x.index as number} onChange={(v) => set('index', v)} /></Field>
        </div>
      );
    case 'regex':
      return (
        <div className="grid grid-cols-[1fr_5rem_5rem] gap-2">
          <Field label="Expression"><TextInput value={(x.expression as string) ?? ''} placeholder="token=(\\w+)" onChange={(e) => set('expression', e.target.value)} /></Field>
          <Field label="Group"><NumField value={x.group as number} onChange={(v) => set('group', v)} /></Field>
          <Field label="Index"><NumField value={x.index as number} onChange={(v) => set('index', v)} /></Field>
        </div>
      );
    case 'xpath':
      return <Field label="Expression"><TextInput value={(x.expression as string) ?? ''} placeholder="//token/text()" onChange={(e) => set('expression', e.target.value)} /></Field>;
    case 'css':
      return (
        <div className="grid grid-cols-[1fr_7rem_5rem] gap-2">
          <Field label="Selector"><TextInput value={(x.expression as string) ?? ''} placeholder="input#csrf" onChange={(e) => set('expression', e.target.value)} /></Field>
          <Field label="Attribute"><TextInput value={(x.attribute as string) ?? ''} placeholder="value" onChange={(e) => set('attribute', e.target.value || undefined)} /></Field>
          <Field label="Index"><NumField value={x.index as number} onChange={(v) => set('index', v)} /></Field>
        </div>
      );
    case 'boundary':
      return (
        <div className="grid grid-cols-[1fr_1fr_5rem] gap-2">
          <Field label="Left"><TextInput value={(x.left as string) ?? ''} placeholder="token=" onChange={(e) => set('left', e.target.value)} /></Field>
          <Field label="Right"><TextInput value={(x.right as string) ?? ''} placeholder="&" onChange={(e) => set('right', e.target.value)} /></Field>
          <Field label="Index"><NumField value={x.index as number} onChange={(v) => set('index', v)} /></Field>
        </div>
      );
    case 'header':
      return <Field label="Header"><TextInput value={(x.header as string) ?? ''} placeholder="Set-Cookie" onChange={(e) => set('header', e.target.value)} /></Field>;
    default:
      return null;
  }
}

function Section({ id, title, subtitle, action, children }: { id?: string; title: string; subtitle?: string; action?: React.ReactNode; children: React.ReactNode }) {
  return (
    <section id={id} className="scroll-mt-4 space-y-3">
      <div className="flex items-end justify-between">
        <div>
          <h2 className="text-sm font-bold text-white">{title}</h2>
          {subtitle && <p className="text-xs text-mist">{subtitle}</p>}
        </div>
        {action}
      </div>
      {children}
    </section>
  );
}
