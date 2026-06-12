# Architecture overview

(See also [`ARCHITECTURE.md`](https://github.com/reaandrew/loadr.io/blob/main/ARCHITECTURE.md)
in the repository root.)

## Crate layout

```text
loadr-config     YAML schema (serde + schemars), validation, ${...} templates
loadr-core       engine: executors, VU pool, flow interpreter, metrics,
                 thresholds, checks, extraction, data feeds, cookies;
                 traits: ProtocolHandler, ScriptEngine, Output
loadr-protocols  HTTP/1.1+2 (hyper + custom timing connector), WS, gRPC,
                 GraphQL, TCP, UDP — implements ProtocolHandler
loadr-js         QuickJS runtime — implements ScriptEngine/VuScript
loadr-outputs    JSONL, CSV, Prometheus, InfluxDB, OTLP, StatsD — implement Output
loadr-plugin-api WASM (WIT) + native (abi_stable) plugin loading & SDK
loadr-agent      controller/agent gRPC coordination, HDR delta merging
loadr-convert    .jmx and k6 importers
loadr-cli        wiring + UX
loadr-plugin-webui  the management UI (axum + embedded SPA)
```

Dependency rule: everything depends *down* onto `loadr-core`/`loadr-config`;
core knows nothing about concrete protocols, JS engines or outputs — only
trait objects. That keeps each decision (QuickJS, hyper, tonic…) replaceable.

## Execution data flow

```text
YAML ──parse/validate──▶ TestPlan ──compile──▶ ScenarioPrograms
                                        │
       Scheduler ◀─ executors ─ VuPool ─┘     (per scenario)
           │ iteration
           ▼
  FlowInterpreter ── steps ──▶ ProtocolHandler (per-VU cookies, pools, vars)
           │ hooks                  │ samples
           ▼                        ▼
      VuScript (JS)            MetricsBus ─▶ Aggregator ─▶ thresholds
                                                │            outputs
                                                │            web UI (SSE)
                                                └─▶ controller stream (distributed)
```

The `MetricsBus` is an unbounded mpsc fan-in; the aggregator drains it,
snapshots once per second for live consumers, evaluates thresholds
continuously, and produces the final summary.

## Individual decisions

- [ADR-001: JavaScript runtime](001-js-runtime.md) — QuickJS over V8/Bun
- [ADR-002: Plugin system](002-plugins.md) — WASM components + abi_stable
- [ADR-003: Coordination protocol](003-coordination.md) — gRPC, HDR deltas
- [ADR-004: HTTP stack](004-http-stack.md) — hyper + hand-rolled timing connector
- [ADR-005: Metrics engine](005-metrics.md) — HDR histograms, tag series
- [ADR-006: Executor model](006-executors.md) — open vs closed models
