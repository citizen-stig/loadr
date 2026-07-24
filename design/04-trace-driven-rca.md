# Design Spec: Trace-Driven Root Cause

**Status:** Draft · **Feature family:** Observability / Reporting

## 1. Goal & user story

Today loadr can already stamp a W3C `traceparent` on every HTTP request
(`defaults.http.tracing: true`, plumbed through `HttpDefaults.tracing` in
`crates/loadr-config/src/plan.rs:145` and injected by `make_traceparent()` in
`crates/loadr-protocols/src/http.rs:866`). But the generated trace id is thrown
away the instant the header is built — nothing captures it, so a p99 spike in the
HTML report is a dead end: you see *that* something was slow, never *why*.

**Trace-Driven Root Cause (RCA)** closes the loop. loadr keeps the trace ids of
the slowest and errored requests, and after the run pulls those server-side
distributed traces from Tempo/Jaeger/OTLP and attaches the server spans — as a
per-sample waterfall/flamegraph — directly into the loadr HTML report, aligned
against the client-observed latency.

**One-command experience:**

```yaml
defaults:
  http:
    tracing: true            # already exists — injects traceparent
observe:
  - type: traces
    source: http://tempo:3200
    backend: tempo
    capture: { slowest: 20, errors: all }
```

```bash
loadr run --summary-export results.json test.yaml
loadr report results.json -o report.html   # now contains a "Root cause" section
```

Zero manual copy-pasting of trace ids into Grafana.

## 2. CLI / API surface

No new subcommand. RCA rides the existing `observe:` block (the same seam used
by `type: system` and `type: prometheus`, `crates/loadr-config/src/plan.rs:2069`)
and the existing `loadr report` command. New config, mirroring the
`ObserveConfig::Prometheus` variant's field style:

```yaml
observe:
  - type: traces
    source: http://tempo:3200        # backend base URL (required)
    backend: tempo                    # tempo | jaeger | otlp   (default: tempo)
    service: checkout-api             # optional server service.name filter
    token: ${env.TEMPO_TOKEN}         # optional bearer (like prometheus.token)
    capture:
      slowest: 20                     # keep N slowest http_req_duration samples
      errors: all                     # all | <int> — keep errored samples
    lookback: 5m                      # query window padding after run (default 2m)
```

`loadr run` gains one flag pair mirroring `--http-debug`, purely for opt-in
override without editing YAML:

```
--rca <BACKEND_URL>     Enable trace RCA against this backend (implies tracing)
--rca-slowest <N>       Override slowest-sample budget (default 20)
```

Output: on a run with RCA configured, the console prints one line matching the
existing observe log style (`run.rs:375`):

```
✓ resolved 18 of 20 server traces for root-cause correlation
```

