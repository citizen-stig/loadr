# loadr Architecture

loadr is a single-binary load testing platform that combines k6's scriptable, metrics-first
model with JMeter's breadth of protocol support, assertions, and timers — implemented in Rust
on Tokio, extensible through WASM and native plugins, and operable standalone or as a
distributed controller/agent fleet.

## Workspace layout

```
loadr.io/
├── crates/
│   ├── loadr-core/        # Engine: scheduling, executors, VUs, metrics, thresholds, checks, lifecycle
│   ├── loadr-config/      # YAML schema (serde + schemars), validation, ${...} interpolation, env overrides
│   ├── loadr-js/          # Embedded JavaScript runtime (rquickjs) + k6-flavoured stdlib
│   ├── loadr-protocols/   # HTTP/1.1+2, WebSocket, gRPC, GraphQL, TCP, UDP clients with phase metrics
│   ├── loadr-plugin-api/  # Stable plugin ABI (abi_stable) + WASM WIT world + SDK helpers
│   ├── loadr-agent/       # Controller + agent: gRPC coordination, load partitioning, HDR merge
│   └── loadr-cli/         # The `loadr` binary: run, validate, convert, agent, controller, plugin, report
├── plugins/
│   ├── loadr-plugin-webui/        # RabbitMQ-style management UI (axum + rust-embed, service plugin)
│   └── examples/
│       ├── wasm-extractor/        # Example WASM extractor plugin (WIT component)
│       ├── wasm-assertion/        # Example WASM assertion plugin
│       ├── native-output/         # Example native (abi_stable) output plugin
│       └── native-protocol/       # Example native protocol plugin
├── testsupport/
│   └── loadr-testserver/  # In-repo HTTP/WS/gRPC echo servers used by integration tests
├── docs/                  # mdBook
├── examples/              # Runnable YAML test definitions
├── deploy/                # Dockerfile, docker-compose, k8s manifests, Helm chart, Grafana dashboard
└── .github/workflows/     # CI + release + docs deploy
```

## Key design decisions (ADR summaries — full ADRs in docs/src/adr/)

### ADR-001: JavaScript runtime — rquickjs (QuickJS), not deno_core (V8)

- **Startup & footprint.** A VU may create/tear down JS contexts thousands of times per run.
  QuickJS contexts are ~microseconds and ~KBs; V8 isolates are orders of magnitude heavier and
  drag in a multi-minute build plus snapshot machinery.
- **Synchronous execution model.** loadr iterations are written as straight-line code
  (`http.get(...)`, `check(...)`, `sleep(...)`). QuickJS lets us run user code synchronously on
  a blocking-friendly worker and bridge into Tokio for I/O, exactly like k6's goja approach.
  deno_core forces an async event-loop model that complicates deterministic pacing.
- **Sandboxing.** QuickJS exposes per-runtime memory limits and interrupt handlers; we enforce
  per-iteration wall-clock and memory budgets directly (`loadr-js::limits`).
- **Trade-off accepted:** no JIT, so raw JS throughput is lower than V8. Load tests are
  I/O-bound; scripting overhead is dwarfed by network time. This mirrors k6's choice of goja.

### ADR-002: Plugin system — dual mechanism

1. **WASM components (wasmtime + WIT)** for portable, sandboxed plugins. The interface lives in
   `crates/loadr-plugin-api/wit/loadr.wit` (world `loadr-plugin`). Extractors and assertions are
   pure functions over bytes — a perfect fit for components. Capability-safe: no FS/network
   unless granted.
2. **Native cdylib plugins (`abi_stable`)** for performance-critical or long-running plugins
   (protocols, outputs, services such as the web UI). `abi_stable` gives us a checked, versioned
   C ABI with Rust ergonomics (`#[sabi_trait]` objects, `RString`/`RVec` FFI-safe types) and
   layout validation at load time, so a mismatched plugin fails loudly instead of UB.

Plugin types: `protocol`, `output`, `extractor`, `assertion`, `service`. Discovery: a plugins
directory (`~/.loadr/plugins` or `--plugins-dir`) holding `*.wasm` and `*.so/.dylib/.dll` plus a
`plugin.toml` manifest; plugins are referenced by name from YAML (`plugins:` block). The web UI
ships as a first-party service plugin but is also statically linked into the default binary
(feature `webui`, on by default) so the single-binary story holds.

### ADR-003: Coordination protocol — gRPC (tonic) with mTLS option

Controller and agents speak `loadr.coordination.v1` (proto in `crates/loadr-agent/proto/`),
compiled at build time with **protox** (pure-Rust protoc — no system protoc dependency).
Bidirectional streaming RPC `AgentSession` carries: register → assignment (test definition +
data-file blobs + load partition) → synchronized start barrier → metric deltas (1 s cadence) →
control (pause/stop/scale) → drain/teardown. Heartbeats piggyback on the stream with a
server-side liveness window; agents reconnect with exponential backoff and resume by run-id.
Protocol is versioned via a `protocol_version` field checked at registration.

**Metric correctness:** agents serialize HDR histograms (V2 deflate encoding) and the controller
*merges* histograms — percentiles are computed only after merge, never averaged.

**Load partitioning:** closed models (VUs/iterations) split VU counts per agent weight with
round-robin remainder; open models (arrival rates) split target rates fractionally. Stage
schedules are identical on all agents; only magnitudes are scaled, so global ramps are exact.

### ADR-004: HTTP stack — hyper with a hand-rolled timing connector

