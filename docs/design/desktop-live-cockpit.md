# Design RFC — loadr Desktop: the live run cockpit

**Status:** draft / for review
**Goal:** turn the desktop app from a *plan editor that can run tests* into a
**world-class live run cockpit** — the surface an engineer or tester opens and
immediately understands: author a test (AI-assisted), **run it and steer the
load by hand**, watch load **and the target system** correlate in real time,
see pass/fail and *why* at a glance, diff against a baseline, and ship it to CI.
The visual bar is Grafana / Datadog / Linear / Vercel — dark, data-dense, calm,
fast — not a utilitarian wrapper around a CLI.

---

## 1. Motivation

The desktop app (Electron 33 + Vite + React 19 + Tailwind 4 + Monaco) already
does the hard parts: a real plan editor, AI generation (prompt or from a repo),
local runs via the bundled CLI, run history and a baseline-comparison table.
But the **live run view** is four tiles + a hand-rolled bar chart fed by parsing
one stdout line per second (`running HH:MM:SS vus N rps X p95 Yms failed Z`).
That's functional, not *world-class*, and it under-sells loadr's real strengths:
correct distributed stats, the new system-metric **correlation** (`observe`),
threshold gating, and live load control.

Engineers and testers arriving from k6/Grafana, JMeter or Postman have a built-in
expectation: a polished, real-time dashboard with proper charts, clear SLO
status, and a tight author→run→understand loop. We should meet — and beat — it,
and the desktop app is the best place to do it (it's the only surface that also
has AI-authoring-from-your-repo).

---

## 2. Non-goals

- Not a rewrite. Reuse the existing editor, AI panel, history, IPC and Tailwind
  theme; replace/upgrade the **run** experience and add control.
- Not a new metrics protocol. Consume what the engine already exposes (stdout
  progress today; the web-UI SSE snapshot stream for the rich path).
- Not a replacement for the browser web UI; the desktop *fleet* mode (phase 3)
  is a native client for the same controller API.

---

## 3. Design system (the world-class bar)

This is the heart of the RFC: the rules that make it feel premium and familiar.

### Principles
- **Dark-first, calm, data-dense.** Near-black canvas, content forward, no
  chrome for chrome's sake. Whitespace and hierarchy over borders and boxes.
- **Numbers are the UI.** Every metric in `font-mono`, `tabular-nums`, right
  aligned; deltas carry sign + colour. A tester reads pass/fail and magnitude
  without thinking.
- **One accent.** A single ember accent on neutrals; colour means *something*
  (accent = loadr/primary, green/amber/red = SLO state). Never decorative.
- **Motion is feedback, not flair.** Sub-second chart updates, smooth dial,
  a quiet live-pulse; everything respects `prefers-reduced-motion`.

### Brand & colour (one fix to make first)
The app's `@theme` (`src/renderer/app.css`) currently uses a pure red as
"ember" (`--color-ember:#ef4444`), but loadr's logo, site, datasheet and HTML
report all use the **orange-ember `#ff5a36`**. Align the desktop accent to
`#ff5a36` so the **report ↔ app feel like one product** (the report charts we
ship are already ember `#ff5a36`). Keep the existing neutral ramp (`ink → coal →
panel → edge`) and semantic `ok/warn`; introduce a `danger` red distinct from the
brand accent so "brand" and "failure" never collide.

```
canvas   #07070a   panel   #141419   edge   #232330
accent   #ff5a36   (was #ef4444)      accent-dim #b83b22
ok       #4ade80   warn   #fbbf24     danger #f0493f
text     #d1d5db   dim    #9ca3af     faint  #6b7280
font: Inter (UI) · JetBrains Mono / SF Mono (numbers, code)
```

### Charts
Adopt the **HTML report's chart language** verbatim (inline SVG line charts,
shared hover crosshair, ember series, gridlines, `tabular-nums` readouts) so
there is one charting vocabulary across report and app. Small-multiples for
system metrics (one auto-scaled panel per metric), exactly as the report now
does. No heavyweight charting dependency — the report's renderer is ~200 lines
and already battle-tested; port it to a React `<TimeChart>` component.

### Layout & density
- A **three-zone run view**: a slim left rail (run controls + SLO board), a main
  grid of charts, a right context drawer (selected-iteration waterfall / Explain).
- 8px spacing grid; cards `--panel` on `--canvas`, 12px radius, 1px `--edge`.
- Responsive grid (`repeat(auto-fit, minmax(360px, 1fr))`) — already the report's
  approach.

### States & semantics
- SLO state is the loudest signal: a **traffic-light board** (green/amber/red)
  that flips the instant a threshold crosses, with the offending metric pulsing.
- Empty/first-run, running, finished, aborted, and error states each get a
  designed treatment (no raw stack traces — the app already has `cliError`).

### Accessibility & input
- WCAG-AA contrast on text; never colour-only (icons + labels on SLO states).
- **Keyboard-first:** ⌘K command palette, ⌘↵ run, ⌘. stop, ⌘D duplicate, `[`/`]`
  to scrub the timeline. Engineers expect it.

---

## 4. Architecture

### Live data: upgrade from stdout to the SSE snapshot stream
Today the renderer parses the once-per-second stdout progress line — only
`rps / vus / p95 / failed`. For a real cockpit (full latency percentiles, error
rate, per-metric series, and the `observe` correlation), drive the run with the
engine's **live web UI** and consume its **Server-Sent Events**:

