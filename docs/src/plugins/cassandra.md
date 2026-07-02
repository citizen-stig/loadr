# Cassandra / ScyllaDB plugin

> **Status:** planned — not yet shipped in the [plugin index](installing.md).
> This page documents the intended shape; the scheme, block names and metric
> names below are the target design and may shift before release.

`loadr-plugin-cassandra` adds **Apache Cassandra** and **ScyllaDB** as a
load-test target. It is a **native protocol plugin** (`kind: protocol` — a
*Protocol adapter*): Cassandra support is not built into loadr core. The plugin
carries a full **CQL binary-protocol** client — session management, prepared
statements and result paging — inside its own dynamic library, and each request
runs **one prepared + bound statement**, its bind values coming from the
request's `cql` block (or `body`). Once installed, a request to a `cql://` URL
routes straight to this plugin — no explicit `protocol:` needed.

Unlike the SQL adapters, which keep one connection pool per URI, this plugin
holds a **CQL session per VU**: each virtual user opens its own session against
the cluster and reuses it for the life of the run, which matches how the native
driver load-balances requests across cluster nodes.

The contract it uses is documented in
[Developing a plugin](developing.md#native-protocol-plugins).

## When to use

Reach for this when the thing under test *is* the cluster: validating a table or
partition-key design under write pressure, sizing a keyspace's replication,
finding the point at which a query falls over, or proving latency holds at a
steady query rate against Cassandra or ScyllaDB. For an application that merely
*uses* Cassandra behind an HTTP API, test the API with the `http` handler
instead.

## Why it is a plugin (and not in core)

Speaking CQL properly needs a **full session/paging client** — a heavy native
dependency that is not near-pure-Rust and does not build cleanly for every
target loadr ships. Keeping it in a separate, opt-in native library means the
core binary stays lean and portable, and only users who target Cassandra pull
the driver in.

## Install

Once released, `cassandra` will ship in the signed plugin index, so the
one-line install resolves it by name, picks the artifact for your host target,
verifies its sha256, and drops it into your plugins directory:

```bash
loadr plugin install cassandra
loadr plugin info cassandra
```

Installed layout (`~/.loadr/plugins/cassandra/`, or `$LOADR_PLUGINS_DIR`) holds
`plugin.toml` next to the platform artifact. The manifest declares the URL
scheme the plugin serves:

```toml
[plugin]
name = "cassandra"
kind = "protocol"
type = "native"
entry = "libloadr_plugin_cassandra.so"   # .dylib on macOS, .dll on Windows
schemes = ["cql"]
```

Because the driver is a heavy native dependency, prebuilt artifacts are
published only for the targets it builds cleanly on; on other hosts, build from
source:

```bash
cargo build -p loadr-plugin-cassandra --release

mkdir -p dist
cp plugins/loadr-plugin-cassandra/plugin.toml dist/
cp target/release/libloadr_plugin_cassandra.so dist/   # .dylib / .dll elsewhere
loadr plugin install ./dist
```

## The target URL

```
cql://host[:port]/keyspace
```

- **scheme** is `cql`.
- **port** defaults to the CQL native-protocol port (`9042`).
- **keyspace** — the path segment selects the keyspace the session uses, so
  statements can name tables unqualified. `cql://db.example.com:9042/loadr`
  binds the session to the `loadr` keyspace.

## Use it in a test

List the plugin under `plugins:` and target a `cql://` URL. The statement and
its bind values go in the request's **`cql`** block:

```yaml
plugins:
  - name: cassandra    # or: { name: cassandra, path: target/release/libloadr_plugin_cassandra.so }

scenarios:
  main:
    executor: constant-vus
    vus: 20
    duration: 30s
    flow:
      - request:
          name: read user
          url: cql://db.example.com:9042/loadr
          cql:
            query: SELECT id, email FROM users WHERE id = ?
            params: ["${vu}"]
          checks:
            - { type: status, equals: 1 }                 # 1 = ok, 0 = CQL error
            - { type: duration, name: query is fast, max: 250ms }

      - request:
          name: write event
          url: cql://db.example.com:9042/loadr
          cql:
            query: INSERT INTO events (id, kind, at) VALUES (?, ?, toTimestamp(now()))
            params: ["${row.id}", "${row.kind}"]
          assert:
            - { type: status, equals: 1 }
```

## Expressing the statement

- **`query`** — the CQL to run. Use CQL's positional `?` placeholder syntax for
  bind values. The plugin **prepares** the statement (caching the prepared id
  per session) and binds against it, so the same query text is prepared once per
  VU and reused.
- **`params`** — positional bind values, bound *safely* by the driver (never
  string-spliced, so there is no injection surface). Each value is given as
  text; the plugin infers a type so comparisons against typed columns work — a
  value that parses as an integer binds as an integer, a decimal as a float,
  everything else as text.

`${...}` interpolation works in both `query` and `params`, so per-VU values and
data-feed columns flow straight into the statement. As a shorthand, a request
with no `cql` block uses its **`body`** as the statement text (no parameters);
an empty statement is rejected.

## Config reference

| Field | Where | Meaning |
|-------|-------|---------|
| `url` | request | `cql://host[:port]/keyspace` — selects host, port and session keyspace |
| `cql.query` | request | the statement, with positional `?` placeholders; prepared per session |
| `cql.params` | request | positional bind values (text; type inferred), interpolation-aware |
| `body` | request | fallback statement text when no `cql` block is present (no params) |
| `[config]` | `plugin.toml` | none required — all connection details come from the request URL |

## Status, rows, and errors

A request **succeeds** when the statement executes without a CQL error:

| Outcome | `status` | `error` | `extras.rows` |
|---------|----------|---------|---------------|
| SELECT (row-producing) | `1` | — | rows returned |
| INSERT / UPDATE / DELETE | `1` | — | `0` (CQL writes report no row count) |
| CQL error (bad statement, unavailable, …) | `0` | the CQL message | — |
| connection failure / timeout | `0` | the transport error | — |

`extras` carries:

- `extras.backend` — `cassandra`.
- `extras.rows` — rows returned for row-producing statements.

The response **body** is the row count rendered as text, so `body`-based checks
and extraction still work.

## Metrics

loadr turns the plugin's response into the `cassandra` metric family:

| Metric | Kind | Meaning |
|--------|------|---------|
| `cassandra_reqs` | counter | statements executed (one per request) |
| `cassandra_req_duration` | trend (time) | per-statement round-trip latency (ms) |

A request is marked **failed** when the statement errors (response `status` 0),
so `http_req_failed` (the shared failure-rate metric) tracks CQL errors too, and
`checks` / `assert` entries can gate on `status` (1 = ok).

```yaml
thresholds:
  checks: [ "rate>0.99" ]
  cassandra_req_duration: [ "p(95)<100ms" ]
```

## Notes

- **Session per VU.** Each VU opens and holds its own CQL session for the run,
  rather than sharing a single pool. The native driver already spreads a
  session's requests across the cluster's nodes, so a session per VU maps VU
  concurrency onto the driver's own connection management without contending on
  a shared pool.
- **Prepared statements.** A statement is prepared once per session and the
  prepared id is reused for every subsequent request with the same query text,
  so only bind values cross the wire on the hot path.
- **Heavy native dependency.** The full session/paging client is why this ships
  as an opt-in plugin rather than in core; prebuilt artifacts are limited to the
  targets it builds cleanly on, with source builds available elsewhere.
- **Cassandra and ScyllaDB.** ScyllaDB is CQL-wire-compatible, so the same
  `cql://` URL and `cql` block target either — point the URL at a Scylla node.
