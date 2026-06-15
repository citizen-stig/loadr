# The management UI

A built-in, RabbitMQ-style management interface — shipped as a first-party
**service plugin**, statically linked into the default binary.

```bash
loadr run --ui test.yaml                  # standalone: dashboard for this run
loadr controller --ui-bind 0.0.0.0:6464   # distributed: manage the whole fleet
```

Default address `127.0.0.1:6464` (loopback unless you bind otherwise —
deliberate security default).

## Pages

- **Overview** — live stat cards (RPS, active VUs, error rate, p95) and
  streaming charts (request rate, latency percentiles, errors), per-scenario
  table, threshold pass/fail pills, live check rates, and a **failure
  breakdown** panel (see below). Updates once per second over SSE.
- **Runs** — every run with state and outcome; a run page with live charts,
  the threshold table, scenario breakdown, and controls: **Stop** (graceful),
  **Kill**, **Pause/Resume**, and a VU dial for `externally-controlled`
  scenarios. Finished runs render the full summary (metric table, checks,
  thresholds).
- **Tests** — a test library: upload/edit YAML in the browser with
  line-numbered editing and one-click **Validate** (the same diagnostics as
  `loadr validate`, inline), then **Run**.
- **Agents** — the fleet: health, active VUs, cores, labels, last heartbeat.
- **Logs** — live tail of engine logs.

Dark mode is the default (there's a toggle; it remembers). No CDNs, no
trackers — the entire SPA is embedded in the binary.

## Failure breakdown

When a test produces failures, the **Failure breakdown** panel on the Overview
and live Run dashboards groups them by *cause* so you can see *why* requests
failed, not just *how many*. Four groups are shown, each row carrying its count
and share of the group, with a bar for quick scanning:

- **HTTP status** — failed responses (4xx/5xx) grouped by status code.
- **Transport / error** — connection-level failures grouped by a coarse kind
  (`timeout`, `dns`, `tls`, `connection_refused`, `connection_reset`,
  `connection`, `transport`) plus prepare/protocol/extraction errors.
- **Failed checks** — each [check](yaml/assertions-checks.md) that failed,
  by name, with the number of failing evaluations.
- **Script exceptions** — uncaught exceptions from JS hooks, `exec`
  functions, and `js` steps, grouped by a normalised message (volatile detail
  such as numbers and quoted strings is collapsed so the same logical error
  groups together).

High-cardinality groups are capped to the top causes with the remainder folded
into an **other** row.

### Downloading the breakdown

Two buttons in the panel header export the current breakdown entirely in the
browser — no server round-trip:

- **↓ CSV** — a `category,cause,count,share_pct` file
  (`loadr-failures-<timestamp>.csv`) ready for spreadsheets or further
  analysis.
- **↓ Report** — a self-contained HTML report
  (`loadr-failures-<timestamp>.html`) you can archive or share.

The breakdown is also available programmatically as the `failures` object on
the live metrics payload (see the `/api/overview` and `/api/runs/:id/stream`
responses).

## Authentication

```bash
loadr controller --ui-user admin --ui-password s3cret      # HTTP Basic
loadr controller --ui-token "$(openssl rand -hex 24)"      # bearer token(s)
```

Both may be active at once; SSE/WebSocket connections accept
`?token=`. Without any auth flags the UI is open — bind it to loopback or put
it behind your proxy.

## API

Everything the UI does is a JSON API you can script against:

```text
GET  /api/overview                 GET  /api/runs            POST /api/runs
GET  /api/runs/:id                 GET  /api/runs/:id/summary
GET  /api/runs/:id/stream (SSE)    POST /api/runs/:id/stop|pause|scale
GET  /api/agents                   GET/PUT/DELETE /api/tests[/:name]
POST /api/validate                 GET  /api/logs            GET /healthz
```