reqwest hides connection phases. loadr builds on `hyper` + `tokio-rustls` with its own connector
so every request reports `dns`, `connect`, `tls_handshake`, `ttfb`, `duration`, `bytes_sent`,
`bytes_received`. Connections are pooled **per VU** (a VU models one user-agent: its own
keep-alive connections and cookie jar), with HTTP/2 multiplexing when negotiated via ALPN.
mTLS, custom CAs, redirect policies, proxy (CONNECT + absolute-form), and gzip/br/deflate
decompression are implemented in `loadr-protocols::http`.

### ADR-005: Metrics engine

Four metric kinds, k6-compatible: **Counter**, **Gauge**, **Rate**, **Trend** (HDR histogram,
3 significant figures, auto-resizing). Each sample carries a tag set; aggregation keys are
`(metric, sorted-tags)`. Built-in metrics mirror k6 (`http_req_duration`,
`http_req_connecting`, `http_req_tls_handshaking`, `http_req_waiting`, `http_reqs`, `vus`,
`iterations`, `checks`, `data_sent`, `data_received`, …) plus per-protocol families. Custom
metrics are declared in YAML or created from JS. Thresholds are parsed into an expression AST
(`p(99)<250`, `rate>0.95`, `count<100`, `avg<200`, with `abortOnFail` + `delayAbortEval`) and
evaluated continuously; failures set exit code 99 (k6-compatible) and can abort the run.

### ADR-006: Executor model

All seven k6 executors are implemented in `loadr-core::executor` against one `VuPool`
abstraction. Closed models (`constant-vus`, `ramping-vus`, `per-vu-iterations`,
`shared-iterations`) drive iterations from VU loops; open models (`constant-arrival-rate`,
`ramping-arrival-rate`) drive a token clock that starts iterations on schedule regardless of
completion, growing allocated VUs up to `maxVUs` and recording `dropped_iterations` when
starved. `externally-controlled` exposes a control handle (used by the CLI REST control socket,
the web UI, and the controller). Scenarios run concurrently with independent `startTime`,
`gracefulStop`/`gracefulRampDown`, and executor-specific options.

## YAML schema (summary — full JSON Schema generated by `loadr schema`, reference in docs)

```yaml
name: checkout-flow
description: Browse + checkout under load

defaults:
  http:
    base_url: https://shop.example.com
    headers: { User-Agent: loadr/1.0 }
    timeout: 30s

env:                # environment overrides: loadr run -e staging
  staging:
    defaults: { http: { base_url: https://staging.shop.example.com } }

variables:
  api_key: ${env.API_KEY}        # ${...} interpolation: env., data., vars., js:, extracted names

data:
  users: { type: csv, path: users.csv, mode: shared, on_eof: recycle }

js:
  file: ./helpers.js             # or inline: `script: |`

scenarios:
  browse:
    executor: ramping-vus
    start_vus: 0
    stages: [ { duration: 2m, target: 50 }, { duration: 5m, target: 50 } ]
    flow:
      - request:
          name: home
          method: GET
          url: /
          extract:
            - { name: csrf, type: regex, expression: 'csrf" value="([^"]+)' }
          assert:
            - { type: status, equals: 200 }
            - { type: body_contains, value: "Welcome" }
          checks:
            - { name: "home fast", type: duration, max: 500ms }
      - think_time: { type: uniform, min: 1s, max: 3s }
      - js: "session.counterAdd('pages_viewed', 1)"

thresholds:
  http_req_duration: [ "p(95)<400", { threshold: "p(99)<800", abort_on_fail: true } ]
  checks: [ "rate>0.99" ]

outputs:
  - { type: prometheus, listen: 0.0.0.0:9090 }
  - { type: json, path: results.jsonl }
```

Durations accept `300ms`/`2s`/`5m`/`1h30m`. Validation produces line/column diagnostics with
did-you-mean suggestions (`loadr validate`).

## Execution data flow

```
YAML ──parse/validate──▶ TestPlan ──compile──▶ ScenarioPrograms
                                         │
        Scheduler ◀─ executors ─ VuPool ─┘     (per scenario)
            │ iteration
            ▼
   FlowInterpreter ── steps ──▶ ProtocolClient (per VU state: cookies, conns, extracted vars)
            │                        │
            │ js hooks               ▼ samples
            ▼                   MetricsBus ──▶ Aggregator ──▶ thresholds / outputs / web UI / controller stream
       JsWorker (rquickjs)
```

The `MetricsBus` is an mpsc fan-in when a configured output consumes raw samples; when none
does, VUs record straight into shard-local aggregators instead, drained into the primary
aggregator once per snapshot tick. The `Aggregator` snapshots every second for live consumers
(web UI SSE, controller stream, console progress) and finalizes into the end-of-run summary
(console table + `--summary-export` JSON).

## Distributed mode

`loadr controller` runs the coordination gRPC server, the REST/UI control plane (web UI plugin),
and the central aggregator. `loadr agent --join host:port` registers and waits. A test submitted
to the controller (CLI or UI) is partitioned, shipped (definition + data files inline in the
assignment message), started via a barrier timestamp, and aggregated live. Agent loss triggers
configurable policy: `continue` (default, remaining agents keep their share) or `abort`.

## Security posture

Follows the project constitution: no plaintext secrets (secrets come from env or files), TLS
everywhere it's offered (coordination mTLS, web UI auth via basic/token), listeners default to
localhost except where explicitly configured, plugins are sandboxed (WASM) or layout-validated
(native), JS is resource-limited per iteration.