```
main:  spawn `loadr run <plan> --ui --ui-bind 127.0.0.1:<port>` (+ --summary-export)
renderer:  EventSource → http://127.0.0.1:<port>/api/runs/{id}/stream
           ├── event: "snapshot"  → full Snapshot (all metrics + aggregates + timeline)
           └── event: "status"    → { state, passed }
```

This reuses the existing `loadr-plugin-webui` stream (`/api/runs/{id}/stream`,
`live_run_stream`) — no new engine surface. The stdout parser stays as a
zero-dependency fallback when `--ui` isn't available. (Bundling the webui plugin
with the desktop binary is an open question — §9.)

### Live control: the dial
The dial drives an `externally-controlled` scenario. The web UI already scales
externally-controlled runs at runtime; the desktop posts the same control
command (target VUs / arrival rate) to the local run's API. So the same `--ui`
channel gives us **both** rich metrics *and* live control — one integration.

### System correlation
`observe:` metrics already ride on the summary timeline (post-run) and render in
the report. The cockpit shows them as live small-multiples once observe is
streamed into snapshots (a later observe phase); until then, the correlation
panel populates at end-of-run from the summary, same as the report.

---

## 5. Feature set (phased)

### Phase 1 — the local cockpit (no new infra)
- **Redesigned run view** with real `<TimeChart>` charts: throughput, latency
  (p50/p95/p99/avg), active VUs, error rate — the report's language, live.
- **SLO board**: threshold traffic-lights, live; surfaces `abort_on_fail` and
  shows the run **auto-aborting** on breach.
- **System-correlation panel**: the `observe` small-multiples (CPU / mem / DB /
  cache) beside the load charts.
- **Run-vs-baseline diff**: upgrade the existing comparison table into a visual
  overlay + delta badges.
- **Journey waterfall**: per-iteration step timings, extracted vars, check
  pass/fail in the right drawer.
- **"Explain this result"**: a button that sends the summary (+ observe) to the
  existing AI layer (`src/main/ai.ts`) for a plain-English diagnosis.

### Phase 2 — interactive control
- **Live throughput dial** (externally-controlled via the `--ui` control API).
- **Command palette + keyboard shortcuts**; **timeline scrubber/replay** on a
  finished run (the report already has a hover crosshair to build on).

### Phase 3 — fleet cockpit
- **Connect to controller** mode: the desktop becomes a native client for a
  running controller (`/api/runs`, SSE), with **animated fleet** (agents joining)
  and a **total-RPS odometer**; **spectator/share** a run.

---

## 6. Component specs (new React components, Tailwind + the report's SVG)

| Component | Responsibility |
|---|---|
| `<Cockpit>` | the three-zone run view; owns the EventSource + run lifecycle |
| `<TimeChart>` | port of the report's SVG line chart (series, crosshair, axes) |
| `<Dial>` | draggable VU/RPS knob → posts scale commands; keyboard + scroll |
| `<SloBoard>` | threshold traffic-lights; flips on snapshot threshold state |
| `<CorrelationPanel>` | `observe` small-multiples (one auto-scaled panel/metric) |
| `<Waterfall>` | per-iteration step timeline in the context drawer |
| `<DiffOverlay>` | this run vs baseline, delta badges |
| `<ExplainCard>` | AI diagnosis of the finished run |

`RunMonitor.tsx` is refactored into `<Cockpit>`; `shared/monitor.ts` (sample
accumulation) and `shared/results.ts` (parsing) are extended to ingest the SSE
snapshot shape alongside the stdout fallback. Tests: Vitest for the shared
ingest/scale logic; Playwright-for-Electron for the run view.

---

## 7. Demo storyboard (the cockpit video)
1. **Describe it** → AI generates the plan.
2. **Run + grab the dial** → ramp load; charts respond live.
3. **Correlation panel** lights up; auto-annotation flags the **knee** ("DB-bound").
4. **SLO board** flips red → run **auto-aborts**.
5. **"Explain this result"** → plain-English diagnosis.
6. **Diff vs last run** → p95 regression badge.
7. (Phase 3) **Connect to a controller** → same views, fleet-wide, odometer spinning.

*Describe it, steer it, understand it, ship it* — loadr's whole pitch in 90s,
and uniquely a desktop story.

---

## 8. Rollout
1. Brand/token alignment (`#ff5a36`) + extract `<TimeChart>` from the report.
2. SSE ingest path behind a flag, stdout fallback retained.
3. Ship Phase 1 view; then the dial; then fleet mode.
Each phase is independently shippable in a `desktop-v*` release.

---

## 9. Open questions
- **Accent:** confirm aligning the desktop ember to `#ff5a36` (recommended) vs
  keeping `#ef4444`.
- **webui bundling:** ship the `loadr-plugin-webui` SSE server inside the desktop
  binary (so `--ui` always works offline), or keep stdout as the default and use
  SSE only when present?
- **Dial control API:** confirm the externally-controlled scale endpoint shape on
  the local `--ui` server (and whether non-externally-controlled scenarios should
  expose a read-only dial).
- **Observe live:** the correlation panel is end-of-run until observe streams into
  snapshots — acceptable for Phase 1?
- **Explain provider:** reuse the user's configured AI provider/key from the AI
  panel for "Explain this result"? (Recommended — no extra setup.)