Unresolved ids (backend hadn't ingested them yet) degrade gracefully — the
report shows the client sample with a "trace not found" note, never an error.

## 3. Architecture

Three integration points, all extending code that already exists — net-new
surface is deliberately small.

**(a) Capture live — `crates/loadr-core`.** The trace id currently dies in
`build_request`. Add `trace_id: Option<String>` to `ProtocolResponse`
(`crates/loadr-core/src/protocol.rs:100`). `HttpHandler::build_request`
(`http.rs:468`) already computes the traceparent when `self.tracing`; change
`make_traceparent()` to also return the 32-hex trace id, stash it on the
response. In `Flow::emit_request_metrics` (`crates/loadr-core/src/flow.rs:1246`),
where `http_req_duration`/`http_req_failed` are already emitted per request,
feed `(trace_id, duration_ms, name, status, is_error, timestamp_ms)` into a new
bounded **`RcaCollector`** — a lock-light top-N min-heap for slowest plus a
ring for errors. This is cheaper and lower-cardinality than tagging every
`Sample` with its trace id (which would explode the tag space in
`crates/loadr-core/src/metrics.rs`). The collector lives on the engine and is
drained at run end into the `Summary`.

**(b) Pull post-run — `crates/loadr-outputs/src/observe.rs`.** This is exactly
the file that already does post-run backend queries. Add a `TraceBackend` trait
and a `resolve_traces()` alongside `prometheus_range()` (`observe.rs:150`),
called from the same `collect()` dispatch that handles the `ObserveConfig`
variants. Reuse the shared `http_client::client()` already used there. The
`ObserveConfig::Traces` variant is added to the enum in `plan.rs`.
`run.rs` (post-run block at `run.rs:362`) gains a sibling call:
`observe::resolve_traces(&observe_cfg, &summary.rca_candidates).await` →
`observe::attach_traces(&mut summary, resolved)`.

**(c) Render — `crates/loadr-cli/src/report_html.rs`.** `render(&Summary)`
(`report_html.rs:12`) already composes sections and injects a
`<script type="application/json">` payload (the `ts-data` pattern at
`report_html.rs:294`, consumed by the inline chart script). Add a
`rca_section(&summary.traces)` that emits one self-contained SVG waterfall per
trace (no external assets — same constraint as the existing charts and the
Artifact CSP: inline SVG + inline JS only).

## 4. Key data structures & algorithms

New types in `loadr-core::summary` (serialized into `Summary`, added as
`#[serde(default)]` for backward-compat exactly like `timeline` was —
`summary.rs:161`):

```rust
pub struct RcaCandidate {           // captured live, backend-agnostic
    pub trace_id: String,
    pub request_name: String,
    pub status: i64,
    pub is_error: bool,
    pub client_duration_ms: f64,    // loadr-observed
    pub timestamp_ms: u64,
}

pub struct TraceAttachment {        // filled post-run from the backend
    pub candidate: RcaCandidate,
    pub root_service: String,
    pub server_duration_ms: f64,    // root span wall time
    pub spans: Vec<ServerSpan>,     // flattened, parent-linked
    pub resolved: bool,
}

pub struct ServerSpan {
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub service: String,
    pub name: String,
    pub start_us: u64,              // relative to trace start
    pub duration_us: u64,
    pub status_error: bool,
}
```

`Summary` gains `#[serde(default)] pub traces: Vec<TraceAttachment>` and a
transient (`#[serde(skip)]`) `rca_candidates: Vec<RcaCandidate>` handed from the
engine to the post-run resolver.

**Capture algorithm (RcaCollector):** for slowest, a fixed-cap binary min-heap
keyed on `client_duration_ms` — O(log N) per request, N≈20, negligible. For
errors, a `capacity`-bounded ring. De-dup on `trace_id`. When `tracing` is off
the collector is never constructed (zero overhead — same gate as today).

**Backend adapters:**
- **Tempo:** `GET {source}/api/traces/{trace_id}` → OTLP-JSON `ResourceSpans`.
  Reuse the OTLP protobuf/JSON modules already vendored in
  `crates/loadr-outputs/src/lib.rs:125` (`proto::opentelemetry`) rather than a
  new proto dep.
- **Jaeger:** `GET {source}/api/traces/{trace_id}` → Jaeger JSON.
- **OTLP:** the OTLP `TraceService` HTTP endpoint (same wire types as the
  `loadr-plugin-otlp-metrics` plugin already speaks, `plugins/loadr-plugin-otlp-metrics`).

All three flatten to `Vec<ServerSpan>`. Failures are logged and skipped, never
fatal — matching `observe::collect`'s per-source error handling.

**Correlation:** the waterfall's x-axis is span time; a marker line shows
`client_duration_ms`. The delta (`client_duration_ms − server_duration_ms`)
approximates network + queue time outside the server, surfaced as a labelled bar
so users see "8 ms in-server, 140 ms elsewhere" at a glance.

## 5. Reuse map

| Concern | Reuse (exists) | Net-new |
|---|---|---|
| traceparent generation | `make_traceparent()` `http.rs:866`, `tracing` flag | return trace id (was discarded) |
| per-request emit hook | `emit_request_metrics` `flow.rs:1246` | feed `RcaCollector` |
| response carrier | `ProtocolResponse` `protocol.rs:100` | `trace_id` field |
| config seam | `ObserveConfig` enum, validate.rs | `Traces` variant |
| post-run backend query | `observe::collect`/`prometheus_range` | `resolve_traces` + adapters |
| OTLP wire types | `loadr-outputs::proto::opentelemetry` | Jaeger JSON structs |
| HTTP client | `http_client::client()` | — |
| report section + inline JSON payload | `report_html::render`, `ts-data` | `rca_section` SVG waterfall |
| summary back-compat | `#[serde(default)]` timeline precedent | `traces` field |

Net-new is one config variant, one collector, three thin backend adapters, and
one report section. No new crate.

## 6. Testing plan

Mirrors repo conventions — no real network in unit tests, 70% coverage gate.
- **`loadr-core`:** `RcaCollector` keeps exactly the N slowest, de-dups
  trace ids, captures all errors up to cap, and is a no-op when `tracing` is
  off. `make_traceparent` returns a 32-hex id matching the header's trace field
  (extend the existing helper tests in `http.rs`).
- **`loadr-outputs`:** each backend adapter parses a **committed fixture**
  (`tempo.json`, `jaeger.json`, OTLP protobuf/JSON) into `Vec<ServerSpan>` with
  correct parent links and relative offsets — offline, table-driven like the
  existing `parse_matrix`/`parse_proc_stat` tests in `observe.rs`. A 404 / empty
  body yields `resolved: false`, not an error.
- **`loadr-cli`:** `rca_section` renders a waterfall from a synthetic
  `TraceAttachment`, emits no external asset references, and older summaries with
  no `traces` render unchanged — extend `renders_without_timeline`
  (`report_html.rs:483`).
- **Integration:** a wiremock-style stub backend serving a canned trace behind
  `resolve_traces`; assert the summary gains a resolved attachment. Real Tempo
  behind a feature-gated `#[ignore]` test only.

## 7. Docs / desktop UI / demo

- **Docs:** new `docs/src/observe/traces.md` (sibling to `system.md`), linked
  from `SUMMARY.md` and cross-linked from `docs/src/reporting.md`'s section list.
  Document the `type: traces` config, backend matrix, the tracing prerequisite,
  and the "in-server vs elsewhere" correlation readout.
- **Desktop UI** (`desktop/`, per MEMORY): the report is already rendered
  in-app; the new section is self-contained HTML so it appears for free. Add a
  backend-URL field to the run config form so desktop users needn't hand-edit
  YAML.
- **Demo:** a `loadr-demos`-style recipe (Go API + OTel SDK → local Tempo via
  docker-compose) that runs a load test and produces a report with a real server
  flamegraph — a strong VHS/screenshot asset for the site.

## 8. Milestones

- **M1 — Capture (smallest shippable, ~2 d):** return trace id from
  `make_traceparent`, add `ProtocolResponse.trace_id`, `RcaCollector`, and
  `Summary.rca_candidates`/`traces` fields. Ship: `--summary-export` now lists
  the slowest/errored trace ids for manual lookup. Immediately useful, no backend
  integration.
- **M2 — Tempo resolver (~3 d):** `ObserveConfig::Traces`, `resolve_traces`,
  Tempo adapter, wire into `run.rs` post-run block. Traces land in the summary.
- **M3 — Report waterfall (~3 d):** `rca_section` inline-SVG flamegraph + client
  vs server correlation marker. The headline feature.
- **M4 — Jaeger + OTLP adapters + docs/demo (~2 d):** breadth and the demo.

## 9. Risks & hard parts

- **Ingest race:** the backend may not have persisted a trace by the time loadr
  queries (Tempo flush lag). Mitigated by `lookback` padding and a short bounded
  retry; unresolved ids degrade gracefully rather than blocking.
- **trace_id ↔ span propagation gap:** loadr sets the *parent* traceparent, but
  the server must honour W3C tracecontext and export to the backend. If it
  doesn't, resolution is empty — documented as a hard prerequisite, surfaced as
  "trace not found", never a silent zero.
- **Cardinality restraint:** capturing must stay top-N + ring, never per-sample
  tagging, or high-RPS runs regress memory/throughput.
- **Backend API drift:** Tempo/Jaeger JSON shapes differ and evolve; isolate
  each behind its adapter with committed fixtures so a schema change is a
  one-file fix.
- **Clock skew** between generator and server makes the "elsewhere" delta noisy;
  present it as approximate, not authoritative.
