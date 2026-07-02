# SQL feeder plugin

> **Status:** planned — not yet in the signed [plugin index](installing.md). This
> page documents the intended shape; the config keys and metric names may still
> change before the first release.

`loadr-plugin-sql-feeder` is a **service** plugin in the *data sources &
feeders* role. Instead of driving a target, it acts as a **data source**: at the
start of a run it opens one connection, runs a single `SELECT` via
[`sqlx`](https://github.com/launchbadge/sqlx), materialises the result set in
memory, and hands each VU the next row through the usual feeder interpolation.
It turns a query against a live database into a `data:` feeder — the same shape
as a local `type: csv` file, but sourced from a table so the fixture stays next
to the system under test rather than being exported to the repo.

Reach for it when the data that drives a run already lives in a table — real
user IDs, order numbers, API keys, tenant slugs — and you would otherwise export
it to a CSV first. The feeder does that export for you, at run start, against the
live schema. The database is touched **once, at startup**; it is not on the
request hot path.

Like the [PostgreSQL](postgres.md) and [MySQL](mysql.md) protocol plugins, it is
near-pure Rust: `sqlx` built with **rustls** for TLS (no OpenSSL, no `libpq`),
gating only the driver features for the backends it serves — modelled on those
drivers, with the same connection-string handling and per-backend feature
gating.

The contract it uses is documented in
[Developing a plugin](developing.md#services).

## Install

`sql-feeder` will ship in the signed [plugin index](installing.md), so once
released you install it by name — no build toolchain required:

```bash
loadr plugin install sql-feeder
loadr plugin info sql-feeder
```

This resolves `sql-feeder` in the index, picks the artifact for your host
target, checks it against the plugin ABI your `loadr` build provides, downloads
it, verifies its sha256 and unpacks it into your plugins directory
(`~/.loadr/plugins/sql-feeder/`, or `$LOADR_PLUGINS_DIR`).

The installed manifest declares a service plugin:

```toml
[plugin]
name = "sql-feeder"
kind = "service"
type = "native"
entry = "libloadr_plugin_sql_feeder.so"   # .dylib on macOS, .dll on Windows
```

To run straight from a build tree instead, point the plan's `plugins:` entry at
the built artifact rather than resolving it by name:

```bash
cargo build -p loadr-plugin-sql-feeder --release
```

```yaml
plugins:
  - { name: sql-feeder, path: target/release/libloadr_plugin_sql_feeder.so }
```

## Use it in a test

List the plugin under `plugins:`, then declare a `data:` feeder whose
`service:` names it. The plugin's `config:` block carries the connection `url`
and the `query`; each row column the query returns binds a value the VUs
reference through the usual `${data.<name>.<column>}` interpolation — exactly
like a CSV feeder, but the rows come from a database. So a
`select id, email from users` exposes `${data.users.id}` and
`${data.users.email}`:

```yaml
plugins:
  - name: sql-feeder          # or: { name: sql-feeder, path: target/release/libloadr_plugin_sql_feeder.so }

data:
  users:
    service: sql-feeder       # this feeder is backed by the service plugin
    config:
      url: postgres://loadr:loadr@db.example.com:5432/loadr
      query: select id, email from users
    mode: shared              # all VUs share one cursor (per_vu: each VU gets its own)
    pick: sequential          # sequential | random | shuffle
    on_eof: recycle           # wrap around (stop: retire the VU at end of set)

scenarios:
  signup_replay:
    executor: constant-vus
    vus: 25
    duration: 2m
    flow:
      - request:
          name: fetch profile
          method: GET
          url: "https://api.example.com/users/${data.users.id}"
          headers: { X-User-Email: "${data.users.email}" }
          checks:
            - { type: status, equals: 200 }
```

The query runs **once**, before any VU starts; the request loop only reads from
the cached rows, so no per-VU database traffic happens during the test.

## Config reference

The feeder's behaviour is set through the plugin `config:` block:

| Key     | Required | Default | Meaning |
|---------|----------|---------|---------|
| `url`   | yes      | —       | Connection URI, e.g. `postgres://…` / `mysql://…`; passed straight to `sqlx` (any URL it accepts, including `?sslmode=require` for TLS). |
| `query` | yes      | —       | The `SELECT` to run once at startup; its column names become the feeder's field names. |

`${...}` interpolation works in `url` and `query`, so an environment variable or
`--env` value can supply the DSN (`url: "${env.DATABASE_URL}"`) without
hard-coding credentials in the plan.

Only row-producing statements make sense here: the query must return a result
set, and an empty `query` is rejected. Column values are carried as text into
the feeder, matching how CSV/JSON feeders present fields.

Standard feeder controls apply on top of `config:` — `mode` (shared / per-VU),
`pick` (`sequential` | `random` | `shuffle`) and `on_eof` (`recycle` | `stop`)
behave exactly as they do for a local CSV/JSON source. See
[Feeder strategies](../yaml/feeders.md).

## Metrics

Because the query runs once at startup rather than per request, the feeder does
not emit a per-request metric family. It records a single counter for the rows
it loaded:

| Metric | Kind | Meaning |
|--------|------|---------|
| `sql_feeder_rows` | counter | Rows fetched by the startup `SELECT` and loaded into the feeder. |

A load-time failure — an unreachable database, a bad DSN, or a query that
errors — fails the run at startup (before VUs begin) rather than surfacing as a
per-request failure, so there is no `sql_feeder_reqs` / `_req_duration` family.
Use `sql_feeder_rows` as a sanity check that the feeder was actually populated —
a `count>0` catches an empty or misparsed result set before the run leans on it:

```yaml
thresholds:
  sql_feeder_rows: [ "count>0" ]
```

## Notes

- **Fetched once, then in memory.** The whole result set is read at run start and
  cached, and the connection is closed before the load phase begins. The size of
  the set is bounded by available memory — scope the query with a `WHERE`/`LIMIT`
  rather than selecting an unbounded table.
- **Feeder, not a target.** This plugin *sources data*; it does not send load to
  the database. To put a database itself under test, use the
  [PostgreSQL](postgres.md) or [MySQL](mysql.md) protocol plugin, which run one
  query per request on the hot path.
- **Near-pure Rust.** `sqlx` is built with the **rustls** TLS backend and only
  the driver feature for the backends it serves, mirroring the postgres/mysql
  plugins — no OpenSSL or client-library system dependency, so the artifact is
  self-contained across platforms and installs by name with no build toolchain.
- **Synchronous ABI.** Like the other native plugins, it owns a single Tokio
  runtime and `block_on`s the async `sqlx` fetch at startup, because the plugin
  ABI is synchronous.
